//! `tracker-mcp`: an MCP server over stdio that operates the Tracker app.
//!
//! The bridge holds no analysis state. Each tool call is validated by the app against the
//! same catalog advertised here and runs through the app's shared command layer, so agent
//! actions appear live in the window, in its activity log, and in undo history.
//!
//! Usage: tracker-mcp [--app /path/to/tracker-or-tracker-headless] [--socket PATH]
//! If the app is not running, the bridge starts it with `--agent-control`.

use rmcp::model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, InitializeRequestParams, InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities, Tool, ToolAnnotations};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracker_ipc::{ErrorBody, Request};
use uuid::Uuid;

const INSTRUCTIONS: &str = "\
Tracker is a video analysis app for physics experiments. You operate the user's running app; every action is visible to them and undoable.

Full analysis workflow:
1. tracker_import_video(path) then tracker_get_job(job_id, wait_seconds=20) until completed. tracker_get_session gives size, frame count, first/last frame times and the revision.
2. Look before you mark: tracker_get_frame(time, crop, max_edge) returns a PNG and an exact mapping back to source pixels (source = image / scale + crop). Coordinates are source pixels, origin top-left, y down. Crop around the object to place marks precisely.
3. tracker_add_track, then tracker_mark_point(track_id, time, x, y) on the object's center in the first frame you want to track. Marking works on any frame; you do not need to seek.
4. Scale: tracker_set_calibration only with a length you can justify (a visible meter stick, a known object size, or the user's statement). Otherwise leave units in px and say so. tracker_set_axes places the origin (for example at the launch point).
5. tracker_start_tracking(track_id, from_time) and poll tracker_get_job(wait_seconds=30). On awaiting_review: read tracker_get_review, view the frame (crop around candidate and predicted positions), then tracker_review_match with accept (candidate is right), correct (give x, y), skip (object hidden), or stop. Never accept a candidate you have not checked. Follow the returned continuation job.
6. tracker_get_measurements returns positions, velocities, accelerations and a quadratic fit. Report what was measured, not what was expected.
7. Datasets: tracker_dataset_create once, then tracker_dataset_add_run per analysed track with conditions (experimental variables as text). tracker_dataset_rows returns header + rows in the Google Sheets API values format for a Sheets connector; tracker_dataset_export writes .xlsx (opens directly in Google Sheets) or .csv.
8. tracker_save_project to keep the analysis.

Every mutation needs a fresh UUID request_id; session edits also need expected_revision from your latest tracker_get_session (stale values are refused, re-read and retry with a new request_id). Match scores are similarities, never probabilities. If a call returns control_disabled, ask the user to choose Allow Agent Control in Tracker.";

struct Bridge {
    socket: PathBuf,
    app: Option<PathBuf>,
    client_id: Uuid,
    client_name: Arc<Mutex<Option<String>>>,
}

impl Bridge {
    fn request(&self, command: &str, arguments: Map<String, Value>) -> Request {
        Request { version: tracker_ipc::PROTOCOL_VERSION, client_id: self.client_id, client_name: self.client_name.lock().unwrap().clone(), command: command.into(), arguments }
    }

    /// Calls the app, starting it once if no app is listening.
    fn call_blocking(&self, command: &str, arguments: Map<String, Value>) -> Result<Value, ErrorBody> {
        let req = self.request(command, arguments);
        match tracker_ipc::socket::call(&self.socket, &req, Duration::from_secs(60)) {
            Err(e) if e.code == "connection_error" => {
                self.launch()?;
                tracker_ipc::socket::call(&self.socket, &req, Duration::from_secs(60))
            }
            other => other,
        }
    }

    fn launch(&self) -> Result<(), ErrorBody> {
        let exe = match &self.app {
            Some(p) => p.clone(),
            None => {
                let me = std::env::current_exe().map_err(|e| ErrorBody::new("app_not_found", e.to_string()))?;
                let dir = me.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                ["tracker", "Tracker", "tracker-headless"].iter().map(|n| dir.join(n)).find(|p| p.exists()).ok_or_else(|| ErrorBody::new("app_not_found", "Tracker is not running and no app was found next to tracker-mcp. Pass --app /path/to/tracker."))?
            }
        };
        eprintln!("tracker-mcp: starting {}", exe.display());
        std::process::Command::new(&exe)
            .arg("--agent-control")
            .args(std::env::var_os("TRACKER_CONTROL_SOCKET").map(|s| vec!["--socket".into(), s]).unwrap_or_default())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| ErrorBody::new("app_launch_failed", format!("Could not start {}: {e}", exe.display())))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(150));
            match tracker_ipc::socket::call(&self.socket, &self.request("__hello", Map::new()), Duration::from_secs(5)) {
                Ok(_) => return Ok(()),
                Err(e) if e.code == "connection_error" => continue,
                Err(e) => return Err(e),
            }
        }
        Err(ErrorBody::new("startup_timeout", "Tracker did not open its control socket within 20 seconds."))
    }

    async fn call(self: &Arc<Self>, command: String, arguments: Map<String, Value>) -> Result<Value, ErrorBody> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || me.call_blocking(&command, arguments)).await.unwrap_or_else(|e| Err(ErrorBody::new("bridge_error", e.to_string())))
    }
}

#[derive(Clone)]
struct Server(Arc<Bridge>);

impl ServerHandler for Server {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tracker", env!("CARGO_PKG_VERSION")).with_title("Tracker video analysis"))
            .with_instructions(INSTRUCTIONS)
    }

    async fn initialize(&self, request: InitializeRequestParams, context: RequestContext<RoleServer>) -> Result<InitializeResult, McpError> {
        *self.0.client_name.lock().unwrap() = Some(request.client_info.name.clone());
        context.peer.set_peer_info(request.clone());
        // Connect (and start the app if needed) now, so the first tool call is fast. Failures
        // are reported on tool calls instead of failing the handshake.
        if let Err(e) = self.0.call("__hello".into(), Map::new()).await {
            eprintln!("tracker-mcp: {e}");
        }
        self.negotiate_initialize(&request)
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let tools = tracker_ipc::catalog()
            .iter()
            .map(|t| {
                let schema = Arc::new(t.schema.as_object().cloned().unwrap_or_default());
                Tool::new(t.name, t.description, schema).with_title(t.title).with_annotations(ToolAnnotations::new().read_only(t.read_only).destructive(t.destructive).idempotent(t.idempotent).open_world(false))
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(&self, request: CallToolRequestParams, _context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let args = request.arguments.clone().unwrap_or_default();
        Ok(match self.0.call(request.name.to_string(), args).await {
            Ok(mut value) => {
                let mut content = vec![];
                let image = value.as_object_mut().and_then(|o| o.remove("image_base64")).and_then(|v| v.as_str().map(str::to_string));
                content.push(ContentBlock::text(serde_json::to_string(&value).unwrap_or_default()));
                if let Some(data) = image {
                    content.push(ContentBlock::image(data, "image/png"));
                }
                let mut r = CallToolResult::success(content);
                r.structured_content = Some(value);
                r.into()
            }
            Err(e) => {
                let v = json!({"code": e.code, "message": e.message});
                let mut r = CallToolResult::error(vec![ContentBlock::text(v.to_string())]);
                r.structured_content = Some(v);
                r.into()
            }
        })
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (mut app, mut socket) = (None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--app" => app = args.next().map(PathBuf::from),
            "--socket" => socket = args.next().map(PathBuf::from),
            "--version" => {
                println!("tracker-mcp {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("tracker-mcp: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    if let Some(s) = &socket {
        // The app inherits this when the bridge launches it.
        unsafe { std::env::set_var("TRACKER_CONTROL_SOCKET", s) };
    }
    let bridge = Arc::new(Bridge { socket: socket.unwrap_or_else(tracker_ipc::socket::default_path), app, client_id: Uuid::new_v4(), client_name: Arc::new(Mutex::new(None)) });
    let heartbeat = bridge.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let _ = heartbeat.call("__heartbeat".into(), Map::new()).await;
        }
    });
    let service = match Server(bridge.clone()).serve(rmcp::transport::stdio()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tracker-mcp: {e}");
            std::process::exit(1);
        }
    };
    let _ = service.waiting().await;
    let _ = bridge.call("__disconnect".into(), Map::new()).await;
}
