//! Tracker's application core.
//!
//! [`App`] owns every open video session and is the single command layer used by both the
//! native UI and AI agents (through the control socket and the MCP bridge). There is no
//! separate agent model: an agent's `mark_point` is the same call the UI makes on a click.
//!
//! Locking: one mutex guards the workspace. Decoding, tracking and file I/O never run while it
//! is held; worker threads lock briefly to deliver results.

mod automation;
mod jobs;
mod preview;
mod session;

pub use automation::{AutomationServer, handle_request};
pub use jobs::{JobKind, JobRecord, JobState};
pub use preview::render_png;
pub use session::{DisplayFrame, PendingReview, Session, SessionStatus};

use crossbeam_channel::{Receiver, Sender, unbounded};
use parking_lot::{Condvar, Mutex, MutexGuard};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tracker_core::{Calibration, CoordinateSystem, FrameStamp, LengthUnit, Measurement, Point, Project, Track, TrackId, TrackingSettings};
use tracker_dataset::{Dataset, Observation, RunRecord};
pub use tracker_ipc::ErrorBody as AppError;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, AppError>;

pub(crate) fn fail<T>(code: &str, message: impl Into<String>) -> Result<T> {
    Err(AppError::new(code, message))
}

/// Something changed; the UI redraws.
#[derive(Clone, Debug, PartialEq)]
pub enum AppEvent {
    Workspace,
    Session(Uuid),
    Display(Uuid),
    Job(Uuid),
    Activity,
    Control,
}

#[derive(Clone, Debug)]
pub struct Activity {
    pub at: SystemTime,
    /// "You" for the native UI, otherwise the agent's client name.
    pub actor: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct AgentClient {
    pub name: String,
    pub last_seen: Instant,
}

pub(crate) struct State {
    pub sessions: Vec<Session>,
    pub selected: Option<Uuid>,
    pub jobs: jobs::JobTable,
    pub activity: VecDeque<Activity>,
    pub control_enabled: bool,
    pub clients: HashMap<Uuid, AgentClient>,
    pub requests: automation::RequestCache,
}

pub(crate) struct Shared {
    pub state: Mutex<State>,
    /// Notified on every job update (for long-polling `get_job`).
    pub job_changed: Condvar,
    pub subscribers: Mutex<Vec<Sender<AppEvent>>>,
    pub datasets_root: PathBuf,
    pub slots: usize,
}

/// Cheap to clone; every clone refers to the same workspace.
#[derive(Clone)]
pub struct App {
    pub(crate) shared: Arc<Shared>,
}

pub struct AppConfig {
    pub datasets_root: PathBuf,
    /// Tracking jobs that may run at once across all sessions.
    pub tracking_slots: usize,
    pub agent_control: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        Self { datasets_root: Dataset::default_root(), tracking_slots: (cores / 2).max(1), agent_control: false }
    }
}

const ACTIVITY_LIMIT: usize = 200;

impl App {
    pub fn new(config: AppConfig) -> Self {
        let state = State { sessions: vec![], selected: None, jobs: jobs::JobTable::default(), activity: VecDeque::new(), control_enabled: config.agent_control, clients: HashMap::new(), requests: Default::default() };
        Self { shared: Arc::new(Shared { state: Mutex::new(state), job_changed: Condvar::new(), subscribers: Mutex::new(vec![]), datasets_root: config.datasets_root, slots: config.tracking_slots.max(1) }) }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.shared.state.lock()
    }

    /// Receives every change notification from now on.
    pub fn subscribe(&self) -> Receiver<AppEvent> {
        let (tx, rx) = unbounded();
        self.shared.subscribers.lock().push(tx);
        rx
    }

    pub(crate) fn emit(&self, e: AppEvent) {
        self.shared.subscribers.lock().retain(|s| s.send(e.clone()).is_ok());
    }

    pub fn datasets_root(&self) -> &Path {
        &self.shared.datasets_root
    }
    pub fn tracking_slots(&self) -> usize {
        self.shared.slots
    }

    pub fn log(&self, actor: &str, text: impl Into<String>) {
        {
            let mut s = self.lock();
            s.activity.push_front(Activity { at: SystemTime::now(), actor: actor.to_string(), text: text.into() });
            s.activity.truncate(ACTIVITY_LIMIT);
        }
        self.emit(AppEvent::Activity);
    }

    pub fn activity(&self, limit: usize) -> Vec<Activity> {
        self.lock().activity.iter().take(limit).cloned().collect()
    }

    // ---- agent control -------------------------------------------------------------------

    pub fn control_enabled(&self) -> bool {
        self.lock().control_enabled
    }
    pub fn set_control_enabled(&self, enabled: bool) {
        {
            let mut s = self.lock();
            s.control_enabled = enabled;
            if !enabled {
                s.clients.clear();
            }
        }
        self.log("You", if enabled { "Allowed agent control" } else { "Stopped agent control" });
        self.emit(AppEvent::Control);
    }
    /// Agents seen within the last 30 seconds.
    pub fn connected_agents(&self) -> Vec<String> {
        let mut s = self.lock();
        s.clients.retain(|_, c| c.last_seen.elapsed().as_secs() < 30);
        let mut names: Vec<_> = s.clients.values().map(|c| c.name.clone()).collect();
        names.sort();
        names
    }

    // ---- sessions ------------------------------------------------------------------------

    pub fn session_ids(&self) -> Vec<Uuid> {
        self.lock().sessions.iter().map(|s| s.id).collect()
    }
    pub fn selected(&self) -> Option<Uuid> {
        self.lock().selected
    }
    pub fn select(&self, id: Uuid) -> Result<()> {
        let mut s = self.lock();
        if !s.sessions.iter().any(|x| x.id == id) {
            return fail("session_not_found", "No open session with this ID.");
        }
        s.selected = Some(id);
        for x in s.sessions.iter_mut() {
            if x.id != id {
                x.stop_playback();
            }
        }
        drop(s);
        self.emit(AppEvent::Workspace);
        Ok(())
    }

    /// Runs `f` with read access to a session.
    pub fn with_session<R>(&self, id: Uuid, f: impl FnOnce(&Session) -> R) -> Result<R> {
        let s = self.lock();
        let session = s.sessions.iter().find(|x| x.id == id).ok_or_else(|| AppError::new("session_not_found", "No open session with this ID."))?;
        Ok(f(session))
    }

    pub(crate) fn with_session_mut<R>(&self, id: Uuid, f: impl FnOnce(&mut Session) -> Result<R>) -> Result<R> {
        let mut s = self.lock();
        let session = s.sessions.iter_mut().find(|x| x.id == id).ok_or_else(|| AppError::new("session_not_found", "No open session with this ID."))?;
        let r = f(session);
        drop(s);
        if r.is_ok() {
            self.emit(AppEvent::Session(id));
        }
        r
    }

    /// Opens a video. Returns the session and the import job; indexing runs in the background.
    pub fn import_video(&self, path: &Path) -> Result<(Uuid, Uuid)> {
        if !path.is_absolute() {
            return fail("invalid_path", "Use an absolute path.");
        }
        if !path.exists() {
            return fail("file_not_found", format!("{} does not exist.", path.display()));
        }
        if let Some(existing) = self.lock().sessions.iter().find(|s| s.path == path).map(|s| s.id) {
            self.select(existing)?;
            let job = self.lock().jobs.latest_import(existing);
            if let Some(job) = job {
                return Ok((existing, job));
            }
        }
        let session = Session::loading(path.to_path_buf(), None);
        let id = session.id;
        let job = {
            let mut s = self.lock();
            s.sessions.push(session);
            s.selected = Some(id);
            s.jobs.insert(JobRecord::new(JobKind::Import, id, None))
        };
        self.emit(AppEvent::Workspace);
        let app = self.clone();
        let path = path.to_path_buf();
        std::thread::spawn(move || app.finish_import(id, job, path, None));
        Ok((id, job))
    }

    /// Opens a saved project (Rust v2 or Swift v1). Returns the session and the import job.
    pub fn open_project(&self, project_path: &Path, relink: Option<&Path>) -> Result<(Uuid, Uuid)> {
        let project = Project::read(project_path).map_err(|e| AppError::new("invalid_project", e.to_string()))?;
        let source = match relink {
            Some(p) if p.exists() => p.to_path_buf(),
            Some(p) => return fail("file_not_found", format!("{} does not exist.", p.display())),
            None => project.resolve_source(project_path).ok_or_else(|| AppError::new("relink_required", format!("Missing source footage: {}. Pass relink_path with the original video.", project.source.absolute.display())))?,
        };
        let session = Session::loading(source.clone(), Some(project_path.to_path_buf()));
        let id = session.id;
        let job = {
            let mut s = self.lock();
            s.sessions.push(session);
            s.selected = Some(id);
            s.jobs.insert(JobRecord::new(JobKind::Import, id, None))
        };
        self.emit(AppEvent::Workspace);
        let app = self.clone();
        let relinked = relink.is_some();
        std::thread::spawn(move || app.finish_import(id, job, source, Some((project, relinked))));
        Ok((id, job))
    }

    fn finish_import(&self, id: Uuid, job: Uuid, path: PathBuf, project: Option<(Project, bool)>) {
        self.update_job(job, |j| j.state = JobState::Running);
        let opened = tracker_media::open(&path).map_err(|e| AppError::new("media_error", e.to_string())).and_then(|info| {
            if let Some((p, relinked)) = &project {
                if (p.source.width - info.width as f64).abs() > 0.5 || (p.source.height - info.height as f64).abs() > 0.5 {
                    return fail("source_mismatch", "The video's dimensions differ from the project. Relink the original footage.");
                }
                if let (Some(saved), Ok(now)) = (&p.source.fingerprint, tracker_core::io::fingerprint(&path)) {
                    if *saved != now && !relinked {
                        return fail("source_mismatch", "The video file changed since the project was saved. Pass relink_path to confirm the original footage.");
                    }
                }
            }
            Ok(info)
        });
        let fingerprint = tracker_core::io::fingerprint(&path).ok();
        let result = self.with_session_mut(id, |s| {
            match &opened {
                Ok(info) => s.make_ready(Arc::new(info.clone()), project.as_ref().map(|(p, _)| p.clone()), fingerprint),
                Err(e) => s.status = SessionStatus::Failed(e.message.clone()),
            }
            Ok(())
        });
        match (opened, result) {
            (Ok(_), Ok(())) => {
                let desc = self.describe(id).unwrap_or(Value::Null);
                self.update_job(job, |j| {
                    j.state = JobState::Completed;
                    j.result = desc;
                });
                self.request_display(id);
            }
            (Err(e), _) | (_, Err(e)) => self.update_job(job, |j| {
                j.state = JobState::Failed;
                j.error = Some(e);
            }),
        }
    }

    /// Closes a session. Unsaved analysis is refused unless `discard`.
    pub fn close(&self, id: Uuid, discard: bool) -> Result<()> {
        let mut s = self.lock();
        let i = s.sessions.iter().position(|x| x.id == id).ok_or_else(|| AppError::new("session_not_found", "No open session with this ID."))?;
        if s.sessions[i].is_dirty() && !discard {
            return fail("unsaved_changes", "This session has unsaved changes. Save it, or pass discard=true.");
        }
        let jobs: Vec<Uuid> = s.jobs.active_for(id);
        for j in jobs {
            s.jobs.cancel(j);
        }
        let mut session = s.sessions.remove(i);
        session.stop_playback();
        if s.selected == Some(id) {
            s.selected = s.sessions.last().map(|x| x.id);
        }
        drop(s);
        self.shared.job_changed.notify_all();
        self.emit(AppEvent::Workspace);
        Ok(())
    }

    // ---- viewer --------------------------------------------------------------------------

    /// Moves the viewer to the frame displayed at `seconds`. Returns that frame.
    pub fn seek(&self, id: Uuid, seconds: f64) -> Result<FrameStamp> {
        let stamp = self.with_session_mut(id, |s| {
            let info = s.ready()?.clone();
            let stamp = info.index.stamp_at(seconds).ok_or_else(|| AppError::new("invalid_time", "Time is outside this video."))?;
            s.stop_playback();
            s.viewer = stamp;
            Ok(stamp)
        })?;
        self.request_display(id);
        Ok(stamp)
    }

    pub fn step(&self, id: Uuid, frames: isize) -> Result<FrameStamp> {
        let target = self.with_session(id, |s| s.info.as_ref().and_then(|i| i.index.step(s.viewer, frames)))?.ok_or_else(|| AppError::new("session_loading", "The video is still loading."))?;
        self.seek(id, target.seconds())
    }

    /// Decodes the viewer frame in the background and publishes it for display.
    pub fn request_display(&self, id: Uuid) {
        let Ok(Some((info, stamp, generation, cached))) = self.with_session_mut(id, |s| {
            let Some(info) = s.info.clone() else { return Ok(None) };
            s.display_generation += 1;
            if let Some(f) = s.cache_get(s.viewer) {
                s.display = Some(f);
                return Ok(Some((info, s.viewer, s.display_generation, true)));
            }
            Ok(Some((info, s.viewer, s.display_generation, false)))
        }) else {
            return;
        };
        if cached {
            self.emit(AppEvent::Display(id));
            return;
        }
        let app = self.clone();
        std::thread::spawn(move || {
            let frame = tracker_media::frame_at(&info, stamp, true);
            let _ = app.with_session_mut(id, |s| {
                if s.display_generation != generation {
                    return Ok(());
                }
                match frame {
                    Ok(f) => {
                        let d = Arc::new(DisplayFrame { stamp: f.stamp, width: f.luma.width, height: f.luma.height, rgba: f.rgba.unwrap_or_default() });
                        s.cache_put(d.clone());
                        s.display = Some(d);
                        s.display_error = None;
                    }
                    Err(e) => s.display_error = Some(e.to_string()),
                }
                Ok(())
            });
            app.emit(AppEvent::Display(id));
        });
    }

    /// Plays from the viewer frame at source timing until paused or the clip ends.
    pub fn play(&self, id: Uuid) -> Result<()> {
        let (info, start, stop, end) = self.with_session_mut(id, |s| {
            let info = s.ready()?.clone();
            s.stop_playback();
            let mut start = s.viewer;
            if Some(start) == info.index.last() {
                start = info.index.first().unwrap();
            }
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            s.playing = Some(stop.clone());
            Ok((info, start, stop, s.history.state().clip.end))
        })?;
        let app = self.clone();
        std::thread::spawn(move || {
            use std::sync::atomic::Ordering;
            let Ok(mut reader) = tracker_media::FrameReader::new(&info, start, true) else { return };
            let clock = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let Ok(Some(f)) = reader.next_frame() else { break };
                if f.stamp < start || end.is_some_and(|e| f.stamp > e) {
                    if f.stamp < start {
                        continue;
                    }
                    break;
                }
                let due = f.stamp.seconds() - start.seconds();
                let wait = due - clock.elapsed().as_secs_f64();
                if wait > 0.0 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(wait));
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let _ = app.with_session_mut(id, |s| {
                    s.viewer = f.stamp;
                    s.display = Some(Arc::new(DisplayFrame { stamp: f.stamp, width: f.luma.width, height: f.luma.height, rgba: f.rgba.clone().unwrap_or_default() }));
                    Ok(())
                });
                app.emit(AppEvent::Display(id));
            }
            let _ = app.with_session_mut(id, |s| {
                if s.playing.as_ref().is_some_and(|p| Arc::ptr_eq(p, &stop)) {
                    s.playing = None;
                }
                Ok(())
            });
            app.request_display(id);
        });
        Ok(())
    }

    pub fn pause(&self, id: Uuid) {
        let _ = self.with_session_mut(id, |s| {
            s.stop_playback();
            Ok(())
        });
    }

    pub fn set_selected_track(&self, id: Uuid, track: Option<TrackId>) -> Result<()> {
        // View state only: no revision change, no undo step, no job cancellation.
        self.with_session_mut(id, |s| {
            s.selected_track = track;
            Ok(())
        })
    }

    // ---- analysis edits (all undoable, all bump the revision) ---------------------------

    fn edit(&self, id: Uuid, expected: Option<u64>, label: &str, cancel_track: Option<TrackId>, f: impl FnOnce(&mut Session) -> Result<()>) -> Result<u64> {
        let revision = self.with_session_mut(id, |s| {
            s.ready()?;
            if let Some(e) = expected {
                if e != s.revision() {
                    return fail("stale_revision", format!("Session revision is {} (you sent {e}). Inspect it again before editing.", s.revision()));
                }
            }
            let _ = label;
            f(s)?;
            Ok(s.revision())
        })?;
        if let Some(t) = cancel_track {
            // An edit to a track invalidates a run that is still writing to it.
            let jobs = self.lock().jobs.active_for_track(id, t);
            for j in jobs {
                let _ = self.cancel_job(j);
            }
        }
        Ok(revision)
    }

    pub fn add_track(&self, id: Uuid, expected: Option<u64>, name: Option<&str>) -> Result<(TrackId, u64)> {
        let mut new_id = Uuid::nil();
        let rev = self.edit(id, expected, "Add track", None, |s| {
            let n = s.history.state().tracks.len();
            let track = Track::new(name.map(str::to_string).unwrap_or_else(|| format!("mass {}", n + 1)), n);
            new_id = track.id;
            s.history.edit("Add track", |a| a.tracks.push(Arc::new(track)));
            s.selected_track = Some(new_id);
            Ok(())
        })?;
        Ok((new_id, rev))
    }

    pub fn rename_track(&self, id: Uuid, expected: Option<u64>, track: TrackId, name: &str) -> Result<u64> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return fail("invalid_argument", "Track names must be 1 to 80 characters.");
        }
        self.edit(id, expected, "Rename track", None, |s| {
            s.require_track(track)?;
            s.history.edit("Rename track", |a| a.track_mut(track).unwrap().name = name.to_string());
            Ok(())
        })
    }

    pub fn delete_track(&self, id: Uuid, expected: Option<u64>, track: TrackId) -> Result<u64> {
        self.edit(id, expected, "Delete track", Some(track), |s| {
            s.require_track(track)?;
            s.history.edit("Delete track", |a| a.tracks.retain(|t| t.id != track));
            if s.selected_track == Some(track) {
                s.selected_track = s.history.state().tracks.first().map(|t| t.id);
            }
            if s.review.as_ref().is_some_and(|r| r.track_id == track) {
                s.review = None;
            }
            Ok(())
        })
    }

    /// Records a manual mark on the frame displayed at `seconds`. Returns (frame, revision).
    pub fn mark_point(&self, id: Uuid, expected: Option<u64>, track: TrackId, seconds: f64, pixel: Point) -> Result<(FrameStamp, u64)> {
        let mut frame = FrameStamp::ZERO;
        let rev = self.edit(id, expected, "Mark point", Some(track), |s| {
            let info = s.ready()?.clone();
            s.require_track(track)?;
            if !pixel.inside(info.width as f64, info.height as f64) {
                return fail("invalid_coordinate", "The point must lie inside the orientation-corrected source image.");
            }
            frame = info.index.stamp_at(seconds).ok_or_else(|| AppError::new("invalid_time", "Time is outside this video."))?;
            s.history.edit("Mark point", |a| {
                a.track_mut(track).unwrap().put(Measurement::manual(frame, pixel), false);
            });
            Ok(())
        })?;
        Ok((frame, rev))
    }

    pub fn remove_point(&self, id: Uuid, expected: Option<u64>, track: TrackId, seconds: f64) -> Result<u64> {
        self.edit(id, expected, "Remove point", Some(track), |s| {
            let info = s.ready()?.clone();
            let frame = info.index.stamp_at(seconds).ok_or_else(|| AppError::new("invalid_time", "Time is outside this video."))?;
            if s.require_track(track)?.point_at(frame).is_none() {
                return fail("point_not_found", "This track has no point on that frame.");
            }
            s.history.edit("Remove point", |a| {
                a.track_mut(track).unwrap().remove_at(frame);
            });
            Ok(())
        })
    }

    pub fn set_calibration(&self, id: Uuid, expected: Option<u64>, first: Point, second: Point, length: f64, unit: &str) -> Result<u64> {
        let unit = LengthUnit::parse(unit).ok_or_else(|| AppError::new("invalid_calibration", "Unit must be m, cm or mm."))?;
        let c = Calibration { first, second, length, unit };
        self.edit(id, expected, "Set scale", None, |s| {
            let info = s.ready()?.clone();
            if !c.is_valid() || !first.inside(info.width as f64, info.height as f64) || !second.inside(info.width as f64, info.height as f64) {
                return fail("invalid_calibration", "Use two distinct endpoints inside the image and a positive known length.");
            }
            s.history.edit("Set scale", |a| a.coords.calibration = Some(c));
            Ok(())
        })
    }

    pub fn clear_calibration(&self, id: Uuid, expected: Option<u64>) -> Result<u64> {
        self.edit(id, expected, "Clear scale", None, |s| {
            s.history.edit("Clear scale", |a| a.coords.calibration = None);
            Ok(())
        })
    }

    pub fn set_axes(&self, id: Uuid, expected: Option<u64>, origin: Point, angle_deg: f64) -> Result<u64> {
        if !origin.is_finite() || !angle_deg.is_finite() {
            return fail("invalid_argument", "Origin and angle must be finite numbers.");
        }
        self.edit(id, expected, "Move axes", None, |s| {
            s.history.edit("Move axes", |a| {
                a.coords = CoordinateSystem { origin, angle: angle_deg.to_radians(), calibration: a.coords.calibration };
            });
            Ok(())
        })
    }

    pub fn set_clip(&self, id: Uuid, expected: Option<u64>, start: Option<f64>, end: Option<f64>, step: Option<u32>) -> Result<u64> {
        self.edit(id, expected, "Clip settings", None, |s| {
            let info = s.ready()?.clone();
            let start = start.map(|t| info.index.stamp_at(t).ok_or_else(|| AppError::new("invalid_time", "start_time is outside the video."))).transpose()?;
            let end = end.map(|t| info.index.stamp_at(t).ok_or_else(|| AppError::new("invalid_time", "end_time is outside the video."))).transpose()?;
            if let (Some(a), Some(b)) = (start, end) {
                if a >= b {
                    return fail("invalid_argument", "start_time must be before end_time.");
                }
            }
            let step = step.unwrap_or(s.history.state().clip.step).max(1);
            if step > 1000 {
                return fail("invalid_argument", "step must be 1 to 1000.");
            }
            s.history.edit("Clip settings", |a| a.clip = tracker_core::Clip { start, end, step });
            Ok(())
        })
    }

    pub fn configure_tracking(&self, id: Uuid, expected: Option<u64>, settings: TrackingSettings) -> Result<u64> {
        settings.validate().map_err(|m| AppError::new("invalid_settings", m))?;
        self.with_session_mut(id, |s| {
            if let Some(e) = expected {
                if e != s.revision() {
                    return fail("stale_revision", format!("Session revision is {} (you sent {e}).", s.revision()));
                }
            }
            if s.settings != settings {
                s.settings = settings;
                s.meta_revision += 1;
            }
            Ok(s.revision())
        })
    }

    pub fn undo(&self, id: Uuid, expected: Option<u64>) -> Result<(String, u64)> {
        self.undo_redo(id, expected, true)
    }
    pub fn redo(&self, id: Uuid, expected: Option<u64>) -> Result<(String, u64)> {
        self.undo_redo(id, expected, false)
    }
    fn undo_redo(&self, id: Uuid, expected: Option<u64>, undo: bool) -> Result<(String, u64)> {
        // Undo can change any track, so stop this session's running jobs first.
        let jobs = self.lock().jobs.active_for(id);
        for j in jobs {
            let _ = self.cancel_job(j);
        }
        let mut label = String::new();
        let rev = self.edit(id, expected, "undo", None, |s| {
            let r = if undo { s.history.undo() } else { s.history.redo() };
            label = r.ok_or_else(|| AppError::new("nothing_to_undo", if undo { "Nothing to undo." } else { "Nothing to redo." }))?;
            s.review = None;
            Ok(())
        })?;
        Ok((label, rev))
    }

    // ---- files -----------------------------------------------------------------------------

    pub fn save_project(&self, id: Uuid, dest: &Path, overwrite: bool) -> Result<u64> {
        if !dest.is_absolute() {
            return fail("invalid_path", "Use an absolute path.");
        }
        let (mut project, revision) = self.with_session(id, |s| s.snapshot())??;
        project.save(dest, overwrite).map_err(|e| match e {
            tracker_core::ProjectError::Write(tracker_core::io::WriteError::Exists(_)) => AppError::new("destination_exists", "Destination exists; pass overwrite=true to replace it."),
            other => AppError::new("file_write_failed", other.to_string()),
        })?;
        self.with_session_mut(id, |s| {
            s.saved_revision = revision;
            s.project_path = Some(dest.to_path_buf());
            Ok(())
        })?;
        Ok(revision)
    }

    pub fn export_csv(&self, id: Uuid, track: TrackId, dest: &Path, overwrite: bool) -> Result<usize> {
        if !dest.is_absolute() {
            return fail("invalid_path", "Use an absolute path.");
        }
        let (rows, unit, source) = self.with_session(id, |s| -> Result<_> {
            let t = s.require_track(track)?;
            let a = s.history.state();
            Ok((tracker_core::kinematics(t, &a.coords, 3.0), a.coords.unit().to_string(), s.path.clone()))
        })??;
        tracker_core::io::check_not_source(dest, &source).map_err(|e| AppError::new("invalid_destination", e.to_string()))?;
        let mut w = csv::Writer::from_writer(vec![]);
        let u = &unit;
        w.write_record(["t_s", "frame_value", "frame_timescale", "pixel_x", "pixel_y", &format!("x_{u}"), &format!("y_{u}"), &format!("vx_{u}_per_s"), &format!("vy_{u}_per_s"), &format!("speed_{u}_per_s"), &format!("ax_{u}_per_s2"), &format!("ay_{u}_per_s2"), "origin", "match_score"]).map_err(|e| AppError::new("file_write_failed", e.to_string()))?;
        let f = |v: Option<f64>| v.map(|v| v.to_string()).unwrap_or_default();
        for r in &rows {
            w.write_record([r.t.to_string(), r.frame.value.to_string(), r.frame.timescale.to_string(), r.pixel_x.to_string(), r.pixel_y.to_string(), r.x.to_string(), r.y.to_string(), f(r.vx), f(r.vy), f(r.speed), f(r.ax), f(r.ay), r.origin.name().into(), f(r.score)]).map_err(|e| AppError::new("file_write_failed", e.to_string()))?;
        }
        let data = w.into_inner().map_err(|e| AppError::new("file_write_failed", e.to_string()))?;
        tracker_core::io::atomic_write(dest, &data, overwrite).map_err(|e| match e {
            tracker_core::io::WriteError::Exists(_) => AppError::new("destination_exists", "Destination exists; pass overwrite=true to replace it."),
            other => AppError::new("file_write_failed", other.to_string()),
        })?;
        Ok(rows.len())
    }

    // ---- datasets ----------------------------------------------------------------------------

    pub fn dataset_create(&self, name: &str, description: &str) -> Result<Dataset> {
        Dataset::create(self.datasets_root(), name, description).map_err(|e| AppError::new("dataset_error", e.to_string()))
    }
    pub fn dataset_open(&self, name: &str) -> Result<Dataset> {
        Dataset::open(self.datasets_root(), name).map_err(|e| match e {
            tracker_dataset::DatasetError::NotFound(_) => AppError::new("dataset_not_found", e.to_string()),
            _ => AppError::new("dataset_error", e.to_string()),
        })
    }
    pub fn dataset_list(&self) -> Result<Vec<tracker_dataset::Manifest>> {
        Dataset::list(self.datasets_root()).map_err(|e| AppError::new("dataset_error", e.to_string()))
    }

    /// Stores a track as one run in a dataset. Returns the run ID.
    pub fn dataset_add_run(&self, dataset: &str, id: Uuid, track: TrackId, added_by: &str, conditions: std::collections::BTreeMap<String, String>, notes: &str) -> Result<Uuid> {
        let mut ds = self.dataset_open(dataset)?;
        let (record, observations) = self.with_session(id, |s| -> Result<_> {
            let t = s.require_track(track)?;
            if t.points.is_empty() {
                return fail("empty_track", "This track has no points to store.");
            }
            let a = s.history.state();
            let rows = tracker_core::kinematics(t, &a.coords, 3.0);
            let run_id = Uuid::new_v4();
            let unit = a.coords.unit().to_string();
            let count = |o: tracker_core::Origin| t.points.iter().filter(|p| p.origin == o).count();
            let record = RunRecord {
                run_id,
                added_at: String::new(),
                added_by: added_by.to_string(),
                video_path: s.path.display().to_string(),
                video_name: s.name.clone(),
                video_fingerprint: s.fingerprint.clone(),
                track_id: t.id,
                track_name: t.name.clone(),
                unit: unit.clone(),
                pixels_per_unit: a.coords.calibration.map(|c| c.pixels_per_unit()),
                origin_x: a.coords.origin.x,
                origin_y: a.coords.origin.y,
                axes_angle_deg: a.coords.angle.to_degrees(),
                algorithm: s.settings.algorithm.id().to_string(),
                settings_json: serde_json::to_string(&s.settings).unwrap_or_default(),
                points: t.points.len(),
                manual_points: count(tracker_core::Origin::Manual),
                automatic_points: count(tracker_core::Origin::Automatic),
                reviewed_points: count(tracker_core::Origin::AcceptedCandidate),
                t_start: rows.first().map(|r| r.t),
                t_end: rows.last().map(|r| r.t),
                conditions,
                notes: notes.to_string(),
                fit: tracker_dataset::fit_motion(&rows),
            };
            let observations = rows.iter().map(|r| Observation::from_row(run_id, &unit, r)).collect::<Vec<_>>();
            Ok((record, observations))
        })??;
        let run = ds.add_run(record, observations).map_err(|e| AppError::new("dataset_error", e.to_string()))?;
        Ok(run)
    }
}
