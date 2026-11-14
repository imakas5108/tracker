//! A full agent-style analysis through the same request handler the MCP bridge uses.
//! Needs the Java Tracker sample clip ~/Tracker/videos/BallTossOut.mp4; skips without it.

use serde_json::{Value, json};
use tracker_app::{App, AppConfig, handle_request};
use tracker_ipc::Request;
use uuid::Uuid;

struct Agent {
    app: App,
    id: Uuid,
}

impl Agent {
    fn call(&self, command: &str, mut args: Value) -> Result<Value, tracker_ipc::ErrorBody> {
        let tool = tracker_ipc::tool(command).expect("known tool");
        if !tool.read_only && args.get("request_id").is_none() {
            args["request_id"] = json!(Uuid::new_v4().to_string());
        }
        let req = Request { version: 1, client_id: self.id, client_name: Some("test-agent".into()), command: command.into(), arguments: args.as_object().unwrap().clone() };
        handle_request(&self.app, req).into_result()
    }
    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args).unwrap_or_else(|e| panic!("{command}: {e}"))
    }
    fn rev(&self, sid: &str) -> u64 {
        self.ok("tracker_get_session", json!({"session_id": sid}))["revision"].as_u64().unwrap()
    }
}

/// Java Tracker's stored hand marks in BallToss.trk (frame N is at N/30 s).
const JAVA: [(u32, f64, f64); 11] = [(10, 131.277, 207.858), (12, 176.100, 154.661), (14, 221.648, 118.624), (16, 267.195, 100.605), (18, 312.742, 99.103), (20, 358.290, 116.121), (22, 404.838, 150.156), (24, 450.886, 202.711), (26, 497.935, 272.784), (28, 545.485, 361.877), (30, 594.536, 469.990)];

#[test]
fn agent_tracks_balltoss_and_builds_a_dataset() {
    let video = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("Tracker/videos/BallTossOut.mp4");
    if !video.exists() {
        eprintln!("SKIPPED: {} not found", video.display());
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let app = App::new(AppConfig { datasets_root: tmp.path().join("datasets"), tracking_slots: 2, agent_control: true });
    let agent = Agent { app: app.clone(), id: Uuid::new_v4() };

    // Import and wait for indexing.
    let imp = agent.ok("tracker_import_video", json!({"path": video}));
    let sid = imp["session_id"].as_str().unwrap().to_string();
    let job = agent.ok("tracker_get_job", json!({"job_id": imp["job_id"], "wait_seconds": 20}));
    assert_eq!(job["state"], "completed", "{job}");
    let s = agent.ok("tracker_get_session", json!({"session_id": sid}));
    assert_eq!(s["width"], 640);
    assert!((s["first_frame_time"].as_f64().unwrap() - 2.0 / 30.0).abs() < 1e-6);

    // Inspect a frame (crop around the ball) without moving the viewer.
    let viewer_before = s["viewer_time"].as_f64().unwrap();
    let f = agent.ok("tracker_get_frame", json!({"session_id": sid, "time": 10.0 / 30.0, "crop": {"x": 100, "y": 170, "width": 64, "height": 64}, "overlays": true}));
    assert!(f["image_base64"].as_str().unwrap().len() > 100);
    assert_eq!(agent.ok("tracker_get_session", json!({"session_id": sid}))["viewer_time"].as_f64().unwrap(), viewer_before);

    let (track, _) = {
        let r = agent.ok("tracker_add_track", json!({"session_id": sid, "expected_revision": agent.rev(&sid), "name": "ball"}));
        (r["track_id"].as_str().unwrap().to_string(), r["revision"].as_u64().unwrap())
    };
    // Stale revisions are refused.
    let stale = agent.call("tracker_add_track", json!({"session_id": sid, "expected_revision": 0}));
    assert_eq!(stale.unwrap_err().code, "stale_revision");

    let (f10, x10, y10) = JAVA[0];
    let m = agent.ok("tracker_mark_point", json!({"session_id": sid, "expected_revision": agent.rev(&sid), "track_id": track, "time": f10 as f64 / 30.0, "x": x10, "y": y10}));
    assert!((m["time"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-6, "marked on the exact frame without seeking");
    // The same scale reference and axes Java Tracker stored in BallToss.trk: a 1.0 m tape
    // along the meter stick, origin at the first ball position, axes rotated 0.044 rad.
    agent.ok("tracker_set_calibration", json!({"session_id": sid, "expected_revision": agent.rev(&sid), "x1": 272.20046937463627, "y1": 446.11086870171016, "x2": 273.2009904793783, "y2": 47.40320846200204, "length": 1.0, "unit": "m"}));

    // Retrying a mutation with the same request_id returns the same result.
    let rid = Uuid::new_v4().to_string();
    let angle = 0.04401895816986783f64.to_degrees();
    let axes = json!({"request_id": rid, "session_id": sid, "expected_revision": agent.rev(&sid), "origin_x": 131.2767066180302, "origin_y": 207.85825951016153, "angle_deg": angle});
    let a1 = agent.ok("tracker_set_axes", axes.clone());
    let a2 = agent.ok("tracker_set_axes", axes.clone());
    assert_eq!(a1, a2, "same request_id returns the original result");
    let mut changed = axes.clone();
    changed["origin_x"] = json!(0);
    assert_eq!(agent.call("tracker_set_axes", changed).unwrap_err().code, "request_id_conflict");

    // Track with the default v2 settings; resolve any uncertain frames by accepting the
    // candidate only when it is within 3 px of Java's mark, otherwise correct it.
    let mut job = agent.ok("tracker_start_tracking", json!({"session_id": sid, "expected_revision": agent.rev(&sid), "track_id": track, "from_time": f10 as f64 / 30.0}));
    let mut reviews = 0;
    loop {
        job = agent.ok("tracker_get_job", json!({"job_id": job["job_id"], "wait_seconds": 30}));
        match job["state"].as_str().unwrap() {
            "completed" => break,
            "awaiting_review" => {
                reviews += 1;
                assert!(reviews < 10, "too many reviews");
                let r = agent.ok("tracker_get_review", json!({"session_id": sid}));
                assert_eq!(r["recorded"], false);
                let frame = (r["time"].as_f64().unwrap() * 30.0).round() as u32;
                eprintln!("review at frame {frame}: {} score {:.3} sep {:.3}", r["explanation"], r["score"].as_f64().unwrap(), r["separation"].as_f64().unwrap());
                let next = agent.ok("tracker_review_match", json!({"session_id": sid, "job_id": job["job_id"], "action": "skip"}));
                job = next["job"].clone();
            }
            other => panic!("unexpected job state {other}: {job}"),
        }
    }

    // Compare with Java's hand marks.
    let rows = agent.ok("tracker_get_measurements", json!({"session_id": sid, "track_id": track, "limit": 1000}));
    let ms = rows["measurements"].as_array().unwrap();
    let mut errs = vec![];
    for (frame, jx, jy) in JAVA.iter().skip(1) {
        if let Some(r) = ms.iter().find(|r| (r["time"].as_f64().unwrap() * 30.0).round() as u32 == *frame) {
            let d = ((r["pixel_x"].as_f64().unwrap() - jx).powi(2) + (r["pixel_y"].as_f64().unwrap() - jy).powi(2)).sqrt();
            errs.push((*frame, d));
        }
    }
    eprintln!("tracked {} points, {} reviews; vs Java marks: {:?}", ms.len(), reviews, errs);
    assert!(errs.len() >= 8, "tracked most of the Java-marked frames: {errs:?}");
    assert!(errs.iter().all(|(_, d)| *d < 3.0), "within 3 px of Java hand marks: {errs:?}");
    let fit = &rows["fit"];
    eprintln!("fit: {fit}");
    let (ax, ay) = (fit["ax"].as_f64().unwrap(), fit["ay"].as_f64().unwrap());
    let g = ax.hypot(ay);
    eprintln!("fitted acceleration: ax {ax:.3}, ay {ay:.3}, |a| {g:.3} m/s²");
    assert!(ay < 0.0 && (9.0..10.6).contains(&g), "free-fall acceleration near 9.8 m/s², got {g}");

    // Store in a dataset and export for Google Sheets.
    agent.ok("tracker_dataset_create", json!({"name": "Ball toss", "description": "integration test"}));
    let added = agent.ok("tracker_dataset_add_run", json!({"dataset": "Ball toss", "session_id": sid, "track_id": track, "conditions": {"trial": "1"}}));
    assert_eq!(added["runs"], 1);
    let values = agent.ok("tracker_dataset_rows", json!({"dataset": "Ball toss", "sheet": "observations", "limit": 5}));
    assert_eq!(values["values"][0][1], "t_s");
    assert_eq!(values["values"].as_array().unwrap().len(), 6);
    let xlsx = tmp.path().join("balltoss.xlsx");
    agent.ok("tracker_dataset_export", json!({"dataset": "Ball toss", "format": "xlsx", "path": xlsx}));
    assert!(xlsx.exists());
    let csv = tmp.path().join("obs.csv");
    agent.ok("tracker_dataset_export", json!({"dataset": "Ball toss", "format": "csv", "sheet": "observations", "path": csv}));
    assert_eq!(agent.call("tracker_dataset_export", json!({"dataset": "Ball toss", "format": "csv", "path": csv})).unwrap_err().code, "destination_exists");

    // Save, reopen, and compare.
    let proj = tmp.path().join("balltoss.tracker.json");
    agent.ok("tracker_save_project", json!({"session_id": sid, "path": proj}));
    agent.ok("tracker_close_session", json!({"session_id": sid}));
    let reopened = agent.ok("tracker_open_project", json!({"path": proj}));
    let job = agent.ok("tracker_get_job", json!({"job_id": reopened["job_id"], "wait_seconds": 20}));
    assert_eq!(job["state"], "completed", "{job}");
    let s2 = reopened["session_id"].as_str().unwrap();
    let rows2 = agent.ok("tracker_get_measurements", json!({"session_id": s2, "track_id": track, "limit": 1000}));
    assert_eq!(rows2["total"], rows["total"]);
    assert_eq!(rows2["unit"], "m");

    // Every action is in the activity log, attributed to the agent.
    let act = agent.ok("tracker_get_activity", json!({"limit": 200}));
    assert!(act["activity"].as_array().unwrap().iter().any(|e| e["actor"] == "test-agent" && e["text"].as_str().unwrap().starts_with("Started autotracking")));

    // Stopping control blocks agents.
    app.set_control_enabled(false);
    assert_eq!(agent.call("tracker_list_sessions", json!({})).unwrap_err().code, "control_disabled");
}
