//! Import and tracking jobs, the shared scheduler and uncertain-match review.

use crate::session::PendingReview;
use crate::{App, AppError, AppEvent, Result, fail};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracker_core::{FrameStamp, Measurement, Origin, Point, TrackId, TrackingSettings};
use tracker_media::VideoInfo;
use tracker_vision::{AutoTracker, GrayFrame, StepOutcome};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Tracking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    AwaitingReview,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobState::Completed | JobState::Failed | JobState::Cancelled)
    }
    pub fn label(&self) -> &'static str {
        match self {
            JobState::Queued => "Queued",
            JobState::Running => "Running",
            JobState::AwaitingReview => "Awaiting review",
            JobState::Completed => "Completed",
            JobState::Failed => "Failed",
            JobState::Cancelled => "Stopped",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobRecord {
    pub job_id: Uuid,
    pub kind: JobKind,
    pub session_id: Uuid,
    pub track_id: Option<TrackId>,
    pub state: JobState,
    /// Frames examined so far.
    pub processed: usize,
    pub accepted: usize,
    pub skipped: usize,
    /// Time of the last frame examined.
    pub last_time: Option<f64>,
    pub error: Option<AppError>,
    pub result: Value,
    /// When a reviewed job continues, the job that carries on.
    pub continued_by: Option<Uuid>,
    pub elapsed_s: f64,
    #[serde(skip)]
    pub(crate) started: Option<Instant>,
    #[serde(skip)]
    pub(crate) cancel: Arc<AtomicBool>,
    #[serde(skip)]
    pub(crate) seq: u64,
}

impl JobRecord {
    pub(crate) fn new(kind: JobKind, session: Uuid, track: Option<TrackId>) -> Self {
        Self { job_id: Uuid::new_v4(), kind, session_id: session, track_id: track, state: JobState::Queued, processed: 0, accepted: 0, skipped: 0, last_time: None, error: None, result: Value::Null, continued_by: None, elapsed_s: 0.0, started: None, cancel: Arc::new(AtomicBool::new(false)), seq: 0 }
    }
}

type Start = Box<dyn FnOnce() + Send>;

/// Bounded job records plus the tracking scheduler (N slots shared by all sessions).
#[derive(Default)]
pub(crate) struct JobTable {
    records: HashMap<Uuid, JobRecord>,
    order: VecDeque<Uuid>,
    running: usize,
    queue: VecDeque<(Uuid, Start)>,
    seq: u64,
}

const JOB_LIMIT: usize = 500;

impl JobTable {
    pub fn insert(&mut self, mut r: JobRecord) -> Uuid {
        self.seq += 1;
        r.seq = self.seq;
        let id = r.job_id;
        self.records.insert(id, r);
        self.order.push_back(id);
        // Drop the oldest finished records beyond the limit.
        while self.order.len() > JOB_LIMIT {
            let Some(pos) = self.order.iter().position(|j| self.records.get(j).is_none_or(|r| r.state.is_terminal())) else { break };
            let old = self.order.remove(pos).unwrap();
            self.records.remove(&old);
        }
        id
    }
    pub fn get(&self, id: Uuid) -> Option<&JobRecord> {
        self.records.get(&id)
    }
    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut JobRecord> {
        self.records.get_mut(&id)
    }
    pub fn list(&self, session: Option<Uuid>) -> Vec<JobRecord> {
        self.order.iter().rev().filter_map(|j| self.records.get(j)).filter(|r| session.is_none_or(|s| r.session_id == s)).take(50).cloned().collect()
    }
    pub fn latest_import(&self, session: Uuid) -> Option<Uuid> {
        self.order.iter().rev().find(|j| self.records.get(j).is_some_and(|r| r.session_id == session && r.kind == JobKind::Import)).copied()
    }
    pub fn active_for(&self, session: Uuid) -> Vec<Uuid> {
        self.records.values().filter(|r| r.session_id == session && r.kind == JobKind::Tracking && !r.state.is_terminal()).map(|r| r.job_id).collect()
    }
    pub fn active_for_track(&self, session: Uuid, track: TrackId) -> Vec<Uuid> {
        self.records.values().filter(|r| r.session_id == session && r.track_id == Some(track) && matches!(r.state, JobState::Queued | JobState::Running)).map(|r| r.job_id).collect()
    }
    pub fn active_track(&self, session: Uuid) -> Option<&JobRecord> {
        self.order.iter().rev().filter_map(|j| self.records.get(j)).find(|r| r.session_id == session && r.kind == JobKind::Tracking && !r.state.is_terminal())
    }
    pub fn latest_track(&self, session: Uuid) -> Option<&JobRecord> {
        self.order.iter().rev().filter_map(|j| self.records.get(j)).find(|r| r.session_id == session && r.kind == JobKind::Tracking)
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
    pub fn running(&self) -> usize {
        self.running
    }
    /// Marks a job cancelled (if not finished) and signals its worker.
    pub fn cancel(&mut self, id: Uuid) {
        if let Some(r) = self.records.get_mut(&id) {
            r.cancel.store(true, Ordering::SeqCst);
            if !r.state.is_terminal() {
                r.state = JobState::Cancelled;
            }
        }
        self.queue.retain(|(j, _)| *j != id);
    }
}

/// Everything a tracking worker needs, captured when the job is submitted.
struct TrackingPlan {
    info: Arc<VideoInfo>,
    session: Uuid,
    track: TrackId,
    job: Uuid,
    /// Tracking continues with frames after this one.
    after: FrameStamp,
    settings: TrackingSettings,
    tracker: Option<AutoTracker>,
    seed: Option<(FrameStamp, Point)>,
    end: Option<FrameStamp>,
    step: u32,
    cancel: Arc<AtomicBool>,
}

impl App {
    pub(crate) fn update_job(&self, id: Uuid, f: impl FnOnce(&mut JobRecord)) {
        {
            let mut s = self.lock();
            if let Some(r) = s.jobs.get_mut(id) {
                f(r);
                if let Some(t) = r.started {
                    r.elapsed_s = t.elapsed().as_secs_f64();
                }
            }
        }
        self.shared.job_changed.notify_all();
        self.emit(AppEvent::Job(id));
    }

    pub fn job(&self, id: Uuid) -> Option<JobRecord> {
        self.lock().jobs.get(id).cloned()
    }
    pub fn jobs(&self, session: Option<Uuid>) -> Vec<JobRecord> {
        self.lock().jobs.list(session)
    }
    /// The running or paused tracking job of a session, if any.
    pub fn active_tracking(&self, session: Uuid) -> Option<JobRecord> {
        self.lock().jobs.active_track(session).cloned()
    }
    pub fn latest_tracking(&self, session: Uuid) -> Option<JobRecord> {
        self.lock().jobs.latest_track(session).cloned()
    }
    /// (running, queued) tracking jobs across the workspace.
    pub fn scheduler_load(&self) -> (usize, usize) {
        let s = self.lock();
        (s.jobs.running(), s.jobs.queued())
    }

    /// Waits up to `timeout` until the job finishes or pauses for review.
    pub fn wait_job(&self, id: Uuid, timeout: Duration) -> Option<JobRecord> {
        let deadline = Instant::now() + timeout;
        let mut s = self.lock();
        loop {
            let now = s.jobs.get(id).cloned()?;
            if now.state.is_terminal() || now.state == JobState::AwaitingReview || Instant::now() >= deadline {
                return Some(now);
            }
            self.shared.job_changed.wait_until(&mut s, deadline);
        }
    }

    /// Starts autotracking forward from the track's point on the frame at `from_seconds`.
    pub fn start_tracking(&self, id: Uuid, expected: Option<u64>, track: TrackId, from_seconds: f64) -> Result<Uuid> {
        let (plan, job) = {
            let mut s = self.lock();
            let state = &mut *s;
            let session = state.sessions.iter_mut().find(|x| x.id == id).ok_or_else(|| AppError::new("session_not_found", "No open session with this ID."))?;
            let info = session.ready()?.clone();
            if let Some(e) = expected {
                if e != session.revision() {
                    return fail("stale_revision", format!("Session revision is {} (you sent {e}).", session.revision()));
                }
            }
            let frame = info.index.stamp_at(from_seconds).ok_or_else(|| AppError::new("invalid_time", "Time is outside this video."))?;
            let t = session.require_track(track)?;
            let seed = t.point_at(frame).ok_or_else(|| AppError::new("invalid_seed", "Mark this track on that frame first (tracker_mark_point), then start tracking from it."))?.pixel;
            if state.jobs.active_track(id).is_some() {
                return fail("job_active", "This session already has a running or paused tracking job. Resolve or cancel it first.");
            }
            session.settings.validate().map_err(|m| AppError::new("invalid_settings", m))?;
            session.review = None;
            session.selected_track = Some(track);
            let clip = session.history.state().clip;
            let record = JobRecord::new(JobKind::Tracking, id, Some(track));
            let cancel = record.cancel.clone();
            let job = state.jobs.insert(record);
            (TrackingPlan { info, session: id, track, job, after: frame, settings: session.settings, tracker: None, seed: Some((frame, seed)), end: clip.end, step: clip.step.max(1), cancel }, job)
        };
        self.submit(plan);
        Ok(job)
    }

    fn submit(&self, plan: TrackingPlan) {
        let job = plan.job;
        let app = self.clone();
        let start: Box<dyn FnOnce() + Send> = Box::new(move || {
            std::thread::Builder::new().name("tracker-job".into()).spawn(move || app.run_tracking(plan)).expect("spawn tracking thread");
        });
        let start_now = {
            let mut s = self.lock();
            if s.jobs.running < self.shared.slots {
                s.jobs.running += 1;
                Some(start)
            } else {
                s.jobs.queue.push_back((job, start));
                None
            }
        };
        self.shared.job_changed.notify_all();
        self.emit(AppEvent::Job(job));
        if let Some(start) = start_now {
            start();
        }
    }

    fn release_slot(&self) {
        let next = {
            let mut s = self.lock();
            match s.jobs.queue.pop_front() {
                Some((_, start)) => Some(start),
                None => {
                    s.jobs.running = s.jobs.running.saturating_sub(1);
                    None
                }
            }
        };
        if let Some(start) = next {
            start();
        }
    }

    fn run_tracking(&self, plan: TrackingPlan) {
        let job = plan.job;
        if plan.cancel.load(Ordering::SeqCst) {
            self.release_slot();
            return;
        }
        self.update_job(job, |j| {
            j.state = JobState::Running;
            j.started = Some(Instant::now());
        });
        let outcome = self.track_frames(plan);
        if let Err(e) = outcome {
            self.update_job(job, |j| {
                if !j.state.is_terminal() {
                    j.state = JobState::Failed;
                    j.error = Some(e.clone());
                }
            });
            self.log("Tracker", format!("Tracking failed: {}", e.message));
        }
        self.release_slot();
    }

    fn track_frames(&self, mut plan: TrackingPlan) -> Result<()> {
        let media = |e: tracker_media::MediaError| AppError::new("media_error", e.to_string());
        let mut tracker = match plan.tracker.take() {
            Some(t) => t,
            None => {
                let (frame, seed) = plan.seed.expect("seed or tracker");
                let f = tracker_media::frame_at(&plan.info, frame, false).map_err(media)?;
                AutoTracker::new(&f.luma, seed, frame.seconds(), plan.settings).map_err(|e| AppError::new("invalid_template", e.to_string()))?
            }
        };
        let mut reader = tracker_media::FrameReader::new(&plan.info, plan.after, false).map_err(media)?;
        let base = plan.info.index.position(plan.after).unwrap_or(0);
        let mut batch: Vec<Measurement> = Vec::new();
        let mut last_delivery = Instant::now();
        let mut last_revision: Option<u64> = None;
        let mut counts = (0usize, 0usize, 0usize);
        let mut last_time = None;
        loop {
            if plan.cancel.load(Ordering::SeqCst) {
                self.deliver(&plan, &mut batch, &mut last_revision, counts, last_time)?;
                return Ok(());
            }
            let Some(frame) = reader.next_frame().map_err(media)? else { break };
            if frame.stamp <= plan.after {
                continue;
            }
            if plan.end.is_some_and(|e| frame.stamp > e) {
                break;
            }
            if plan.step > 1 {
                let pos = plan.info.index.position(frame.stamp).unwrap_or(0);
                if (pos - base) % plan.step as usize != 0 {
                    continue;
                }
            }
            counts.0 += 1;
            last_time = Some(frame.stamp.seconds());
            match tracker.step(&frame.luma, frame.stamp.seconds()) {
                StepOutcome::Accepted(m) => {
                    counts.1 += 1;
                    batch.push(Measurement { id: Uuid::new_v4(), frame: frame.stamp, pixel: m.position, origin: Origin::Automatic, score: Some(m.score), run: Some(plan.job) });
                }
                StepOutcome::Skipped(_) => counts.2 += 1,
                StepOutcome::Review(evidence) => {
                    self.deliver(&plan, &mut batch, &mut last_revision, counts, last_time)?;
                    let review = PendingReview { job_id: plan.job, track_id: plan.track, frame: frame.stamp, evidence, tracker: Some(tracker) };
                    let _ = self.with_session_mut(plan.session, |s| {
                        s.review = Some(review);
                        Ok(())
                    });
                    self.update_job(plan.job, |j| {
                        if !j.state.is_terminal() {
                            j.state = JobState::AwaitingReview;
                            j.result = json!({"review_time": frame.stamp.seconds(), "reason": evidence.reason, "score": evidence.score, "separation": evidence.separation});
                        }
                    });
                    self.log("Tracker", format!("Paused at {:.3} s: {}", frame.stamp.seconds(), evidence.reason.message()));
                    return Ok(());
                }
            }
            if last_delivery.elapsed() >= Duration::from_millis(100) {
                self.deliver(&plan, &mut batch, &mut last_revision, counts, last_time)?;
                last_delivery = Instant::now();
            }
        }
        self.deliver(&plan, &mut batch, &mut last_revision, counts, last_time)?;
        self.update_job(plan.job, |j| {
            if !j.state.is_terminal() {
                j.state = JobState::Completed;
                j.result = json!({"frames": counts.0, "accepted": counts.1, "skipped": counts.2, "frames_per_second": counts.0 as f64 / j.started.map_or(1.0, |t| t.elapsed().as_secs_f64().max(1e-6))});
            }
        });
        Ok(())
    }

    /// Writes a batch into the track as one undo step per run (amending while nothing else changed).
    fn deliver(&self, plan: &TrackingPlan, batch: &mut Vec<Measurement>, last_revision: &mut Option<u64>, counts: (usize, usize, usize), last_time: Option<f64>) -> Result<()> {
        if !batch.is_empty() && !plan.cancel.load(Ordering::SeqCst) {
            let points = std::mem::take(batch);
            let track = plan.track;
            let _ = self.with_session_mut(plan.session, |s| {
                if s.history.state().track(track).is_none() {
                    return Ok(());
                }
                let apply = |a: &mut tracker_core::AnalysisState| {
                    let t = a.track_mut(track).unwrap();
                    for p in points {
                        // Manual and reviewed points stay authoritative.
                        t.put(p, true);
                    }
                };
                if *last_revision == Some(s.history.revision()) {
                    s.history.amend(apply);
                } else {
                    s.history.edit("Autotrack", apply);
                }
                *last_revision = Some(s.history.revision());
                Ok(())
            });
        }
        batch.clear();
        self.update_job(plan.job, |j| {
            j.processed = counts.0;
            j.accepted = counts.1;
            j.skipped = counts.2;
            j.last_time = last_time;
        });
        Ok(())
    }

    pub fn cancel_job(&self, id: Uuid) -> Result<JobRecord> {
        let session = {
            let mut s = self.lock();
            let r = s.jobs.get(id).ok_or_else(|| AppError::new("job_not_found", "No job with this ID in this app session."))?;
            let session = r.session_id;
            let was_queued = r.state == JobState::Queued;
            s.jobs.cancel(id);
            if was_queued {
                // A queued job never took a slot; nothing else to release.
            }
            if let Some(sess) = s.sessions.iter_mut().find(|x| x.id == session) {
                if sess.review.as_ref().is_some_and(|r| r.job_id == id) {
                    sess.review = None;
                }
            }
            session
        };
        self.shared.job_changed.notify_all();
        self.emit(AppEvent::Job(id));
        self.emit(AppEvent::Session(session));
        Ok(self.job(id).unwrap())
    }

    /// Resolves a paused job. Returns the continuation job for accept, correct and skip.
    pub fn review_match(&self, id: Uuid, job: Uuid, action: &str, correction: Option<Point>, actor: &str) -> Result<Option<Uuid>> {
        let (mut review, info, settings, clip) = {
            let mut s = self.lock();
            let session = s.sessions.iter_mut().find(|x| x.id == id).ok_or_else(|| AppError::new("session_not_found", "No open session with this ID."))?;
            let info = session.ready()?.clone();
            match &session.review {
                Some(r) if r.job_id == job => {}
                _ => return fail("review_not_found", "This job has no pending uncertain frame."),
            }
            if action == "accept" && session.review.as_ref().unwrap().evidence.candidate.is_none() {
                return fail("no_candidate", "There is no candidate to accept; use correct with x and y, or skip.");
            }
            if action == "correct" {
                let p = correction.ok_or_else(|| AppError::new("invalid_argument", "correct needs x and y."))?;
                if !p.inside(info.width as f64, info.height as f64) {
                    return fail("invalid_coordinate", "The correction must lie inside the image.");
                }
            }
            if !matches!(action, "accept" | "correct" | "skip" | "stop") {
                return fail("invalid_argument", "action must be accept, correct, skip or stop.");
            }
            let review = session.review.take().unwrap();
            (review, info, session.settings, session.history.state().clip)
        };
        let record = |pixel: Point, origin: Origin| -> Result<()> {
            self.with_session_mut(id, |s| {
                s.history.edit(if origin == Origin::Manual { "Correct point" } else { "Accept candidate" }, |a| {
                    if let Some(t) = a.track_mut(review.track_id) {
                        t.put(Measurement { id: Uuid::new_v4(), frame: review.frame, pixel, origin, score: (origin == Origin::AcceptedCandidate).then_some(review.evidence.score), run: Some(job) }, false);
                    }
                });
                Ok(())
            })
        };
        let mut tracker = review.tracker.take();
        match action {
            "stop" => {
                self.update_job(job, |j| j.state = JobState::Cancelled);
                self.log(actor, format!("Stopped tracking at {:.3} s", review.frame.seconds()));
                return Ok(None);
            }
            "accept" | "correct" => {
                let pixel = if action == "accept" { review.evidence.candidate.unwrap() } else { correction.unwrap() };
                record(pixel, if action == "accept" { Origin::AcceptedCandidate } else { Origin::Manual })?;
                let f = tracker_media::frame_at(&info, review.frame, false).map_err(|e| AppError::new("media_error", e.to_string()))?;
                if let Some(t) = tracker.as_mut() {
                    if let Err(e) = t.accept_correction(&f.luma, review.frame.seconds(), pixel) {
                        self.update_job(job, |j| j.state = JobState::Completed);
                        return fail("invalid_template", e.to_string());
                    }
                } else {
                    tracker = Some(AutoTracker::new(&f.luma, pixel, review.frame.seconds(), settings).map_err(|e| AppError::new("invalid_template", e.to_string()))?);
                }
            }
            _ => {
                if let Some(t) = tracker.as_mut() {
                    t.skip();
                }
            }
        }
        let record = JobRecord::new(JobKind::Tracking, id, Some(review.track_id));
        let cancel = record.cancel.clone();
        let next = self.lock().jobs.insert(record);
        self.update_job(job, |j| {
            j.state = JobState::Completed;
            j.continued_by = Some(next);
        });
        self.log(actor, format!("{} uncertain frame at {:.3} s", match action { "accept" => "Accepted", "correct" => "Corrected", _ => "Skipped" }, review.frame.seconds()));
        let tracker = match tracker {
            Some(t) => t,
            None => return fail("continuation_failed", "Tracking state was lost; mark a point and start again."),
        };
        self.submit(TrackingPlan { info, session: id, track: review.track_id, job: next, after: review.frame, settings, tracker: Some(tracker), seed: None, end: clip.end, step: clip.step.max(1), cancel });
        Ok(Some(next))
    }

    /// The luma of a frame, for tests and diagnostics.
    pub fn luma(&self, id: Uuid, stamp: FrameStamp) -> Result<GrayFrame> {
        let info = self.with_session(id, |s| s.ready().cloned())??;
        tracker_media::frame_at(&info, stamp, false).map(|f| f.luma).map_err(|e| AppError::new("media_error", e.to_string()))
    }
}
