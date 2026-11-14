//! The workspace window. Layout follows Java Tracker (toolbar, track bar, main video view with
//! player bar, plot and table panes on the right) and keeps the Swift app's additions: a status
//! line and an agent bar that shows what connected AI agents are doing.

use crate::plot::{self, Var};
use crate::style;
use crate::widgets::{Field, button, label, separator};
use crate::*;
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tracker_app::{App as Core, DisplayFrame, JobState, SessionStatus};
use tracker_core::{CoordinateSystem, FrameStamp, KinematicRow, Point as Px, Track, TrackingSettings};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Navigate,
    Mark,
    Calibrate,
    Axes,
    Correct,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaneView {
    Plot,
    Table,
}

enum Dialog {
    Calibration { first: Px, second: Px, length: Field, unit: &'static str },
    Clip { start: Field, end: Field, step: Field },
    Dataset { name: Field, notes: Field },
    Rename { track: Uuid, name: Field },
}

impl Dialog {
    fn fields(&mut self) -> Vec<&mut Field> {
        match self {
            Dialog::Calibration { length, .. } => vec![length],
            Dialog::Clip { start, end, step } => vec![start, end, step],
            Dialog::Dataset { name, notes } => vec![name, notes],
            Dialog::Rename { name, .. } => vec![name],
        }
    }
}

/// Everything the window draws for the selected session, copied out under the lock.
struct Snapshot {
    id: Uuid,
    name: String,
    status: SessionStatus,
    width: f64,
    height: f64,
    duration: f64,
    frames: usize,
    first: Option<FrameStamp>,
    last: Option<FrameStamp>,
    viewer: FrameStamp,
    viewer_index: usize,
    playing: bool,
    display: Option<Arc<DisplayFrame>>,
    display_error: Option<String>,
    tracks: Vec<Arc<Track>>,
    coords: CoordinateSystem,
    selected: Option<Uuid>,
    settings: TrackingSettings,
    review: Option<(Uuid, FrameStamp, tracker_vision::Evidence)>,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    undo_label: Option<String>,
    revision: u64,
    project: Option<PathBuf>,
    clip_end: Option<FrameStamp>,
}

pub struct TrackerWindow {
    core: Core,
    focus: FocusHandle,
    tool: Tool,
    calib_points: Vec<Px>,
    dialog: Option<Dialog>,
    dialog_focus: usize,
    image: Option<(Uuid, FrameStamp, Arc<RenderImage>)>,
    retired: Rc<std::cell::RefCell<Vec<Arc<RenderImage>>>>,
    top: PaneView,
    bottom: PaneView,
    plot_var: Var,
    show_activity: bool,
    message: Option<(String, bool)>,
    video_rect: Rc<Cell<Option<Bounds<Pixels>>>>,
    server_error: Option<String>,
    pending_discard: Option<Uuid>,
    /// The last uncertain frame the viewer jumped to, so each new pause is shown once.
    seen_review: Option<(Uuid, FrameStamp)>,
    scrub_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _poll: Task<()>,
}

impl TrackerWindow {
    pub fn new(core: Core, server_error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rx = core.subscribe();
        // Coalesce core events (from agents, jobs and decoding) into at most ~30 redraws a second.
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(33)).await;
                let mut changed = false;
                while rx.try_recv().is_ok() {
                    changed = true;
                }
                if changed && this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            core,
            focus,
            tool: Tool::Navigate,
            calib_points: vec![],
            dialog: None,
            dialog_focus: 0,
            image: None,
            retired: Rc::new(std::cell::RefCell::new(vec![])),
            top: PaneView::Plot,
            bottom: PaneView::Table,
            plot_var: Var::Y,
            show_activity: false,
            message: None,
            video_rect: Rc::new(Cell::new(None)),
            server_error,
            pending_discard: None,
            seen_review: None,
            scrub_bounds: Rc::new(Cell::new(None)),
            _poll: poll,
        }
    }

    fn snapshot(&self) -> Option<Snapshot> {
        let id = self.core.selected()?;
        self.core
            .with_session(id, |s| {
                let info = s.info.as_ref();
                let a = s.history.state();
                Snapshot {
                    id,
                    name: s.name.clone(),
                    status: s.status.clone(),
                    width: info.map_or(0.0, |i| i.width as f64),
                    height: info.map_or(0.0, |i| i.height as f64),
                    duration: info.map_or(0.0, |i| i.duration),
                    frames: info.map_or(0, |i| i.index.len()),
                    first: info.and_then(|i| i.index.first()),
                    last: info.and_then(|i| i.index.last()),
                    viewer: s.viewer,
                    viewer_index: info.and_then(|i| i.index.position(s.viewer)).unwrap_or(0),
                    playing: s.is_playing(),
                    display: s.display.clone(),
                    display_error: s.display_error.clone(),
                    tracks: a.tracks.clone(),
                    coords: a.coords,
                    selected: s.selected_track.filter(|t| a.track(*t).is_some()).or_else(|| a.tracks.first().map(|t| t.id)),
                    settings: s.settings,
                    review: s.review.as_ref().map(|r| (r.job_id, r.frame, r.evidence)),
                    dirty: s.is_dirty(),
                    can_undo: s.history.can_undo(),
                    can_redo: s.history.can_redo(),
                    undo_label: s.history.undo_label().map(str::to_string),
                    revision: s.revision(),
                    project: s.project_path.clone(),
                    clip_end: a.clip.end,
                }
            })
            .ok()
    }

    fn report<T>(&mut self, r: Result<T, tracker_app::AppError>, cx: &mut Context<Self>) -> Option<T> {
        match r {
            Ok(v) => {
                self.message = None;
                cx.notify();
                Some(v)
            }
            Err(e) => {
                self.message = Some((e.message, true));
                cx.notify();
                None
            }
        }
    }

    /// Like `report`, and on success records the action in the shared activity log as "You",
    /// next to agent actions.
    fn act<T>(&mut self, label: &str, r: Result<T, tracker_app::AppError>, cx: &mut Context<Self>) -> Option<T> {
        let v = self.report(r, cx);
        if v.is_some() {
            self.core.log("You", label);
        }
        v
    }

    fn selected_track(&self, s: &Snapshot) -> Option<Uuid> {
        s.selected
    }

    // ---- actions --------------------------------------------------------------------------

    fn open_video(&mut self, _: &OpenVideo, _: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Open".into()) });
        let core = self.core.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                for p in paths {
                    let r = if p.extension().is_some_and(|e| e == "json") { core.open_project(&p, None) } else { core.import_video(&p) };
                    let _ = this.update(cx, |w, cx| {
                        w.report(r, cx);
                    });
                }
            }
        })
        .detach();
    }

    fn open_project(&mut self, _: &OpenProject, w: &mut Window, cx: &mut Context<Self>) {
        self.open_video(&OpenVideo, w, cx);
    }

    fn save_as(&mut self, cx: &mut Context<Self>, id: Uuid, name: String) {
        let dir = dirs_home();
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{}.tracker.json", name.rsplit_once('.').map_or(name.as_str(), |(n, _)| n))));
        let core = self.core.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let r = core.save_project(id, &path, true);
                let _ = this.update(cx, |w, cx| {
                    if w.report(r, cx).is_some() {
                        w.message = Some((format!("Saved {}", path.display()), false));
                    }
                });
            }
        })
        .detach();
    }

    fn save(&mut self, _: &SaveProject, _: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.snapshot() else { return };
        match s.project.clone().filter(|p| p.extension().is_some_and(|e| e == "json")) {
            Some(p) => {
                if self.act("Saved project", self.core.save_project(s.id, &p, true), cx).is_some() {
                    self.message = Some((format!("Saved {}", p.display()), false));
                }
            }
            None => self.save_as(cx, s.id, s.name),
        }
    }

    fn save_project_as(&mut self, _: &SaveProjectAs, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.save_as(cx, s.id, s.name);
        }
    }

    fn export_csv(&mut self, _: &ExportCsv, _: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.snapshot() else { return };
        let Some(track) = self.selected_track(&s) else { return };
        let name = s.tracks.iter().find(|t| t.id == track).map(|t| t.name.clone()).unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dirs_home(), Some(&format!("{name}.csv")));
        let core = self.core.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let r = core.export_csv(s.id, track, &path, true);
                let _ = this.update(cx, |w, cx| {
                    if let Some(n) = w.report(r, cx) {
                        w.message = Some((format!("Exported {n} rows to {}", path.display()), false));
                    }
                });
            }
        })
        .detach();
    }

    fn close_video(&mut self, _: &CloseVideo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.snapshot() else { return };
        if s.dirty && self.pending_discard != Some(s.id) {
            self.pending_discard = Some(s.id);
            self.message = Some(("This video has unsaved changes. Save Project to keep them, or choose Close Video again to discard them.".into(), true));
            cx.notify();
            return;
        }
        let discard = self.pending_discard == Some(s.id);
        self.pending_discard = None;
        self.act("Closed video", self.core.close(s.id, discard), cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.act("Undid an edit", self.core.undo(s.id, None), cx);
        }
    }
    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.act("Redid an edit", self.core.redo(s.id, None), cx);
        }
    }
    fn play_pause(&mut self, _: &PlayPause, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            if s.playing {
                self.core.pause(s.id);
            } else {
                self.report(self.core.play(s.id), cx);
            }
        }
    }
    fn step(&mut self, n: isize, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.report(self.core.step(s.id, n), cx);
        }
    }
    fn next_frame(&mut self, _: &NextFrame, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }
    fn prev_frame(&mut self, _: &PrevFrame, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }
    fn first_frame(&mut self, _: &FirstFrame, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.report(self.core.seek(s.id, 0.0), cx);
        }
    }
    fn new_point_mass(&mut self, _: &NewPointMass, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            if self.act("Added a point mass", self.core.add_track(s.id, None, None), cx).is_some() {
                self.tool = Tool::Mark;
            }
        }
    }
    fn delete_track(&mut self, _: &DeleteTrack, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            if let Some(t) = s.selected {
                self.act("Deleted a track", self.core.delete_track(s.id, None, t), cx);
            }
        }
    }
    fn rename_track(&mut self, _: &RenameTrack, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            if let Some(t) = s.tracks.iter().find(|t| Some(t.id) == s.selected) {
                self.dialog = Some(Dialog::Rename { track: t.id, name: Field::new("Name", t.name.clone()) });
                self.dialog_focus = 0;
                cx.notify();
            }
        }
    }
    fn clip_settings(&mut self, _: &ClipSettings, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            let fmt = |f: Option<FrameStamp>| f.map(|f| format!("{:.4}", f.seconds())).unwrap_or_default();
            let clip = self.core.with_session(s.id, |x| x.history.state().clip).unwrap_or_default();
            self.dialog = Some(Dialog::Clip { start: Field::new("Start time (s)", fmt(clip.start)), end: Field::new("End time (s)", fmt(clip.end)), step: Field::new("Step (frames)", clip.step.max(1).to_string()) });
            self.dialog_focus = 0;
            cx.notify();
        }
    }
    fn calibrate(&mut self, _: &Calibrate, _: &mut Window, cx: &mut Context<Self>) {
        self.tool = Tool::Calibrate;
        self.calib_points.clear();
        self.message = Some(("Click both ends of an object of known length (for example a meter stick).".into(), false));
        cx.notify();
    }
    fn move_axes(&mut self, _: &MoveAxes, _: &mut Window, cx: &mut Context<Self>) {
        self.tool = Tool::Axes;
        self.message = Some(("Click where the coordinate origin should be.".into(), false));
        cx.notify();
    }
    fn clear_scale(&mut self, _: &ClearScale, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            self.act("Cleared the scale", self.core.clear_calibration(s.id, None), cx);
        }
    }
    fn start_autotrack(&mut self, _: &StartAutotrack, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            if let Some(t) = s.selected {
                self.act("Started autotracking", self.core.start_tracking(s.id, None, t, s.viewer.seconds()), cx);
            }
        }
    }
    fn add_to_dataset(&mut self, _: &AddToDataset, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.core.dataset_list().ok().and_then(|l| l.last().map(|m| m.name.clone())).unwrap_or_else(|| "My experiment".into());
        self.dialog = Some(Dialog::Dataset { name: Field::new("Dataset", last), notes: Field::new("Notes", "") });
        self.dialog_focus = 0;
        cx.notify();
    }
    fn toggle_agent_control(&mut self, _: &ToggleAgentControl, _: &mut Window, cx: &mut Context<Self>) {
        let on = self.core.control_enabled();
        self.core.set_control_enabled(!on);
        cx.notify();
    }
    fn show_activity(&mut self, _: &ShowActivity, _: &mut Window, cx: &mut Context<Self>) {
        self.show_activity = !self.show_activity;
        cx.notify();
    }

    fn apply_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.snapshot() else {
            self.dialog = None;
            return;
        };
        let Some(d) = self.dialog.take() else { return };
        let result: Result<String, tracker_app::AppError> = match &d {
            Dialog::Calibration { first, second, length, unit } => match length.number() {
                Some(l) => self.core.set_calibration(s.id, None, *first, *second, l, unit).map(|_| format!("Scale set: {l} {unit} between the two endpoints.")),
                None => Err(tracker_app::AppError::new("invalid", "Enter the known length as a number.")),
            },
            Dialog::Clip { start, end, step } => {
                let st = step.number().map(|v| v.max(1.0) as u32);
                self.core.set_clip(s.id, None, start.number(), end.number(), st).map(|_| "Clip settings updated.".into())
            }
            Dialog::Rename { track, name } => self.core.rename_track(s.id, None, *track, &name.text).map(|_| "Track renamed.".into()),
            Dialog::Dataset { name, notes } => {
                let ds = name.text.trim().to_string();
                if self.core.dataset_open(&ds).is_err() {
                    let _ = self.core.dataset_create(&ds, "");
                }
                match s.selected {
                    Some(t) => self.core.dataset_add_run(&ds, s.id, t, "user", Default::default(), &notes.text).map(|_| format!("Stored the track in dataset \"{ds}\" ({}).", self.core.datasets_root().join(&ds).display())),
                    None => Err(tracker_app::AppError::new("no_track", "Select a track first.")),
                }
            }
        };
        match result {
            Ok(m) => {
                self.core.log("You", m.clone());
                self.message = Some((m, false));
            }
            Err(e) => {
                self.message = Some((e.message, true));
                self.dialog = Some(d);
            }
        }
        if matches!(self.tool, Tool::Calibrate) && self.dialog.is_none() {
            self.tool = Tool::Navigate;
            self.calib_points.clear();
        }
        cx.notify();
    }

    /// Tab and Shift-Tab: next or previous dialog field; otherwise normal focus navigation.
    fn move_field(&mut self, delta: isize, cx: &mut Context<Self>) {
        match self.dialog.as_mut() {
            Some(d) => {
                let n = d.fields().len() as isize;
                self.dialog_focus = ((self.dialog_focus as isize + delta).rem_euclid(n.max(1))) as usize;
                cx.notify();
            }
            None => cx.propagate(),
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if self.dialog.is_some() {
            match ks.key.as_str() {
                "escape" => {
                    self.dialog = None;
                    if self.tool == Tool::Calibrate {
                        self.calib_points.clear();
                    }
                }
                "enter" => self.apply_dialog(cx),
                "tab" => {
                    let n = self.dialog.as_mut().map_or(1, |d| d.fields().len());
                    self.dialog_focus = (self.dialog_focus + 1) % n;
                }
                _ => {
                    let i = self.dialog_focus;
                    if let Some(d) = self.dialog.as_mut() {
                        if let Some(f) = d.fields().into_iter().nth(i) {
                            f.key(ks);
                        }
                    }
                }
            }
            cx.notify();
            return;
        }
        if ks.modifiers.platform || ks.modifiers.control {
            return;
        }
        match ks.key.as_str() {
            "space" => self.play_pause(&PlayPause, window, cx),
            "right" => self.step(1, cx),
            "left" => self.step(-1, cx),
            "escape" => {
                self.tool = Tool::Navigate;
                self.calib_points.clear();
                cx.notify();
            }
            _ => {}
        }
    }

    // ---- video interaction --------------------------------------------------------------

    fn to_source(&self, pos: Point<Pixels>, s: &Snapshot) -> Option<Px> {
        let r = self.video_rect.get()?;
        if !r.contains(&pos) || s.width <= 0.0 {
            return None;
        }
        let fx = f32::from(pos.x - r.origin.x) / f32::from(r.size.width);
        let fy = f32::from(pos.y - r.origin.y) / f32::from(r.size.height);
        Some(Px::new(fx as f64 * s.width, fy as f64 * s.height))
    }

    fn video_click(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(s) = self.snapshot() else { return };
        if s.status != SessionStatus::Ready {
            return;
        }
        let Some(p) = self.to_source(ev.position, &s) else { return };
        let tool = if ev.modifiers.shift && self.tool == Tool::Navigate { Tool::Mark } else { self.tool };
        match tool {
            Tool::Navigate => {}
            Tool::Mark => {
                let track = match s.selected {
                    Some(t) => Some(t),
                    None => self.act("Added a point mass", self.core.add_track(s.id, None, None), cx).map(|(t, _)| t),
                };
                if let Some(t) = track {
                    if self.act(&format!("Marked ({:.1}, {:.1}) at {:.3} s", p.x, p.y, s.viewer.seconds()), self.core.mark_point(s.id, None, t, s.viewer.seconds(), p), cx).is_some() && self.tool == Tool::Mark {
                        // Java Tracker advances one step after each mark.
                        let _ = self.core.step(s.id, 1);
                    }
                }
            }
            Tool::Calibrate => {
                self.calib_points.push(p);
                if self.calib_points.len() == 2 {
                    self.dialog = Some(Dialog::Calibration { first: self.calib_points[0], second: self.calib_points[1], length: Field::new("Known length", "1.0"), unit: "m" });
                    self.dialog_focus = 0;
                }
            }
            Tool::Axes => {
                let angle = s.coords.angle.to_degrees();
                if self.act("Moved axes origin", self.core.set_axes(s.id, None, p, angle), cx).is_some() {
                    self.tool = Tool::Navigate;
                    self.message = Some(("Origin moved. Positions recalculated without re-tracking.".into(), false));
                }
            }
            Tool::Correct => {
                if let Some((job, _, _)) = s.review {
                    self.report(self.core.review_match(s.id, job, "correct", Some(p), "You"), cx);
                }
                self.tool = Tool::Navigate;
            }
        }
        cx.notify();
    }

    fn render_image(&mut self, s: &Snapshot) -> Option<Arc<RenderImage>> {
        let d = s.display.as_ref()?;
        if let Some((id, stamp, img)) = &self.image {
            if *id == s.id && *stamp == d.stamp {
                return Some(img.clone());
            }
        }
        // GPUI images are BGRA.
        let mut bgra = d.rgba.clone();
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        let buf = image::RgbaImage::from_raw(d.width as u32, d.height as u32, bgra)?;
        let img = Arc::new(RenderImage::new(vec![image::Frame::new(buf)]));
        if let Some((_, _, old)) = self.image.replace((s.id, d.stamp, img.clone())) {
            self.retired.borrow_mut().push(old);
        }
        Some(img)
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

impl Render for TrackerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut snap = self.snapshot();
        // Show each newly paused frame so its candidate is visible. Only the viewer moves;
        // agents never depend on it.
        if let Some(s) = &snap {
            if let Some((job, frame, _)) = s.review {
                if self.seen_review != Some((job, frame)) {
                    self.seen_review = Some((job, frame));
                    if s.viewer != frame {
                        let _ = self.core.seek(s.id, frame.seconds());
                        snap = self.snapshot();
                    }
                }
            }
        }
        let ids = self.core.session_ids();
        let mut root = div()
            .id("tracker-root")
            .key_context("Tracker")
            .track_focus(&self.focus)
            .on_action(cx.listener(|w, _: &NextField, _, cx| w.move_field(1, cx)))
            .on_action(cx.listener(|w, _: &PrevField, _, cx| w.move_field(-1, cx)))
            .on_key_down(cx.listener(Self::on_key))
            .on_action(cx.listener(Self::open_video))
            .on_action(cx.listener(Self::open_project))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_project_as))
            .on_action(cx.listener(Self::export_csv))
            .on_action(cx.listener(Self::close_video))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::play_pause))
            .on_action(cx.listener(Self::next_frame))
            .on_action(cx.listener(Self::prev_frame))
            .on_action(cx.listener(Self::first_frame))
            .on_action(cx.listener(Self::new_point_mass))
            .on_action(cx.listener(Self::delete_track))
            .on_action(cx.listener(Self::rename_track))
            .on_action(cx.listener(Self::clip_settings))
            .on_action(cx.listener(Self::calibrate))
            .on_action(cx.listener(Self::move_axes))
            .on_action(cx.listener(Self::clear_scale))
            .on_action(cx.listener(Self::start_autotrack))
            .on_action(cx.listener(Self::add_to_dataset))
            .on_action(cx.listener(Self::toggle_agent_control))
            .on_action(cx.listener(Self::show_activity))
            .size_full()
            .flex()
            .flex_col()
            .bg(style::window_bg())
            .text_color(style::text())
            .font_family(".SystemUIFont")
            .relative();

        root = root.child(self.render_tabs(&ids, snap.as_ref().map(|s| s.id), cx));
        root = root.child(self.render_toolbar(snap.as_ref(), cx));
        match &snap {
            None => root = root.child(self.render_empty(cx)),
            Some(s) => {
                root = root.child(self.render_trackbar(s, cx));
                root = root.child(self.render_autotracker(s, cx));
                root = root.child(
                    div().flex_1().min_h(px(0.)).child(
                        h_resizable("main-split")
                            .child(resizable_panel().child(div().size_full().flex().flex_col().child(self.render_video(s, window, cx)).child(self.render_player(s, cx))))
                            .child(resizable_panel().size(px(400.)).size_range(px(280.)..px(760.)).child(
                                v_resizable("right-split").child(resizable_panel().child(self.render_pane(s, true, cx))).child(resizable_panel().child(self.render_pane(s, false, cx))),
                            )),
                    ),
                );
            }
        }
        root = root.child(self.render_status(snap.as_ref()));
        root = root.child(self.render_agent_bar(cx));
        if self.show_activity {
            root = root.child(self.render_activity(cx));
        }
        if self.dialog.is_some() {
            root = root.child(self.render_dialog(cx));
        }
        root
    }
}

impl TrackerWindow {
    fn render_tabs(&self, ids: &[Uuid], selected: Option<Uuid>, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div().flex().items_end().h(px(30.)).px_2().gap_1().bg(style::window_bg()).border_b_1().border_color(style::border());
        for id in ids {
            let (name, dirty, status) = self.core.with_session(*id, |s| (s.name.clone(), s.is_dirty(), s.status.clone())).unwrap_or_default();
            let active = Some(*id) == selected;
            let id = *id;
            let suffix = match status {
                SessionStatus::Loading => " (loading)",
                SessionStatus::Failed(_) => " (error)",
                SessionStatus::Ready if dirty => " •",
                _ => "",
            };
            row = row.child(
                div()
                    .id(SharedString::from(format!("tab-{id}")))
                    .px_3()
                    .py(px(4.))
                    .rounded_t(px(5.))
                    .border_1()
                    .border_b_0()
                    .border_color(if active { style::border() } else { style::hairline() })
                    .bg(if active { style::panel_bg() } else { style::window_bg() })
                    .text_size(px(12.5))
                    .text_color(if active { style::text() } else { style::text_muted() })
                    .cursor_pointer()
                    .child(format!("{name}{suffix}"))
                    .on_click(cx.listener(move |w, _, _, cx| {
                        let _ = w.core.select(id);
                        cx.notify();
                    })),
            );
        }
        row
    }

    fn render_toolbar(&self, s: Option<&Snapshot>, cx: &mut Context<Self>) -> impl IntoElement {
        let ready = s.is_some_and(|s| s.status == SessionStatus::Ready);
        let has_track = s.is_some_and(|s| s.selected.is_some());
        let tool = self.tool;
        let calibrated = s.is_some_and(|s| s.coords.calibration.is_some());
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(5.))
            .bg(style::panel_bg())
            .border_b_1()
            .border_color(style::border())
            .child(button("tb-open", "Open", true, false, cx.listener(|w, _, win, cx| w.open_video(&OpenVideo, win, cx))))
            .child(button("tb-save", "Save", ready, false, cx.listener(|w, _, win, cx| w.save(&SaveProject, win, cx))))
            .child(button("tb-undo", "Undo", s.is_some_and(|s| s.can_undo), false, cx.listener(|w, _, win, cx| w.undo(&Undo, win, cx))))
            .child(button("tb-redo", "Redo", s.is_some_and(|s| s.can_redo), false, cx.listener(|w, _, win, cx| w.redo(&Redo, win, cx))))
            .child(separator())
            .child(button("tb-clip", "Clip Settings", ready, false, cx.listener(|w, _, win, cx| w.clip_settings(&ClipSettings, win, cx))))
            .child(button("tb-cal", if calibrated { "Calibration ✓" } else { "Calibration" }, ready, tool == Tool::Calibrate, cx.listener(|w, _, win, cx| w.calibrate(&Calibrate, win, cx))))
            .child(button("tb-axes", "Axes", ready, tool == Tool::Axes, cx.listener(|w, _, win, cx| w.move_axes(&MoveAxes, win, cx))))
            .child(separator())
            .child(button("tb-new", "New Point Mass", ready, false, cx.listener(|w, _, win, cx| w.new_point_mass(&NewPointMass, win, cx))))
            .child(button("tb-mark", "Mark", ready && has_track, tool == Tool::Mark, cx.listener(|w, _, _, cx| {
                w.tool = if w.tool == Tool::Mark { Tool::Navigate } else { Tool::Mark };
                w.message = Some(("Click the object to mark it; the video steps forward after each mark. Shift-click marks in any mode.".into(), false));
                cx.notify();
            })))
            .child(button("tb-auto", "Autotrack", ready && has_track, false, cx.listener(|w, _, win, cx| w.start_autotrack(&StartAutotrack, win, cx))))
            .child(separator())
            .child(button("tb-csv", "Export CSV", ready && has_track, false, cx.listener(|w, _, win, cx| w.export_csv(&ExportCsv, win, cx))))
            .child(button("tb-ds", "Add to Dataset", ready && has_track, false, cx.listener(|w, _, win, cx| w.add_to_dataset(&AddToDataset, win, cx))))
            .child(div().flex_1())
            .child(label(match tool {
                Tool::Navigate => "Navigate".to_string(),
                Tool::Mark => "Marking".to_string(),
                Tool::Calibrate => format!("Calibrating: endpoint {} of 2", self.calib_points.len() + 1),
                Tool::Axes => "Placing origin".to_string(),
                Tool::Correct => "Click the correct position".to_string(),
            }))
    }

    fn render_trackbar(&self, s: &Snapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let mut bar = div().flex().items_center().gap_2().px_2().py(px(4.)).bg(style::panel_bg()).border_b_1().border_color(style::border()).child(label("Track"));
        if s.tracks.is_empty() {
            bar = bar.child(label("None yet. Choose New Point Mass, then mark the object."));
        }
        for t in &s.tracks {
            let id = t.id;
            let sid = s.id;
            let active = Some(t.id) == s.selected;
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("trk-{id}")))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(2.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if active { style::accent() } else { style::border() })
                    .bg(if active { style::accent_soft() } else { style::surface() })
                    .cursor_pointer()
                    .child(div().size(px(9.)).rounded_full().bg(style::track_color(t.color)))
                    .child(div().text_size(px(12.5)).child(format!("{} ({})", t.name, t.points.len())))
                    .on_click(cx.listener(move |w, _, _, cx| {
                        // View state only: selecting never cancels tracking or changes the revision.
                        let _ = w.core.set_selected_track(sid, Some(id));
                        cx.notify();
                    })),
            );
        }
        if let Some(t) = s.tracks.iter().find(|t| Some(t.id) == s.selected) {
            bar = bar.child(separator());
            match t.point_at(s.viewer) {
                Some(m) => {
                    let w = s.coords.to_world(m.pixel);
                    bar = bar.child(label(format!("t {:.3} s   x {:.4} {u}   y {:.4} {u}   via {}{}", s.viewer.seconds(), w.x, w.y, m.origin.name(), m.score.map(|sc| format!(" (score {sc:.3})")).unwrap_or_default(), u = s.coords.unit())));
                }
                None => bar = bar.child(label(format!("t {:.3} s   no point on this frame", s.viewer.seconds()))),
            }
        }
        bar
    }

    fn render_autotracker(&self, s: &Snapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let job = self.core.active_tracking(s.id).or_else(|| self.core.latest_tracking(s.id));
        let mut strip = div().flex().items_center().gap_2().px_2().py(px(4.)).bg(style::panel_bg()).border_b_1().border_color(style::border()).child(div().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).child("Autotracker"));
        let set = s.settings;
        strip = strip.child(label(format!("{} · template {} px · search {} px · {}", match set.algorithm { tracker_core::Algorithm::NccV2 => "masked NCC + prediction", tracker_core::Algorithm::SsdV1 => "Swift baseline SSD" }, set.template_radius, set.search_radius, match set.review_policy { tracker_core::ReviewPolicy::Pause => "pause when unsure", tracker_core::ReviewPolicy::SkipUncertain => "skip unsure frames" })));
        let sid = s.id;
        let adjust = move |w: &mut TrackerWindow, f: &dyn Fn(&mut TrackingSettings)| {
            let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
            f(&mut st);
            let r = w.core.configure_tracking(sid, None, st);
            r.map(|_| ())
        };
        let _ = adjust;
        strip = strip
            .child(button("at-s-", "Search −", true, false, cx.listener(move |w, _, _, cx| {
                let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
                st.search_radius = (st.search_radius.saturating_sub(8)).max(8);
                w.act("Changed autotracker settings", w.core.configure_tracking(sid, None, st), cx);
            })))
            .child(button("at-s+", "Search +", true, false, cx.listener(move |w, _, _, cx| {
                let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
                st.search_radius = (st.search_radius + 8).min(256);
                w.act("Changed autotracker settings", w.core.configure_tracking(sid, None, st), cx);
            })))
            .child(button("at-t-", "Template −", true, false, cx.listener(move |w, _, _, cx| {
                let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
                st.template_radius = (st.template_radius.saturating_sub(2)).max(3);
                w.act("Changed autotracker settings", w.core.configure_tracking(sid, None, st), cx);
            })))
            .child(button("at-t+", "Template +", true, false, cx.listener(move |w, _, _, cx| {
                let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
                st.template_radius = (st.template_radius + 2).min(48);
                w.act("Changed autotracker settings", w.core.configure_tracking(sid, None, st), cx);
            })))
            .child(button("at-pol", "Policy", true, false, cx.listener(move |w, _, _, cx| {
                let mut st = w.core.with_session(sid, |x| x.settings).unwrap_or_default();
                st.review_policy = match st.review_policy { tracker_core::ReviewPolicy::Pause => tracker_core::ReviewPolicy::SkipUncertain, _ => tracker_core::ReviewPolicy::Pause };
                w.act("Changed autotracker settings", w.core.configure_tracking(sid, None, st), cx);
            })));
        strip = strip.child(div().flex_1());
        if let Some(j) = &job {
            let text = match j.state {
                JobState::Running | JobState::Queued => format!("{} · {} frames, {} found", j.state.label(), j.processed, j.accepted),
                JobState::Completed => match j.result.get("frames_per_second").and_then(|v| v.as_f64()) {
                    Some(fps) => format!("Completed · {} found in {} frames · {:.0} frames/s", j.accepted, j.processed, fps),
                    None => "Completed".into(),
                },
                JobState::Failed => format!("Failed: {}", j.error.as_ref().map(|e| e.message.clone()).unwrap_or_default()),
                other => other.label().to_string(),
            };
            strip = strip.child(div().text_size(px(12.)).text_color(if j.state == JobState::Failed { style::bad() } else { style::text_muted() }).child(text));
            if matches!(j.state, JobState::Running | JobState::Queued | JobState::AwaitingReview) {
                let job_id = j.job_id;
                strip = strip.child(button("at-stop", "Stop", true, false, cx.listener(move |w, _, _, cx| {
                    w.act("Stopped tracking", w.core.cancel_job(job_id), cx);
                })));
            }
        }
        let mut col = div().flex().flex_col().child(strip);
        if let Some((job, frame, e)) = s.review {
            col = col.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py(px(5.))
                    .bg(style::warn_soft())
                    .border_b_1()
                    .border_color(style::border())
                    .child(div().text_size(px(12.5)).text_color(style::warn()).font_weight(FontWeight::SEMIBOLD).child(format!("Uncertain at {:.3} s: {}", frame.seconds(), e.reason.message())))
                    .child(label(format!("score {:.3} · separation {:.3} · not a probability · the yellow circle is not recorded", e.score, e.separation)))
                    .child(div().flex_1())
                    .child(button("rv-accept", "Accept", e.candidate.is_some(), false, cx.listener(move |w, _, _, cx| {
                        w.report(w.core.review_match(sid, job, "accept", None, "You"), cx);
                    })))
                    .child(button("rv-correct", "Correct…", true, self.tool == Tool::Correct, cx.listener(|w, _, _, cx| {
                        w.tool = Tool::Correct;
                        w.message = Some(("Click the object's true position on this frame.".into(), false));
                        cx.notify();
                    })))
                    .child(button("rv-skip", "Skip Frame", true, false, cx.listener(move |w, _, _, cx| {
                        w.report(w.core.review_match(sid, job, "skip", None, "You"), cx);
                    })))
                    .child(button("rv-stop", "Stop", true, false, cx.listener(move |w, _, _, cx| {
                        w.report(w.core.review_match(sid, job, "stop", None, "You"), cx);
                    }))),
            );
        }
        col
    }

    fn render_video(&mut self, s: &Snapshot, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let message = match &s.status {
            SessionStatus::Loading => Some("Opening video and indexing frame timestamps…".to_string()),
            SessionStatus::Failed(m) => Some(format!("This video could not be opened. {m}")),
            SessionStatus::Ready if s.display.is_none() => Some(s.display_error.clone().map(|e| format!("Frame read failed: {e}")).unwrap_or_else(|| "Decoding frame…".into())),
            _ => None,
        };
        let image = self.render_image(s);
        let retired = self.retired.clone();
        let rect_out = self.video_rect.clone();
        let (sw, sh) = (s.width, s.height);
        let tracks = s.tracks.clone();
        let coords = s.coords;
        let viewer = s.viewer;
        let selected = s.selected;
        let review = s.review;
        let calib = self.calib_points.clone();
        let settings = s.settings;
        let canvas = canvas(
            move |_, _, _| (),
            move |bounds, _, window, _| {
                window.paint_quad(fill(bounds, style::video_bg()));
                for old in retired.borrow_mut().drain(..) {
                    let _ = window.drop_image(old);
                }
                if sw <= 0.0 || sh <= 0.0 {
                    rect_out.set(None);
                    return;
                }
                let bw = f32::from(bounds.size.width) as f64;
                let bh = f32::from(bounds.size.height) as f64;
                let scale = (bw / sw).min(bh / sh);
                let (w, h) = (sw * scale, sh * scale);
                let rect = Bounds::new(point(bounds.origin.x + px(((bw - w) / 2.0) as f32), bounds.origin.y + px(((bh - h) / 2.0) as f32)), size(px(w as f32), px(h as f32)));
                rect_out.set(Some(rect));
                if let Some(img) = &image {
                    let _ = window.paint_image(rect, rect, Corners::default(), img.clone(), 0, false);
                }
                let map = |p: Px| point(rect.origin.x + px((p.x * scale) as f32), rect.origin.y + px((p.y * scale) as f32));
                let line = |window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, c: Hsla, wdt: f32| {
                    let mut pb = PathBuilder::stroke(px(wdt));
                    pb.move_to(a);
                    pb.line_to(b);
                    if let Ok(p) = pb.build() {
                        window.paint_path(p, c);
                    }
                };
                // Coordinate axes.
                let o = map(coords.origin);
                let (sn, cs) = coords.angle.sin_cos();
                let l = (w.max(h) * 0.12) as f32;
                let axes: Hsla = style::axes().into();
                line(window, o, point(o.x + px(cs as f32 * l), o.y - px(sn as f32 * l)), axes, 2.0);
                line(window, o, point(o.x - px(sn as f32 * l), o.y - px(cs as f32 * l)), axes, 2.0);
                // Calibration tape.
                let tape: Hsla = style::tape().into();
                if let Some(c) = coords.calibration {
                    line(window, map(c.first), map(c.second), tape, 2.0);
                }
                if calib.len() == 1 {
                    let p = map(calib[0]);
                    window.paint_quad(fill(Bounds::new(point(p.x - px(3.), p.y - px(3.)), size(px(6.), px(6.))), tape));
                }
                // Tracks: trail of marks, current frame emphasised.
                for t in &tracks {
                    let color = style::track_color(t.color);
                    let is_sel = Some(t.id) == selected;
                    for m in &t.points {
                        let p = map(m.pixel);
                        let current = m.frame == viewer;
                        let r = px(if current { 5.0 } else if is_sel { 2.5 } else { 1.8 });
                        let rect = Bounds::new(point(p.x - r, p.y - r), size(r * 2., r * 2.));
                        if current {
                            window.paint_quad(outline(rect, color, BorderStyle::Solid));
                            line(window, point(p.x - px(9.), p.y), point(p.x + px(9.), p.y), color, 1.5);
                            line(window, point(p.x, p.y - px(9.)), point(p.x, p.y + px(9.)), color, 1.5);
                        } else {
                            window.paint_quad(fill(rect, color.opacity(if is_sel { 0.9 } else { 0.6 })));
                        }
                    }
                    // Template and search boxes around the selected track's mark on this frame.
                    if is_sel {
                        if let Some(m) = t.point_at(viewer) {
                            let p = map(m.pixel);
                            for (rad, alpha) in [(settings.template_radius as f64, 0.9), (settings.search_radius as f64, 0.5)] {
                                let r = px((rad * scale) as f32);
                                window.paint_quad(outline(Bounds::new(point(p.x - r, p.y - r), size(r * 2., r * 2.)), Hsla::white().opacity(alpha), BorderStyle::Dashed));
                            }
                        }
                    }
                }
                // Uncertain candidate (not recorded) and the prediction.
                if let Some((_, frame, e)) = review {
                    if frame == viewer {
                        let cand: Hsla = style::candidate().into();
                        if let Some(c) = e.candidate {
                            let p = map(c);
                            let r = px(11.);
                            window.paint_quad(quad(Bounds::new(point(p.x - r, p.y - r), size(r * 2., r * 2.)), r, Hsla::transparent_black(), px(2.), cand, BorderStyle::Solid));
                        }
                        if let Some(pr) = e.predicted {
                            let p = map(pr);
                            line(window, point(p.x - px(6.), p.y - px(6.)), point(p.x + px(6.), p.y + px(6.)), cand, 1.5);
                            line(window, point(p.x - px(6.), p.y + px(6.)), point(p.x + px(6.), p.y - px(6.)), cand, 1.5);
                        }
                    }
                }
            },
        )
        .size_full();
        let mut v = div()
            .id("video")
            .flex_1()
            .min_h(px(0.))
            .relative()
            .child(canvas)
            .cursor(if matches!(self.tool, Tool::Navigate) { CursorStyle::Arrow } else { CursorStyle::Crosshair })
            .on_mouse_down(MouseButton::Left, cx.listener(|w, ev: &MouseDownEvent, _, cx| w.video_click(ev, cx)));
        if let Some(m) = message {
            v = v.child(div().absolute().inset_0().flex().items_center().justify_center().child(div().px_3().py_2().rounded(px(4.)).bg(rgb(0x000000)).text_color(rgb(0xffffff)).text_size(px(13.)).child(m)));
        }
        v
    }

    fn render_player(&self, s: &Snapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let ready = s.status == SessionStatus::Ready;
        let sid = s.id;
        let frac = if s.frames > 1 { s.viewer_index as f32 / (s.frames - 1) as f32 } else { 0.0 };
        let duration = s.duration;
        let (first, last) = (s.first, s.last);
        let slider = div()
            .id("scrub")
            .flex_1()
            .h(px(16.))
            .relative()
            .cursor_pointer()
            .child(div().absolute().top(px(7.)).left_0().right_0().h(px(2.)).bg(style::border()))
            .child(div().absolute().top(px(7.)).left_0().h(px(2.)).w(relative(frac)).bg(style::accent()))
            .child(div().absolute().top(px(2.)).left(relative(frac)).ml(px(-5.)).size(px(12.)).rounded_full().bg(style::accent()))
            .when_some(s.clip_end.zip(last), |d, (end, last)| {
                let f = (end.seconds() / last.seconds().max(1e-9)) as f32;
                d.child(div().absolute().top(px(2.)).left(relative(f.min(1.0))).w(px(2.)).h(px(12.)).bg(style::warn()))
            })
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                let _ = (first, duration);
                if let Some(b) = w.scrub_bounds.get() {
                    let f = (f32::from(ev.position.x - b.origin.x) / f32::from(b.size.width)).clamp(0.0, 1.0);
                    if let Some(last) = last {
                        w.report(w.core.seek(sid, f as f64 * last.seconds()), cx);
                    }
                }
            }));
        let bounds_out = self.scrub_bounds.clone();
        let slider_measured = div().flex_1().child(slider).child(canvas(move |b, _, _| bounds_out.set(Some(b)), |_, _, _, _| {}).absolute().inset_0());
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(5.))
            .bg(style::panel_bg())
            .border_t_1()
            .border_color(style::border())
            .child(button("pl-start", "|◀", ready, false, cx.listener(|w, _, win, cx| w.first_frame(&FirstFrame, win, cx))))
            .child(button("pl-prev", "◀", ready, false, cx.listener(|w, _, _, cx| w.step(-1, cx))))
            .child(button("pl-play", if s.playing { "Pause" } else { "Play" }, ready, s.playing, cx.listener(|w, _, win, cx| w.play_pause(&PlayPause, win, cx))))
            .child(button("pl-next", "▶", ready, false, cx.listener(|w, _, _, cx| w.step(1, cx))))
            .child(div().relative().flex_1().mx_2().child(slider_measured))
            .child(div().text_size(px(12.)).text_color(style::text()).child(format!("frame {} of {}   t = {:.3} s", s.viewer_index + 1, s.frames, s.viewer.seconds())))
    }

    fn render_pane(&self, s: &Snapshot, top: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let view = if top { self.top } else { self.bottom };
        let track = s.tracks.iter().find(|t| Some(t.id) == s.selected).cloned();
        let rows: Vec<KinematicRow> = track.as_ref().map(|t| tracker_core::kinematics(t, &s.coords, 3.0)).unwrap_or_default();
        let unit = s.coords.unit();
        let mut header = div().flex().items_center().gap_1().px_2().py(px(4.)).bg(style::panel_bg()).border_b_1().border_color(style::border());
        for (v, name) in [(PaneView::Plot, "Plot"), (PaneView::Table, "Table")] {
            header = header.child(button(SharedString::from(format!("pane-{top}-{name}")), name, true, v == view, cx.listener(move |w, _, _, cx| {
                if top { w.top = v } else { w.bottom = v }
                cx.notify();
            })));
        }
        if view == PaneView::Plot {
            header = header.child(separator());
            for var in Var::ALL {
                let active = self.plot_var == var;
                header = header.child(button(SharedString::from(format!("var-{top}-{}", var.name())), var.name(), true, active, cx.listener(move |w, _, _, cx| {
                    w.plot_var = var;
                    cx.notify();
                })));
            }
        }
        header = header.child(div().flex_1()).child(label(track.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| "no track".into())));
        let body: AnyElement = match view {
            PaneView::Plot => {
                let color = track.as_ref().map(|t| style::track_color(t.color)).unwrap_or_else(|| style::accent().into());
                plot::plot(&rows, self.plot_var, unit, color, s.viewer.seconds()).into_any_element()
            }
            PaneView::Table => {
                let mut head = div().flex().px_2().h(px(22.)).items_center().bg(style::panel_bg()).border_b_1().border_color(style::hairline());
                for (i, c) in plot::TABLE_COLUMNS.iter().enumerate() {
                    let text = if i == 0 { c.to_string() } else if i <= 2 { format!("{c} ({unit})") } else if i <= 4 { format!("{c} ({unit}/s)") } else { format!("{c} ({unit}/s²)") };
                    head = head.child(div().flex_1().min_w(px(0.)).overflow_hidden().whitespace_nowrap().text_size(px(10.5)).text_color(style::text_muted()).child(text));
                }
                head = head.child(div().w(px(18.)).flex_none().text_size(px(10.5)).text_color(style::text_muted()).child("via"));
                let viewer = s.viewer;
                let rows = Rc::new(rows);
                let n = rows.len();
                let list = uniform_list(SharedString::from(format!("table-{top}")), n, move |range, _, _| range.map(|i| plot::table_row(&rows[i], rows[i].frame == viewer)).collect::<Vec<_>>()).flex_1();
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .bg(style::surface())
                    .child(head)
                    .child(if n == 0 { div().p_3().child(label("Marked and tracked points appear here, with velocity and acceleration.")).into_any_element() } else { list.into_any_element() })
                    .child(div().px_2().py(px(3.)).border_t_1().border_color(style::hairline()).child(label("M manual · A automatic · R reviewed. Velocities: three-point differences on source timestamps.")))
                    .into_any_element()
            }
        };
        div().size_full().flex().flex_col().bg(style::surface()).child(header).child(body)
    }

    fn render_status(&self, s: Option<&Snapshot>) -> impl IntoElement {
        let (running, queued) = self.core.scheduler_load();
        let text = match (&self.message, s) {
            (Some((m, _)), _) => m.clone(),
            (None, None) => "Open a video to begin.".into(),
            (None, Some(s)) => match &s.status {
                SessionStatus::Loading => "Indexing frame timestamps…".into(),
                SessionStatus::Failed(m) => m.clone(),
                SessionStatus::Ready => match (s.tracks.is_empty(), s.coords.calibration.is_none()) {
                    (true, _) => "Choose New Point Mass, then click the object in the video.".into(),
                    (false, true) => "Positions are in pixels until you set a Calibration from an object of known length.".into(),
                    _ => format!("{} · revision {}{}", if s.dirty { "Unsaved changes" } else { "Saved" }, s.revision, s.undo_label.as_ref().map(|l| format!(" · Undo: {l}")).unwrap_or_default()),
                },
            },
        };
        let error = self.message.as_ref().is_some_and(|m| m.1);
        let points: usize = s.map_or(0, |s| s.tracks.iter().map(|t| t.points.len()).sum());
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .h(px(24.))
            .bg(style::panel_bg())
            .border_t_1()
            .border_color(style::border())
            .child(div().flex_1().text_size(px(12.)).text_color(if error { style::bad() } else { style::text_muted() }).child(text))
            .child(label(format!("{points} points · {}", s.map_or("px", |s| s.coords.unit()))))
            .child(label(format!("tracking {running} running, {queued} queued, {} slots", self.core.tracking_slots())))
    }

    fn render_agent_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.core.control_enabled();
        let agents = self.core.connected_agents();
        let last = self.core.activity(1).into_iter().next();
        let state = match (&self.server_error, enabled) {
            (Some(e), _) => format!("Agent control unavailable: {e}"),
            (None, true) if agents.is_empty() => "Agent control allowed · no agent connected".into(),
            (None, true) => format!("Agent control allowed · {} connected", agents.join(", ")),
            (None, false) => "Agent control stopped".into(),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .h(px(30.))
            .bg(style::window_bg())
            .border_t_1()
            .border_color(style::border())
            .child(div().size(px(8.)).rounded_full().bg(if enabled && self.server_error.is_none() { style::ok() } else { rgb(0x9aa1ab) }))
            .child(div().text_size(px(12.)).text_color(style::text()).child(state))
            .when_some(last, |d, a| d.child(label(format!("Last: {} · {}", a.actor, a.text))))
            .child(div().flex_1())
            .child(button("ag-activity", "Agent Activity", true, self.show_activity, cx.listener(|w, _, win, cx| w.show_activity(&ShowActivity, win, cx))))
            .child(button("ag-toggle", if enabled { "Stop Agent Control" } else { "Allow Agent Control" }, self.server_error.is_none(), false, cx.listener(|w, _, win, cx| w.toggle_agent_control(&ToggleAgentControl, win, cx))))
    }

    fn render_activity(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.core.activity(40);
        let mut list = div().flex().flex_col().gap_1();
        if entries.is_empty() {
            list = list.child(label("No activity yet. Actions by you and connected agents appear here."));
        }
        for e in entries {
            let local = chrono::DateTime::<chrono::Local>::from(e.at).format("%H:%M:%S");
            list = list.child(div().text_size(px(12.)).child(format!("{local}  {}  {}", e.actor, e.text)));
        }
        div()
            .id("activity")
            .absolute()
            .right(px(12.))
            .bottom(px(60.))
            .w(px(460.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .p_3()
            .rounded(px(6.))
            .border_1()
            .border_color(style::border())
            .bg(style::surface())
            .shadow_md()
            .child(div().flex().items_center().mb_2().child(div().flex_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("Agent and user activity")).child(button("act-close", "Close", true, false, cx.listener(|w, _, _, cx| {
                w.show_activity = false;
                cx.notify();
            }))))
            .child(list)
            .child(div().mt_2().child(label("Jobs belong to Tracker: disconnecting an agent never cancels its work. Stop Agent Control blocks new requests.")))
    }

    fn render_dialog(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.dialog_focus;
        let (title, hint) = match self.dialog.as_ref().unwrap() {
            Dialog::Calibration { first, second, .. } => ("Calibration", format!("Endpoints ({:.1}, {:.1}) and ({:.1}, {:.1}) px, {:.1} px apart. Enter their real distance.", first.x, first.y, second.x, second.y, first.distance(*second))),
            Dialog::Clip { .. } => ("Clip Settings", "Leave start or end empty to use the whole video. Step 2 tracks every second frame.".to_string()),
            Dialog::Dataset { .. } => ("Add Track to Dataset", format!("Stores the selected track's positions, velocities, accelerations and calibration as one run. Datasets live in {}; export them as XLSX or CSV for Google Sheets.", self.core.datasets_root().display())),
            Dialog::Rename { .. } => ("Rename Track", String::new()),
        };
        let unit_row = if let Some(Dialog::Calibration { unit, .. }) = &self.dialog {
            let current = *unit;
            let mut row = div().flex().items_center().gap_1().child(div().w(px(120.)).text_size(px(12.5)).child("Unit"));
            for u in ["m", "cm", "mm"] {
                row = row.child(button(SharedString::from(format!("unit-{u}")), u, true, current == u, cx.listener(move |w, _, _, cx| {
                    if let Some(Dialog::Calibration { unit, .. }) = w.dialog.as_mut() {
                        *unit = u;
                    }
                    cx.notify();
                })));
            }
            Some(row)
        } else {
            None
        };
        let mut fields = div().flex().flex_col().gap_2();
        if let Some(d) = self.dialog.as_mut() {
            for (i, f) in d.fields().into_iter().enumerate() {
                fields = fields.child(div().id(SharedString::from(format!("field-{i}"))).cursor_text().child(f.render(i == focus)).on_click(cx.listener(move |w, _, _, cx| {
                    w.dialog_focus = i;
                    cx.notify();
                })));
            }
        }
        div().absolute().inset_0().flex().items_center().justify_center().bg(Hsla::black().opacity(0.25)).child(
            div()
                .w(px(520.))
                .p_4()
                .rounded(px(8.))
                .bg(style::surface())
                .border_1()
                .border_color(style::border())
                .shadow_lg()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .when(!hint.is_empty(), |d| d.child(div().text_size(px(12.5)).text_color(style::text_muted()).child(hint)))
                .child(fields)
                .when_some(unit_row, |d, r| d.child(r))
                .child(label("Type to edit · Tab next field · Enter apply · Esc cancel"))
                .child(div().flex().justify_end().gap_2().child(button("dlg-cancel", "Cancel", true, false, cx.listener(|w, _, _, cx| {
                    w.dialog = None;
                    w.calib_points.clear();
                    cx.notify();
                }))).child(button("dlg-ok", "Apply", true, true, cx.listener(|w, _, _, cx| w.apply_dialog(cx))))),
        )
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex_1().flex().items_center().justify_center().bg(style::video_bg()).child(
            div()
                .w(px(520.))
                .p_5()
                .rounded(px(8.))
                .bg(style::surface())
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("Open a video to begin"))
                .child(div().text_size(px(13.)).text_color(style::text_muted()).child("Mark an object frame by frame or autotrack it, set a scale from something of known length, and read positions, velocities and accelerations. Projects from the Swift app (.tracker.json) open too."))
                .child(div().text_size(px(13.)).text_color(style::text_muted()).child("AI agents can run the same analysis through the tracker-mcp server. Their actions appear in the bar at the bottom, and you can stop them at any time."))
                .child(div().flex().gap_2().child(button("empty-open", "Open Video…", true, true, cx.listener(|w, _, win, cx| w.open_video(&OpenVideo, win, cx))))),
        )
    }
}
