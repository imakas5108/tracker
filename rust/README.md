# Tracker in Rust

The Rust implementation of Tracker: video analysis for physics experiments, with a
Java Tracker style workspace and an MCP server that lets AI agents run a complete
analysis in the app you are looking at. macOS (Apple Silicon) first; platform code sits
behind traits so Windows and Linux can follow.

This is an independent port maintained separately from the Swift application in
`../swift/`. It does not replace or freeze Swift development. The port was
introduced by TkmanDevs in commit `a4e5137`; the shared MIT license is `../LICENSE`.
Swift comparisons must identify the exact revision and fixtures used.

## Crates

| Crate | What it owns |
| --- | --- |
| `tracker-core` | Exact rational frame stamps, tracks with provenance, coordinate systems (origin, rotation, calibration), clip range, bounded undo history, kinematics, project files (reads Swift v1, writes v2 with relative paths), atomic file output |
| `tracker-vision` | Matchers and the autotracker: `ssd_v1` (exact port of the Swift baseline) and `masked_ncc` (normalized cross-correlation, circular mask, motion prediction, tethered template evolution, review policy) |
| `tracker-media` | AVFoundation backend: frame index from sample timing (no decoding), exact frames, sequential luma read straight from the Y plane |
| `tracker-dataset` | Datasets of runs across videos; CSV, XLSX and Google Sheets `values` rows; quadratic fit summary |
| `tracker-ipc` | Local control protocol, the shared tool catalog with argument validation, and the private Unix socket |
| `tracker-app` | The single command layer for UI and agents: sessions, edits, jobs, multi-slot scheduler, review, datasets, activity log, and the control server. Also builds `tracker-headless` |
| `tracker-mcp` | The `tracker-mcp` stdio MCP server (official Rust SDK, `rmcp`) |
| `tracker-ui` | The `tracker` window, built on GPUI (`gpui-kit`) |

## Build and run

Requires Rust 1.90 or newer (tested with 1.100.0) and the Xcode Command Line Tools.
Full Xcode is not required.

```sh
cd rust
cargo build --release
./target/release/tracker            # the window
./target/release/tracker-headless   # no window; serves agents only
```

`tracker` accepts video or project paths as arguments, `--agent-control` to start with
agent control allowed, and `--socket PATH` to use another control socket.

## Tests

```sh
cargo test --release                 # unit and integration tests
python3 scripts/mcp-e2e.py           # an agent analyses BallTossOut.mp4 through tracker-mcp
```

Media and workflow tests use the Java Tracker sample clips in `~/Tracker/videos`
(`BallTossOut.mp4`). Without them they print `SKIPPED` and pass; that is not a
validation. Observed on an Apple M4 on 2026-11-14 with that clip and the calibration
stored in Java's `BallToss.trk`: 9 of Java's hand-marked frames compared, all within
1.07 px; fitted acceleration 10.03 m/s²; three uncertain frames paused for review (next
to the meter stick and at the image edge).

The window was also checked on 2026-11-15 by posting real mouse and key events to it
and confirming each effect through the control socket: 30 of 30 checks passed
(transport, slider, track selection, marking and Shift-click, Undo/Redo, calibration
clicks and dialog, axes, clip dialog with Tab, autotrack, jump to the uncertain frame,
Skip/Accept/Stop review, activity log, Stop/Allow agent control, Add to Dataset, and
closing with unsaved changes). The MCP acceptance script also passes against the
window (`TRACKER_APP=target/release/tracker python3 scripts/mcp-e2e.py`). The click
script is not part of the repository: it needs Screen Recording and Accessibility
permission and fixed window coordinates.

## Connecting agents

The bridge starts the app if it is not running (it looks for `tracker`, then
`tracker-headless`, next to itself, or use `--app PATH`).

Codex (`.codex/config.toml` in a trusted project, or `~/.codex/config.toml`):

```toml
[mcp_servers.tracker]
command = "/absolute/path/to/rust/target/release/tracker-mcp"
startup_timeout_sec = 30
tool_timeout_sec = 90
```

Claude Code:

```sh
claude mcp add tracker -- /absolute/path/to/rust/target/release/tracker-mcp
```

Agent control must be allowed in the window (agent bar at the bottom, or start the app
with `--agent-control`). Every agent action appears in the Agent Activity panel and in
undo history. Stop Agent Control blocks new requests immediately; running jobs stay
visible and can be stopped from the autotracker strip.

### Tools (36)

Sessions and frames: `tracker_list_sessions`, `tracker_import_video`,
`tracker_open_project`, `tracker_get_session`, `tracker_select_session`, `tracker_seek`,
`tracker_close_session`, `tracker_get_frame` (PNG plus exact source-pixel mapping).

Analysis: `tracker_add_track`, `tracker_rename_track`, `tracker_delete_track`,
`tracker_mark_point`, `tracker_remove_point`, `tracker_set_calibration`,
`tracker_set_axes`, `tracker_set_clip`, `tracker_configure_tracking`, `tracker_undo`,
`tracker_redo`.

Tracking: `tracker_start_tracking`, `tracker_get_job` (long-poll with `wait_seconds`),
`tracker_list_jobs`, `tracker_get_review`, `tracker_review_match` (accept, correct, skip,
stop), `tracker_cancel_job`.

Results: `tracker_get_measurements` (positions, velocities, accelerations, fit),
`tracker_save_project`, `tracker_export_csv`, `tracker_get_activity`.

Datasets: `tracker_dataset_list`, `tracker_dataset_create`, `tracker_dataset_add_run`,
`tracker_dataset_get`, `tracker_dataset_rows`, `tracker_dataset_remove_run`,
`tracker_dataset_export`.

Contract: every mutation takes a fresh UUID `request_id` (a retry with the same ID and
arguments returns the original result); session edits take `expected_revision`, which
changes only when the analysis changes (selecting a track, seeking or playing never
changes it). Coordinates for images and marks are orientation-corrected source pixels,
origin top-left.

## Datasets and Google Sheets

A dataset is a folder in `~/Documents/Tracker Datasets` (override with
`TRACKER_DATASETS_DIR`) holding `dataset.json`, `runs.csv` and `observations.csv`.
Each stored run records the video, track, calibration, axes, algorithm and settings,
point counts by provenance, experimental conditions you or the agent supply, and a
quadratic fit of x(t) and y(t). Observations hold every point with time, source
pixels, calibrated x and y, velocity, speed, acceleration, provenance and match score.

To get a dataset into Google Sheets:

- Export XLSX (`tracker_dataset_export` with `format: "xlsx"`, or the files in the
  folder) and open it in Google Sheets: the workbook has runs, observations and about
  sheets.
- Or import `runs.csv` and `observations.csv` with File > Import.
- Or let an agent with a Google Sheets connector call `tracker_dataset_rows`: it returns
  the header row and data rows in the Sheets API `values` format, paginated.

Tracker itself does not sign in to Google.

## Swift weaknesses addressed

| Review finding | Rust behavior |
| --- | --- |
| S1 selecting a track stopped tracking and changed the revision | Selection is view state outside undo history; edits only cancel a job that is writing to the edited track |
| S2 unnormalized SSD score | `masked_ncc` default: zero-mean normalized cross-correlation over a circular mask |
| S3 no motion prediction | Constant-acceleration prediction from the last three accepted points, a wider acquisition search on the first frame, and a bounded second search around the last position |
| S4 full-frame color conversion per frame | Luma copied directly from the decoder's Y plane |
| S5 one global tracking slot | Scheduler with N slots (half the cores by default), FIFO queue, cancellation |
| S6 agent and user shared one viewer | Marking and frame inspection take an explicit time; seeking never switches tabs |
| S7 unbounded request and job records | Bounded caches (2,000 requests, 500 jobs) |
| S8 every tool claimed idempotency | Hints come from the catalog: only read-only and truly idempotent tools say so |
| S9 no track rename or delete | Both, in the UI and over MCP, undoable |
| S10 absolute video paths | Projects store absolute and relative paths plus a content fingerprint |
| S11 machine-specific config | Config examples use placeholders; the bridge finds the app next to itself |
| S12 heavy, toolchain-dependent graph | The bridge uses `rmcp` with only server and stdio features; `Cargo.lock` is committed |
| S13 float time matching | Exact rational frame stamps with consistent equality, ordering and hashing |

## Not done yet

- Java parity beyond point mass tracking: tape measure and protractor tracks, center of
  mass, vectors, models, video filters, `.trk` import, data tool fitting.
- Windows and Linux media backends (FFmpeg behind `MediaSource`) and packaging
  (signed `.app`, installers).
- Rotated (portrait phone) footage is handled in code but not yet tested on a real clip.
- Accessibility work listed in `DESIGN.md`.
