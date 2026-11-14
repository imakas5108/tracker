//! Agent control: validates tool calls against the shared catalog, enforces request IDs and
//! revisions, dispatches to the same [`App`] commands the UI uses, and logs every action.

use crate::preview::{Crop, render_png};
use crate::{AgentClient, App, AppError, AppEvent, Result, fail};
use base64::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracker_core::{Algorithm, MaskShape, Point, ReviewPolicy};
use tracker_ipc::{ErrorBody, Request, Response};
use uuid::Uuid;

/// Recent mutation results by request ID, so retries return the original outcome.
#[derive(Default)]
pub(crate) struct RequestCache {
    map: HashMap<Uuid, (String, Response)>,
    order: VecDeque<Uuid>,
}

const REQUEST_LIMIT: usize = 2000;

impl RequestCache {
    fn get(&self, id: &Uuid) -> Option<&(String, Response)> {
        self.map.get(id)
    }
    fn put(&mut self, id: Uuid, fingerprint: String, r: Response) {
        if self.map.insert(id, (fingerprint, r)).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > REQUEST_LIMIT {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

struct Args<'a>(&'a Map<String, Value>);

impl Args<'_> {
    fn str(&self, k: &str) -> Result<&str> {
        self.0.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).ok_or_else(|| AppError::new("invalid_argument", format!("{k} must be a non-empty string.")))
    }
    fn opt_str(&self, k: &str) -> Option<&str> {
        self.0.get(k).and_then(|v| v.as_str())
    }
    fn num(&self, k: &str) -> Result<f64> {
        self.0.get(k).and_then(|v| v.as_f64()).filter(|v| v.is_finite()).ok_or_else(|| AppError::new("invalid_argument", format!("{k} must be a finite number.")))
    }
    fn opt_num(&self, k: &str) -> Option<f64> {
        self.0.get(k).and_then(|v| v.as_f64())
    }
    fn opt_u64(&self, k: &str) -> Option<u64> {
        self.0.get(k).and_then(|v| v.as_u64())
    }
    fn opt_bool(&self, k: &str) -> Option<bool> {
        self.0.get(k).and_then(|v| v.as_bool())
    }
    fn id(&self, k: &str) -> Result<Uuid> {
        Uuid::parse_str(self.str(k)?).map_err(|_| AppError::new("invalid_argument", format!("{k} must be a UUID.")))
    }
    fn revision(&self) -> Option<u64> {
        self.opt_u64("expected_revision")
    }
    fn path(&self, k: &str) -> Result<PathBuf> {
        let p = self.str(k)?;
        if !p.starts_with('/') || p.contains('\0') {
            return fail("invalid_path", format!("{k} must be an absolute local path."));
        }
        Ok(Path::new(p).to_path_buf())
    }
}

/// Handles one control request. Used by the socket server and directly by tests.
pub fn handle_request(app: &App, req: Request) -> Response {
    let actor = req.client_name.clone().unwrap_or_else(|| "Agent".into());
    match req.command.as_str() {
        "__disconnect" => {
            app.lock().clients.remove(&req.client_id);
            app.emit(AppEvent::Control);
            return Response::ok(json!({"disconnected": true}));
        }
        "__hello" | "__heartbeat" => {
            if !app.control_enabled() {
                return Response::err(ErrorBody::new("control_disabled", "Agent control is off. Choose Allow Agent Control in Tracker's status bar."));
            }
            let first = app.lock().clients.insert(req.client_id, AgentClient { name: actor.clone(), last_seen: Instant::now() }).is_none();
            if first {
                app.log(&actor, "Connected");
            }
            app.emit(AppEvent::Control);
            return Response::ok(json!({"protocol_version": tracker_ipc::PROTOCOL_VERSION, "app": "tracker-rs", "app_version": env!("CARGO_PKG_VERSION"), "control_enabled": true}));
        }
        _ => {}
    }
    if !app.control_enabled() {
        return Response::err(ErrorBody::new("control_disabled", "Agent control is off. Choose Allow Agent Control in Tracker's status bar."));
    }
    app.lock().clients.insert(req.client_id, AgentClient { name: actor.clone(), last_seen: Instant::now() });
    let tool = match tracker_ipc::validate(&req.command, &req.arguments) {
        Ok(t) => t,
        Err(e) => return Response::err(e),
    };
    if tool.read_only {
        return match dispatch(app, &req.command, &req.arguments, &actor) {
            Ok(v) => Response::ok(v),
            Err(e) => Response::err(e),
        };
    }
    let request_id = match Args(&req.arguments).id("request_id") {
        Ok(id) => id,
        Err(e) => return Response::err(e),
    };
    let fingerprint = serde_json::to_string(&json!({"command": req.command, "arguments": req.arguments})).unwrap_or_default();
    if let Some((fp, response)) = app.lock().requests.get(&request_id).cloned() {
        return if fp == fingerprint { response } else { Response::err(ErrorBody::new("request_id_conflict", "This request_id was already used with different arguments.")) };
    }
    let result = dispatch(app, &req.command, &req.arguments, &actor);
    let response = match &result {
        Ok(v) => Response::ok(v.clone()),
        Err(e) => Response::err(e.clone()),
    };
    app.lock().requests.put(request_id, fingerprint, response.clone());
    let label = describe_action(app, &req.command, &req.arguments);
    app.log(&actor, match &result {
        Ok(_) => label,
        Err(e) => format!("{label} (refused: {})", e.code),
    });
    response
}

fn describe_action(app: &App, cmd: &str, a: &Map<String, Value>) -> String {
    let args = Args(a);
    let session = args.id("session_id").ok().and_then(|id| app.with_session(id, |s| s.name.clone()).ok());
    let file = |k: &str| args.opt_str(k).map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_default();
    let text = match cmd {
        "tracker_import_video" => format!("Opened video {}", file("path")),
        "tracker_open_project" => format!("Opened project {}", file("path")),
        "tracker_select_session" => "Selected video".into(),
        "tracker_seek" => format!("Moved viewer to {:.3} s", args.opt_num("time").unwrap_or(0.0)),
        "tracker_close_session" => "Closed video".into(),
        "tracker_add_track" => format!("Added track {}", args.opt_str("name").unwrap_or("")),
        "tracker_rename_track" => format!("Renamed track to {}", args.opt_str("name").unwrap_or("")),
        "tracker_delete_track" => "Deleted a track".into(),
        "tracker_mark_point" => format!("Marked ({:.1}, {:.1}) at {:.3} s", args.opt_num("x").unwrap_or(0.0), args.opt_num("y").unwrap_or(0.0), args.opt_num("time").unwrap_or(0.0)),
        "tracker_remove_point" => format!("Removed point at {:.3} s", args.opt_num("time").unwrap_or(0.0)),
        "tracker_set_calibration" => format!("Set scale {} {}", args.opt_num("length").unwrap_or(0.0), args.opt_str("unit").unwrap_or("")),
        "tracker_set_axes" => "Moved axes".into(),
        "tracker_set_clip" => "Changed clip range".into(),
        "tracker_configure_tracking" => "Changed autotracker settings".into(),
        "tracker_start_tracking" => format!("Started autotracking from {:.3} s", args.opt_num("from_time").unwrap_or(0.0)),
        "tracker_review_match" => format!("Review: {}", args.opt_str("action").unwrap_or("")),
        "tracker_cancel_job" => "Stopped a job".into(),
        "tracker_undo" => "Undid an edit".into(),
        "tracker_redo" => "Redid an edit".into(),
        "tracker_save_project" => format!("Saved project {}", file("path")),
        "tracker_export_csv" => format!("Exported CSV {}", file("path")),
        "tracker_dataset_create" => format!("Created dataset {}", args.opt_str("name").unwrap_or("")),
        "tracker_dataset_add_run" => format!("Stored a run in dataset {}", args.opt_str("dataset").unwrap_or("")),
        "tracker_dataset_remove_run" => format!("Removed a run from dataset {}", args.opt_str("dataset").unwrap_or("")),
        "tracker_dataset_export" => format!("Exported dataset {} to {}", args.opt_str("dataset").unwrap_or(""), file("path")),
        other => other.to_string(),
    };
    match session {
        Some(name) => format!("{text} · {name}"),
        None => text,
    }
}

fn job_json(app: &App, r: &crate::JobRecord) -> Value {
    let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
    if let Ok(rev) = app.with_session(r.session_id, |s| s.revision()) {
        v["session_revision"] = json!(rev);
    }
    v
}

impl App {
    /// JSON description of a session for agents.
    pub fn describe(&self, id: Uuid) -> Result<Value> {
        let active = self.active_tracking(id).map(|j| json!({"job_id": j.job_id, "state": j.state, "track_id": j.track_id}));
        self.with_session(id, |s| {
            let a = s.history.state();
            let info = s.info.as_ref();
            json!({
                "session_id": s.id,
                "name": s.name,
                "source_path": s.path,
                "status": match &s.status { crate::SessionStatus::Loading => "loading", crate::SessionStatus::Ready => "ready", crate::SessionStatus::Failed(_) => "failed" },
                "error": match &s.status { crate::SessionStatus::Failed(m) => json!(m), _ => Value::Null },
                "revision": s.revision(),
                "dirty": s.is_dirty(),
                "project_path": s.project_path,
                "width": info.map(|i| i.width), "height": info.map(|i| i.height),
                "duration": info.map(|i| i.duration),
                "frame_count": info.map(|i| i.index.len()),
                "first_frame_time": info.and_then(|i| i.index.first()).map(|f| f.seconds()),
                "last_frame_time": info.and_then(|i| i.index.last()).map(|f| f.seconds()),
                "nominal_fps": info.map(|i| i.nominal_fps),
                "viewer_time": s.viewer.seconds(),
                "playing": s.is_playing(),
                "coordinates": {"origin_x": a.coords.origin.x, "origin_y": a.coords.origin.y, "angle_deg": a.coords.angle.to_degrees(), "unit": a.coords.unit(), "y_axis": "up"},
                "calibration": a.coords.calibration.map(|c| json!({"x1": c.first.x, "y1": c.first.y, "x2": c.second.x, "y2": c.second.y, "length": c.length, "unit": c.unit.symbol(), "pixels_per_unit": c.pixels_per_unit()})),
                "clip": {"start_time": a.clip.start.map(|f| f.seconds()), "end_time": a.clip.end.map(|f| f.seconds()), "step": a.clip.step},
                "tracking_settings": settings_json(&s.settings),
                "tracks": a.tracks.iter().map(|t| json!({"track_id": t.id, "name": t.name, "points": t.points.len(), "first_time": t.points.first().map(|p| p.time()), "last_time": t.points.last().map(|p| p.time())})).collect::<Vec<_>>(),
                "selected_track_id": s.selected_track,
                "active_job": active,
                "pending_review": s.review.as_ref().map(|r| json!({"job_id": r.job_id, "time": r.frame.seconds()})),
                "can_undo": s.history.can_undo(), "can_redo": s.history.can_redo(),
            })
        })
    }
}

fn settings_json(s: &tracker_core::TrackingSettings) -> Value {
    json!({
        "algorithm": match s.algorithm { Algorithm::NccV2 => "masked_ncc", Algorithm::SsdV1 => "ssd_v1" },
        "algorithm_id": s.algorithm.id(),
        "template_radius": s.template_radius, "search_radius": s.search_radius,
        "minimum_score": s.minimum_score, "minimum_separation": s.minimum_separation,
        "predict": s.predict, "evolve_rate": s.evolve_rate, "tether": s.tether,
        "mask": match s.mask { MaskShape::Circle => "circle", MaskShape::Square => "square" },
        "review_policy": match s.review_policy { ReviewPolicy::Pause => "pause", ReviewPolicy::SkipUncertain => "skip_uncertain" },
        "max_gap": s.max_gap,
    })
}

fn dispatch(app: &App, cmd: &str, a: &Map<String, Value>, actor: &str) -> Result<Value> {
    let args = Args(a);
    let sid = || args.id("session_id");
    let rev = args.revision();
    match cmd {
        "tracker_list_sessions" => {
            let ids = app.session_ids();
            let sessions: Vec<Value> = ids.iter().filter_map(|id| app.describe(*id).ok()).collect();
            Ok(json!({"sessions": sessions, "selected_session_id": app.selected(), "tracking_slots": app.tracking_slots(), "datasets_folder": app.datasets_root()}))
        }
        "tracker_import_video" => {
            let (session, job) = app.import_video(&args.path("path")?)?;
            Ok(json!({"session_id": session, "job_id": job, "state": app.job(job).map(|j| j.state)}))
        }
        "tracker_open_project" => {
            let relink = args.opt_str("relink_path").map(|_| args.path("relink_path")).transpose()?;
            let (session, job) = app.open_project(&args.path("path")?, relink.as_deref())?;
            Ok(json!({"session_id": session, "job_id": job}))
        }
        "tracker_get_session" => app.describe(sid()?),
        "tracker_select_session" => {
            app.select(sid()?)?;
            Ok(json!({"selected_session_id": sid()?}))
        }
        "tracker_seek" => {
            let f = app.seek(sid()?, args.num("time")?)?;
            Ok(json!({"time": f.seconds(), "frame": {"value": f.value, "timescale": f.timescale}}))
        }
        "tracker_close_session" => {
            app.close(sid()?, args.opt_bool("discard").unwrap_or(false))?;
            Ok(json!({"closed": true}))
        }
        "tracker_get_frame" => {
            let id = sid()?;
            let (info, analysis, viewer, revision) = app.with_session(id, |s| s.ready().cloned().map(|i| (i, s.history.state().clone(), s.viewer, s.revision())))??;
            let stamp = match args.opt_num("time") {
                Some(t) => info.index.stamp_at(t).ok_or_else(|| AppError::new("invalid_time", "Time is outside this video."))?,
                None => viewer,
            };
            let frame = tracker_media::frame_at(&info, stamp, true).map_err(|e| AppError::new("media_error", e.to_string()))?;
            let crop = a.get("crop").and_then(|c| c.as_object()).map(|c| {
                let g = |k: &str| c.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                Crop { x: g("x"), y: g("y"), width: g("width"), height: g("height") }
            });
            let overlays = args.opt_bool("overlays").unwrap_or(false).then_some(&analysis);
            let (png, mut meta) = render_png(frame.rgba.as_deref().unwrap_or(&[]), info.width, info.height, stamp, crop, args.opt_u64("max_edge").unwrap_or(1280) as usize, overlays)?;
            meta["revision"] = json!(revision);
            meta["image_base64"] = json!(base64::engine::general_purpose::STANDARD.encode(png));
            meta["mime_type"] = json!("image/png");
            Ok(meta)
        }
        "tracker_add_track" => {
            let (track, revision) = app.add_track(sid()?, rev, args.opt_str("name"))?;
            Ok(json!({"track_id": track, "revision": revision}))
        }
        "tracker_rename_track" => Ok(json!({"revision": app.rename_track(sid()?, rev, args.id("track_id")?, args.str("name")?)?})),
        "tracker_delete_track" => Ok(json!({"revision": app.delete_track(sid()?, rev, args.id("track_id")?)?})),
        "tracker_mark_point" => {
            let (f, revision) = app.mark_point(sid()?, rev, args.id("track_id")?, args.num("time")?, Point::new(args.num("x")?, args.num("y")?))?;
            Ok(json!({"time": f.seconds(), "frame": {"value": f.value, "timescale": f.timescale}, "revision": revision}))
        }
        "tracker_remove_point" => Ok(json!({"revision": app.remove_point(sid()?, rev, args.id("track_id")?, args.num("time")?)?})),
        "tracker_set_calibration" => {
            let r = app.set_calibration(sid()?, rev, Point::new(args.num("x1")?, args.num("y1")?), Point::new(args.num("x2")?, args.num("y2")?), args.num("length")?, args.str("unit")?)?;
            Ok(json!({"revision": r}))
        }
        "tracker_set_axes" => Ok(json!({"revision": app.set_axes(sid()?, rev, Point::new(args.num("origin_x")?, args.num("origin_y")?), args.opt_num("angle_deg").unwrap_or(0.0))?})),
        "tracker_set_clip" => Ok(json!({"revision": app.set_clip(sid()?, rev, args.opt_num("start_time"), args.opt_num("end_time"), args.opt_u64("step").map(|s| s as u32))?})),
        "tracker_configure_tracking" => {
            let id = sid()?;
            let mut s = app.with_session(id, |s| s.settings)?;
            if let Some(v) = args.opt_str("algorithm") {
                s.algorithm = if v == "ssd_v1" { Algorithm::SsdV1 } else { Algorithm::NccV2 };
            }
            if let Some(v) = args.opt_u64("template_radius") {
                s.template_radius = v as u32;
            }
            if let Some(v) = args.opt_u64("search_radius") {
                s.search_radius = v as u32;
            }
            if let Some(v) = args.opt_num("minimum_score") {
                s.minimum_score = v;
            }
            if let Some(v) = args.opt_num("minimum_separation") {
                s.minimum_separation = v;
            }
            if let Some(v) = args.opt_bool("predict") {
                s.predict = v;
            }
            if let Some(v) = args.opt_num("evolve_rate") {
                s.evolve_rate = v;
            }
            if let Some(v) = args.opt_num("tether") {
                s.tether = v;
            }
            if let Some(v) = args.opt_str("mask") {
                s.mask = if v == "square" { MaskShape::Square } else { MaskShape::Circle };
            }
            if let Some(v) = args.opt_str("review_policy") {
                s.review_policy = if v == "skip_uncertain" { ReviewPolicy::SkipUncertain } else { ReviewPolicy::Pause };
            }
            if let Some(v) = args.opt_u64("max_gap") {
                s.max_gap = v as u32;
            }
            let r = app.configure_tracking(id, rev, s)?;
            Ok(json!({"revision": r, "tracking_settings": settings_json(&s)}))
        }
        "tracker_start_tracking" => {
            let job = app.start_tracking(sid()?, rev, args.id("track_id")?, args.num("from_time")?)?;
            Ok(job_json(app, &app.job(job).unwrap()))
        }
        "tracker_get_job" => {
            let id = args.id("job_id")?;
            let wait = args.opt_num("wait_seconds").unwrap_or(0.0).clamp(0.0, 30.0);
            let r = app.wait_job(id, Duration::from_secs_f64(wait)).ok_or_else(|| AppError::new("job_not_found", "No job with this ID in this app session."))?;
            Ok(job_json(app, &r))
        }
        "tracker_list_jobs" => {
            let session = args.opt_str("session_id").map(|_| args.id("session_id")).transpose()?;
            Ok(json!({"jobs": app.jobs(session).iter().map(|j| job_json(app, j)).collect::<Vec<_>>()}))
        }
        "tracker_get_review" => app.with_session(sid()?, |s| match &s.review {
            None => json!({"pending": false, "revision": s.revision()}),
            Some(r) => json!({
                "pending": true, "job_id": r.job_id, "track_id": r.track_id,
                "time": r.frame.seconds(), "frame": {"value": r.frame.value, "timescale": r.frame.timescale},
                "candidate": r.evidence.candidate.map(|p| json!({"x": p.x, "y": p.y})),
                "predicted": r.evidence.predicted.map(|p| json!({"x": p.x, "y": p.y})),
                "score": r.evidence.score, "separation": r.evidence.separation,
                "reason": r.evidence.reason, "explanation": r.evidence.reason.message(),
                "recorded": false, "score_is_probability": false, "revision": s.revision(),
            }),
        }),
        "tracker_review_match" => {
            let correction = match (args.opt_num("x"), args.opt_num("y")) {
                (Some(x), Some(y)) => Some(Point::new(x, y)),
                _ => None,
            };
            let next = app.review_match(sid()?, args.id("job_id")?, args.str("action")?, correction, actor)?;
            Ok(json!({"continuation_job_id": next, "job": next.and_then(|j| app.job(j)).map(|j| job_json(app, &j))}))
        }
        "tracker_cancel_job" => Ok(job_json(app, &app.cancel_job(args.id("job_id")?)?)),
        "tracker_undo" => {
            let (label, r) = app.undo(sid()?, rev)?;
            Ok(json!({"undone": label, "revision": r}))
        }
        "tracker_redo" => {
            let (label, r) = app.redo(sid()?, rev)?;
            Ok(json!({"redone": label, "revision": r}))
        }
        "tracker_get_measurements" => {
            let id = sid()?;
            let track = args.id("track_id")?;
            let derived = args.opt_bool("derived").unwrap_or(true);
            let offset = args.opt_u64("offset").unwrap_or(0) as usize;
            let limit = args.opt_u64("limit").unwrap_or(500) as usize;
            if !(1..=5000).contains(&limit) {
                return fail("invalid_pagination", "limit must be 1 to 5000.");
            }
            app.with_session(id, |s| -> Result<Value> {
                if let Some(e) = rev {
                    if e != s.revision() {
                        return fail("stale_revision", "The session changed; restart pagination from offset 0.");
                    }
                }
                let t = s.require_track(track)?;
                let a = s.history.state();
                let rows = tracker_core::kinematics(t, &a.coords, 3.0);
                let total = rows.len();
                if offset > total {
                    return fail("invalid_pagination", "offset is past the last row.");
                }
                let page: Vec<Value> = rows[offset..(offset + limit).min(total)]
                    .iter()
                    .map(|r| {
                        let mut v = json!({"time": r.t, "frame": {"value": r.frame.value, "timescale": r.frame.timescale}, "pixel_x": r.pixel_x, "pixel_y": r.pixel_y, "x": r.x, "y": r.y, "origin": r.origin.name(), "match_score": r.score});
                        if derived {
                            v["vx"] = json!(r.vx);
                            v["vy"] = json!(r.vy);
                            v["speed"] = json!(r.speed);
                            v["ax"] = json!(r.ax);
                            v["ay"] = json!(r.ay);
                        }
                        v
                    })
                    .collect();
                let end = (offset + limit).min(total);
                Ok(json!({
                    "revision": s.revision(), "track_id": t.id, "track_name": t.name, "unit": a.coords.unit(),
                    "total": total, "offset": offset, "next_offset": (end < total).then_some(end),
                    "measurements": page,
                    "fit": if derived { serde_json::to_value(tracker_dataset::fit_motion(&rows)).unwrap_or(Value::Null) } else { Value::Null },
                    "notes": "x, y in the session's coordinate system (y up); pixel_x, pixel_y are source pixels (top-left origin). Derivatives are three-point finite differences on source timestamps. match_score is a similarity, not a probability.",
                }))
            })?
        }
        "tracker_save_project" => {
            let r = app.save_project(sid()?, &args.path("path")?, args.opt_bool("overwrite").unwrap_or(false))?;
            Ok(json!({"path": args.str("path")?, "saved_revision": r}))
        }
        "tracker_export_csv" => {
            let rows = app.export_csv(sid()?, args.id("track_id")?, &args.path("path")?, args.opt_bool("overwrite").unwrap_or(false))?;
            Ok(json!({"path": args.str("path")?, "rows": rows}))
        }
        "tracker_dataset_list" => Ok(json!({
            "folder": app.datasets_root(),
            "datasets": app.dataset_list()?.iter().map(|m| json!({"name": m.name, "description": m.description, "runs": m.runs.len(), "updated_at": m.updated_at})).collect::<Vec<_>>(),
        })),
        "tracker_dataset_create" => {
            let ds = app.dataset_create(args.str("name")?, args.opt_str("description").unwrap_or(""))?;
            Ok(json!({"name": ds.manifest.name, "folder": ds.dir, "dataset_id": ds.manifest.id}))
        }
        "tracker_dataset_add_run" => {
            let conditions: BTreeMap<String, String> = a.get("conditions").and_then(|c| c.as_object()).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
            let run = app.dataset_add_run(args.str("dataset")?, sid()?, args.id("track_id")?, actor, conditions, args.opt_str("notes").unwrap_or(""))?;
            let ds = app.dataset_open(args.str("dataset")?)?;
            let rec = ds.manifest.runs.iter().find(|r| r.run_id == run).cloned();
            Ok(json!({"run_id": run, "dataset": ds.manifest.name, "runs": ds.manifest.runs.len(), "observations": ds.observations.len(), "run": rec}))
        }
        "tracker_dataset_get" => {
            let ds = app.dataset_open(args.str("dataset")?)?;
            Ok(json!({"name": ds.manifest.name, "description": ds.manifest.description, "folder": ds.dir, "created_at": ds.manifest.created_at, "observations": ds.observations.len(), "runs": ds.manifest.runs}))
        }
        "tracker_dataset_rows" => {
            let ds = app.dataset_open(args.str("dataset")?)?;
            let limit = args.opt_u64("limit").unwrap_or(1000) as usize;
            if !(1..=5000).contains(&limit) {
                return fail("invalid_pagination", "limit must be 1 to 5000.");
            }
            let offset = args.opt_u64("offset").unwrap_or(0) as usize;
            let sheet = args.str("sheet")?;
            let (values, total) = ds.sheet_values(sheet, offset, limit).map_err(|e| AppError::new("dataset_error", e.to_string()))?;
            let end = (offset + limit).min(total);
            Ok(json!({"sheet": sheet, "total_rows": total, "offset": offset, "next_offset": (end < total).then_some(end), "values": values,
                      "google_sheets": "values is header row + data rows, ready for spreadsheets.values.update or append. Write the header only with the first page."}))
        }
        "tracker_dataset_remove_run" => {
            let mut ds = app.dataset_open(args.str("dataset")?)?;
            ds.remove_run(args.id("run_id")?).map_err(|e| AppError::new("dataset_error", e.to_string()))?;
            Ok(json!({"runs": ds.manifest.runs.len()}))
        }
        "tracker_dataset_export" => {
            let ds = app.dataset_open(args.str("dataset")?)?;
            let dest = args.path("path")?;
            let overwrite = args.opt_bool("overwrite").unwrap_or(false);
            let map = |e: tracker_dataset::DatasetError| match e {
                tracker_dataset::DatasetError::Write(tracker_core::io::WriteError::Exists(_)) => AppError::new("destination_exists", "Destination exists; pass overwrite=true to replace it."),
                other => AppError::new("file_write_failed", other.to_string()),
            };
            match args.str("format")? {
                "xlsx" => ds.export_xlsx(&dest, overwrite).map_err(map)?,
                _ => {
                    let sheet = args.opt_str("sheet").unwrap_or("observations");
                    let data = ds.csv(sheet).map_err(map)?;
                    tracker_core::io::atomic_write(&dest, &data, overwrite).map_err(|e| map(e.into()))?;
                }
            }
            Ok(json!({"path": dest, "runs": ds.manifest.runs.len(), "observations": ds.observations.len()}))
        }
        "tracker_get_activity" => {
            let limit = args.opt_u64("limit").unwrap_or(30).min(200) as usize;
            Ok(json!({"activity": app.activity(limit).iter().map(|e| json!({"actor": e.actor, "text": e.text, "unix_time": e.at.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)})).collect::<Vec<_>>()}))
        }
        other => fail("unknown_tool", format!("No tool named {other}.")),
    }
}

/// The control socket, bound for the lifetime of this value.
pub struct AutomationServer {
    _server: tracker_ipc::socket::Server,
}

impl AutomationServer {
    pub fn start(app: &App, path: Option<PathBuf>) -> Result<Self> {
        let app = app.clone();
        let server = tracker_ipc::socket::Server::start(path.unwrap_or_else(tracker_ipc::socket::default_path), move |req| handle_request(&app, req))?;
        Ok(Self { _server: server })
    }
}
