use crate::protocol::ErrorBody;
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

/// One tool as advertised over MCP and validated by the app.
#[derive(Clone, Debug)]
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub read_only: bool,
    /// Repeating the call with the same arguments has no further effect.
    pub idempotent: bool,
    /// May delete data or overwrite files.
    pub destructive: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Read,
    /// Mutation that needs `request_id` only.
    Write,
    /// Mutation of a session's analysis: needs `request_id`, `session_id` and `expected_revision`.
    Edit,
}

struct Builder(Vec<Tool>);

impl Builder {
    fn add(&mut self, name: &'static str, title: &'static str, description: &'static str, kind: Kind, props: Value, required: &[&str], idempotent: bool, destructive: bool) {
        let mut props: Map<String, Value> = props.as_object().cloned().unwrap_or_default();
        let mut req: Vec<Value> = required.iter().map(|r| json!(r)).collect();
        if kind != Kind::Read {
            props.insert("request_id".into(), json!({"type": "string", "description": "A new UUID for each mutation. Retrying with the same UUID and arguments returns the original result."}));
            req.push(json!("request_id"));
        }
        if kind == Kind::Edit {
            props.insert("session_id".into(), json!({"type": "string"}));
            props.insert("expected_revision".into(), json!({"type": "integer", "minimum": 0, "description": "The session revision from your latest tracker_get_session. Stale revisions are rejected."}));
            req.push(json!("session_id"));
            req.push(json!("expected_revision"));
        }
        let schema = json!({"type": "object", "properties": props, "required": req, "additionalProperties": false});
        self.0.push(Tool { name, title, description, schema, read_only: kind == Kind::Read, idempotent: kind == Kind::Read || idempotent, destructive });
    }
}

fn s() -> Value {
    json!({"type": "string"})
}
fn n() -> Value {
    json!({"type": "number"})
}
fn i() -> Value {
    json!({"type": "integer", "minimum": 0})
}
fn b() -> Value {
    json!({"type": "boolean"})
}

/// The full tool list, in the order an agent typically uses it.
pub fn catalog() -> &'static [Tool] {
    static CATALOG: OnceLock<Vec<Tool>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        use Kind::*;
        let mut c = Builder(vec![]);
        let sid = json!({"session_id": s()});
        c.add("tracker_list_sessions", "List sessions", "List open video sessions with IDs, revisions, sizes, frame counts, tracks and job state.", Read, json!({}), &[], true, false);
        c.add("tracker_import_video", "Import video", "Open a local video by absolute path. Indexing runs in the background: poll tracker_get_job with the returned job_id (or pass wait_seconds) until state is completed.", Write, json!({"path": s()}), &["path"], false, false);
        c.add("tracker_open_project", "Open project", "Open a saved .tracker.json project (Rust v2 or Swift v1). Missing footage returns relink_required; pass relink_path only with the original video.", Write, json!({"path": s(), "relink_path": s()}), &["path"], false, false);
        c.add("tracker_get_session", "Inspect session", "Source path and size, duration, frame count, first and last frame times, nominal fps, coordinate system, calibration, clip, tracking settings, tracks with point counts, viewer time, revision, dirty flag and active job.", Read, sid.clone(), &["session_id"], true, false);
        c.add("tracker_select_session", "Select session", "Bring this session's tab to the front in the app. Agents normally do not need this; every tool takes an explicit session_id.", Write, sid.clone(), &["session_id"], true, false);
        c.add("tracker_seek", "Move viewer", "Move the app's visible frame to the frame displayed at `time` seconds. Optional: marking and frame inspection do not depend on the viewer.", Write, json!({"session_id": s(), "time": n()}), &["session_id", "time"], true, false);
        c.add("tracker_close_session", "Close session", "Close a session. Unsaved changes are refused unless discard is true.", Write, json!({"session_id": s(), "discard": b()}), &["session_id"], false, true);
        c.add("tracker_get_frame", "Get frame image", "PNG of the frame displayed at `time` (default: the viewer frame) plus its exact source timestamp. Coordinates are orientation-corrected source pixels, origin top-left, y down. Optional crop (source pixels) is applied before resizing to max_edge (default 1280, max 1600). Convert back with source = image / scale + crop. overlays draws marks, axes and calibration.", Read, json!({"session_id": s(), "time": n(), "crop": {"type": "object", "properties": {"x": i(), "y": i(), "width": i(), "height": i()}, "required": ["x", "y", "width", "height"], "additionalProperties": false}, "max_edge": i(), "overlays": b()}), &["session_id"], true, false);
        c.add("tracker_add_track", "Add track", "Add a point-mass track. Returns its track_id.", Edit, json!({"name": s()}), &[], false, false);
        c.add("tracker_rename_track", "Rename track", "Rename a track.", Edit, json!({"track_id": s(), "name": s()}), &["track_id", "name"], true, false);
        c.add("tracker_delete_track", "Delete track", "Delete a track and its points (undoable).", Edit, json!({"track_id": s()}), &["track_id"], false, true);
        c.add("tracker_mark_point", "Mark point", "Record or correct an observed position on the frame displayed at `time`. x and y are source pixels (top-left origin). Works on any frame; the viewer is not moved. Returns the exact frame used.", Edit, json!({"track_id": s(), "time": n(), "x": n(), "y": n()}), &["track_id", "time", "x", "y"], false, false);
        c.add("tracker_remove_point", "Remove point", "Remove a track's point on the frame displayed at `time`.", Edit, json!({"track_id": s(), "time": n()}), &["track_id", "time"], false, true);
        c.add("tracker_set_calibration", "Set scale", "Set the physical scale from two source-pixel endpoints a known distance apart. Only use a length you can justify from the footage or the user (for example a visible meter stick). Units: m, cm, mm.", Edit, json!({"x1": n(), "y1": n(), "x2": n(), "y2": n(), "length": n(), "unit": {"type": "string", "enum": ["m", "cm", "mm"]}}), &["x1", "y1", "x2", "y2", "length", "unit"], true, false);
        c.add("tracker_set_axes", "Set axes", "Place the coordinate origin at a source pixel and rotate the x axis by angle_deg (counterclockwise on screen). Analysis y points up. Positions recompute; no re-tracking is needed.", Edit, json!({"origin_x": n(), "origin_y": n(), "angle_deg": n()}), &["origin_x", "origin_y"], true, false);
        c.add("tracker_set_clip", "Set clip range", "Limit tracking and exports to start_time..end_time and use every step-th frame. Omit a bound to clear it.", Edit, json!({"start_time": n(), "end_time": n(), "step": i()}), &[], true, false);
        c.add("tracker_configure_tracking", "Configure autotracker", "Update autotracker settings; omitted fields keep their values. algorithm: masked_ncc (default; normalized correlation, robust to lighting) or ssd_v1 (Swift baseline). review_policy: pause (stop at uncertain frames) or skip_uncertain (leave up to max_gap gaps). Scores are similarities, not probabilities.", Edit, json!({"algorithm": {"type": "string", "enum": ["masked_ncc", "ssd_v1"]}, "template_radius": i(), "search_radius": i(), "minimum_score": n(), "minimum_separation": n(), "predict": b(), "evolve_rate": n(), "tether": n(), "mask": {"type": "string", "enum": ["circle", "square"]}, "review_policy": {"type": "string", "enum": ["pause", "skip_uncertain"]}, "max_gap": i()}), &[], true, false);
        c.add("tracker_start_tracking", "Start autotracking", "Track forward from this track's point on the frame at from_time to the clip end. Returns a job_id; poll tracker_get_job (wait_seconds long-polls). If the job reaches awaiting_review, inspect tracker_get_review and the frame image, then call tracker_review_match.", Edit, json!({"track_id": s(), "from_time": n()}), &["track_id", "from_time"], false, false);
        c.add("tracker_get_job", "Inspect job", "State of an import or tracking job: queued, running, awaiting_review, completed, failed or cancelled, with progress and errors. wait_seconds (up to 30) blocks until the job finishes or pauses for review.", Read, json!({"job_id": s(), "wait_seconds": n()}), &["job_id"], true, false);
        c.add("tracker_list_jobs", "List jobs", "Recent jobs, optionally for one session.", Read, json!({"session_id": s()}), &[], true, false);
        c.add("tracker_get_review", "Inspect uncertain match", "The pending uncertain frame for a session: job and track IDs, frame time, candidate and predicted positions, score, separation and reason. The candidate is not recorded until accepted. Fetch the image with tracker_get_frame.", Read, sid.clone(), &["session_id"], true, false);
        c.add("tracker_review_match", "Resolve uncertain match", "Resolve a paused job: accept records the candidate as reviewed; correct records your x, y instead; skip records nothing; stop ends tracking. accept, correct and skip continue tracking and return the new job_id.", Write, json!({"session_id": s(), "job_id": s(), "action": {"type": "string", "enum": ["accept", "correct", "skip", "stop"]}, "x": n(), "y": n()}), &["session_id", "job_id", "action"], false, false);
        c.add("tracker_cancel_job", "Cancel job", "Cancel a queued, running or paused job. Results already delivered stay (and can be undone).", Write, json!({"job_id": s()}), &["job_id"], true, false);
        c.add("tracker_undo", "Undo", "Undo the latest analysis edit or tracking run in this session.", Edit, json!({}), &[], false, false);
        c.add("tracker_redo", "Redo", "Redo the latest undone edit in this session.", Edit, json!({}), &[], false, false);
        c.add("tracker_get_measurements", "Read measurements", "Paginated points of a track with exact frame stamps, source pixels, calibrated x/y (y up), and when derived is true (default) velocity, speed and acceleration plus a quadratic fit summary. Pass expected_revision across pages to detect changes.", Read, json!({"session_id": s(), "track_id": s(), "offset": i(), "limit": i(), "derived": b(), "expected_revision": i()}), &["session_id", "track_id"], true, false);
        c.add("tracker_save_project", "Save project", "Save the session to a .tracker.json file at an absolute path. Existing files need overwrite=true.", Write, json!({"session_id": s(), "path": s(), "overwrite": b()}), &["session_id", "path"], false, true);
        c.add("tracker_export_csv", "Export CSV", "Write a track's measurements and derived motion to a CSV file at an absolute path. Existing files need overwrite=true.", Write, json!({"session_id": s(), "track_id": s(), "path": s(), "overwrite": b()}), &["session_id", "track_id", "path"], false, true);
        c.add("tracker_dataset_list", "List datasets", "Datasets in the datasets folder, with run counts.", Read, json!({}), &[], true, false);
        c.add("tracker_dataset_create", "Create dataset", "Create a named dataset for collecting runs across videos.", Write, json!({"name": s(), "description": s()}), &["name"], false, false);
        c.add("tracker_dataset_add_run", "Store run in dataset", "Store a track's measurements, derived motion, calibration, settings and fit summary as one run. conditions holds experimental variables as text, e.g. {\"mass_kg\": \"0.05\", \"angle_deg\": \"30\"}.", Write, json!({"dataset": s(), "session_id": s(), "track_id": s(), "conditions": {"type": "object", "additionalProperties": {"type": "string"}}, "notes": s()}), &["dataset", "session_id", "track_id"], false, false);
        c.add("tracker_dataset_get", "Inspect dataset", "A dataset's description, folder, and run summaries (video, track, unit, points by origin, fitted accelerations, conditions).", Read, json!({"dataset": s()}), &["dataset"], true, false);
        c.add("tracker_dataset_rows", "Read dataset rows", "Rows of the runs or observations sheet as arrays: the header row first, then data. This matches the Google Sheets API values format, so a Sheets connector can write them directly. Paginate with offset and limit (max 5000).", Read, json!({"dataset": s(), "sheet": {"type": "string", "enum": ["runs", "observations"]}, "offset": i(), "limit": i()}), &["dataset", "sheet"], true, false);
        c.add("tracker_dataset_remove_run", "Remove run from dataset", "Remove one run and its observations from a dataset.", Write, json!({"dataset": s(), "run_id": s()}), &["dataset", "run_id"], false, true);
        c.add("tracker_dataset_export", "Export dataset", "Export to an absolute path: xlsx writes one workbook (runs, observations, about sheets) that Google Sheets opens directly; csv writes the chosen sheet. Existing files need overwrite=true.", Write, json!({"dataset": s(), "format": {"type": "string", "enum": ["xlsx", "csv"]}, "sheet": {"type": "string", "enum": ["runs", "observations"]}, "path": s(), "overwrite": b()}), &["dataset", "format", "path"], false, true);
        c.add("tracker_get_activity", "Agent activity", "The app's recent activity log (agent and user actions).", Read, json!({"limit": i()}), &[], true, false);
        c.0
    })
}

pub fn tool(name: &str) -> Option<&'static Tool> {
    catalog().iter().find(|t| t.name == name)
}

/// Checks argument names, required fields and JSON types against the tool's schema.
pub fn validate(name: &str, args: &Map<String, Value>) -> Result<&'static Tool, ErrorBody> {
    let t = tool(name).ok_or_else(|| ErrorBody::new("unknown_tool", format!("No tool named {name}.")))?;
    let props = t.schema["properties"].as_object().expect("schema properties");
    for key in args.keys() {
        if !props.contains_key(key) {
            return Err(ErrorBody::new("invalid_argument", format!("Unknown argument {key}.")));
        }
    }
    for r in t.schema["required"].as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
        if args.get(r).is_none_or(|v| v.is_null()) {
            return Err(ErrorBody::new("invalid_argument", format!("Missing {r}.")));
        }
    }
    for (key, value) in args {
        let spec = &props[key];
        let ok = match spec["type"].as_str() {
            Some("string") => value.as_str().is_some_and(|s| spec["enum"].as_array().is_none_or(|e| e.iter().any(|x| x == s))),
            Some("number") => value.as_f64().is_some_and(f64::is_finite),
            Some("integer") => value.as_u64().is_some(),
            Some("boolean") => value.is_boolean(),
            Some("object") => value.is_object(),
            _ => false,
        };
        if !ok {
            let hint = spec["enum"].as_array().map(|e| format!(" (one of {})", e.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))).unwrap_or_default();
            return Err(ErrorBody::new("invalid_argument", format!("Invalid type or value for {key}{hint}.")));
        }
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let names: std::collections::HashSet<_> = catalog().iter().map(|t| t.name).collect();
        assert_eq!(names.len(), catalog().len(), "unique names");
        for t in catalog() {
            assert!(t.name.starts_with("tracker_"));
            let req = t.schema["required"].as_array().unwrap();
            for r in req {
                assert!(t.schema["properties"].get(r.as_str().unwrap()).is_some(), "{} requires undeclared {}", t.name, r);
            }
            if !t.read_only {
                assert!(req.contains(&json!("request_id")), "{} needs request_id", t.name);
            }
        }
        assert!(!tool("tracker_start_tracking").unwrap().idempotent, "mutations are not blanket idempotent");
        assert!(tool("tracker_delete_track").unwrap().destructive);
    }

    #[test]
    fn validation() {
        let args = |v: Value| v.as_object().unwrap().clone();
        assert!(validate("tracker_get_session", &args(json!({"session_id": "x"}))).is_ok());
        assert_eq!(validate("tracker_get_session", &args(json!({}))).unwrap_err().code, "invalid_argument");
        assert!(validate("tracker_get_session", &args(json!({"session_id": "x", "extra": 1}))).is_err());
        assert!(validate("tracker_set_calibration", &args(json!({"request_id":"r","session_id":"s","expected_revision":1,"x1":0,"y1":0,"x2":1,"y2":0,"length":1,"unit":"ft"}))).is_err());
        assert!(validate("tracker_add_track", &args(json!({"request_id":"r","session_id":"s","expected_revision":-1}))).is_err());
        assert!(validate("nope", &args(json!({}))).is_err());
    }
}
