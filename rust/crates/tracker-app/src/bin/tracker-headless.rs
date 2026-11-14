//! Tracker without a window: serves the agent control socket for automation and CI.
//!
//! Usage: tracker-headless [--socket PATH] [--datasets DIR]
//! Agent control is enabled at start. Stop it with Ctrl-C.

fn main() {
    let mut args = std::env::args().skip(1);
    let mut config = tracker_app::AppConfig { agent_control: true, ..Default::default() };
    let mut socket = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = args.next().map(std::path::PathBuf::from),
            "--datasets" => config.datasets_root = args.next().map(std::path::PathBuf::from).expect("--datasets needs a folder"),
            "--agent-control" => {}
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let app = tracker_app::App::new(config);
    let _server = match tracker_app::AutomationServer::start(&app, socket) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tracker-headless: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("tracker-headless: agent control socket ready");
    loop {
        std::thread::park();
    }
}
