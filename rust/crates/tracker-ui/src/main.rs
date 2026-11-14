//! Tracker: native window.
//!
//! Usage: tracker [--agent-control] [--socket PATH] [VIDEO_OR_PROJECT ...]

mod plot;
mod style;
mod widgets;
mod window;

use gpui_kit::*;
use tracker_app::{App as Core, AppConfig, AutomationServer};

actions!(
    tracker,
    [
        OpenVideo, OpenProject, SaveProject, SaveProjectAs, ExportCsv, CloseVideo, Undo, Redo, PlayPause, NextFrame, PrevFrame, FirstFrame, NewPointMass, DeleteTrack, RenameTrack, ClipSettings,
        Calibrate, MoveAxes, ClearScale, StartAutotrack, AddToDataset, ToggleAgentControl, ShowActivity, Quit, NextField, PrevField
    ]
);

fn menus() -> Vec<Menu> {
    vec![
        Menu { name: "Tracker".into(), items: vec![MenuItem::action("Quit Tracker", Quit)], disabled: false },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open Video…", OpenVideo),
                MenuItem::action("Open Project…", OpenProject),
                MenuItem::separator(),
                MenuItem::action("Save Project", SaveProject),
                MenuItem::action("Save Project As…", SaveProjectAs),
                MenuItem::action("Export Track CSV…", ExportCsv),
                MenuItem::action("Add Track to Dataset…", AddToDataset),
                MenuItem::separator(),
                MenuItem::action("Close Video", CloseVideo),
            ],
            disabled: false,
        },
        Menu { name: "Edit".into(), items: vec![MenuItem::action("Undo", Undo), MenuItem::action("Redo", Redo)], disabled: false },
        Menu {
            name: "Video".into(),
            items: vec![MenuItem::action("Play / Pause", PlayPause), MenuItem::action("Next Frame", NextFrame), MenuItem::action("Previous Frame", PrevFrame), MenuItem::action("Go to Start", FirstFrame), MenuItem::separator(), MenuItem::action("Clip Settings…", ClipSettings)],
            disabled: false,
        },
        Menu {
            name: "Tracks".into(),
            items: vec![MenuItem::action("New Point Mass", NewPointMass), MenuItem::action("Rename Track…", RenameTrack), MenuItem::action("Delete Track", DeleteTrack), MenuItem::separator(), MenuItem::action("Autotrack From This Frame", StartAutotrack)],
            disabled: false,
        },
        Menu { name: "Coords".into(), items: vec![MenuItem::action("Calibration Stick…", Calibrate), MenuItem::action("Move Axes Origin", MoveAxes), MenuItem::action("Clear Scale", ClearScale)], disabled: false },
        Menu { name: "Agents".into(), items: vec![MenuItem::action("Allow or Stop Agent Control", ToggleAgentControl), MenuItem::action("Show Agent Activity", ShowActivity)], disabled: false },
    ]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut config = AppConfig::default();
    let mut socket = None;
    let mut files = vec![];
    while let Some(a) = args.next() {
        match a.as_str() {
            "--agent-control" => config.agent_control = true,
            "--socket" => socket = args.next().map(std::path::PathBuf::from),
            other if other.starts_with('-') => eprintln!("tracker: ignoring unknown option {other}"),
            other => files.push(std::path::PathBuf::from(other)),
        }
    }
    let core = Core::new(config);
    let server = AutomationServer::start(&core, socket);
    let server_error = server.as_ref().err().map(|e| e.message.clone());
    for f in files {
        let f = std::fs::canonicalize(&f).unwrap_or(f);
        let _ = if f.extension().is_some_and(|e| e == "json") { core.open_project(&f, None) } else { core.import_video(&f) };
    }

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(menus());
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenVideo, None),
            KeyBinding::new("cmd-shift-o", OpenProject, None),
            KeyBinding::new("cmd-s", SaveProject, None),
            KeyBinding::new("cmd-shift-s", SaveProjectAs, None),
            KeyBinding::new("cmd-w", CloseVideo, None),
            KeyBinding::new("cmd-z", Undo, None),
            KeyBinding::new("cmd-shift-z", Redo, None),
            KeyBinding::new("cmd-q", Quit, None),
            // Deeper than gpui-kit Root's Tab binding, so dialogs can move between fields.
            KeyBinding::new("tab", NextField, Some("Tracker")),
            KeyBinding::new("shift-tab", PrevField, Some("Tracker")),
        ]);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1320.), px(840.)), cx))),
            titlebar: Some(TitlebarOptions { title: Some("Tracker".into()), ..Default::default() }),
            window_min_size: Some(size(px(980.), px(640.))),
            ..Default::default()
        };
        let core = core.clone();
        let server_error = server_error.clone();
        gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| window::TrackerWindow::new(core, server_error, window, cx))).expect("open window");
        cx.activate(true);
    });
    drop(server);
}
