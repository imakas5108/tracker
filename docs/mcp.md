# Local Codex control

Tracker exposes 21 tools through the official Swift MCP SDK, pinned to **0.11.0**.
`TrackerMCP` is bundled beside the app executable. Codex speaks MCP over stdio;
the bridge sends length-prefixed JSON to Tracker over a Unix-domain socket.
There is no HTTP listener, remote access, or continuous video/audio stream.

## Build and connect

Use an Apple Silicon Mac, macOS 14+, Xcode command-line tools and **Swift 6.1+**.
The tested toolchain is Swift 6.4.2; macOS 14 and Swift 6.1 remain untested floors.
`Package.resolved` records the dependency graph. Four supporting dependencies
are constrained in `Package.swift` to avoid raising the Swift tools floor.

```sh
zsh scripts/build-app.sh
python3 scripts/make-tracking-fixtures.py
CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test -c release -Xswiftc -swift-version -Xswiftc 5
python3 scripts/test-mcp.py
```

The SDK's unused Network transport fails Swift 6.4 region-isolation checks in
Swift 6 language mode. The build script explicitly uses Swift 5 language mode for
the dependency graph. SDK source is unchanged. The compiler emits language-mode
warnings; this is a compatibility workaround, not a claim of Swift 6 concurrency
validation. SDK 0.11.0 also types experimental client capability values as
strings, while Codex 0.166.0 sends an object for `codex/auth-change`. A narrow
stdio adapter omits object-valued experimental declarations during initialization
only. Tracker implements no such extension; other capabilities and all tool
messages pass through unchanged. Both compatibility cases have regression checks.
Apply the same flags to direct `swift build`, `swift run` or `swift test`
commands. The protocol and app remain Swift implementations.

After the build/protocol checks pass, use this project-scoped Codex configuration.
Merge just this table into `.codex/config.toml`; preserve other settings. The
absolute command must match your checkout or installed application location.

```toml
[mcp_servers.tracker]
command = "/Users/imakas/Documents/tracker/build/Tracker.app/Contents/MacOS/TrackerMCP"
startup_timeout_sec = 25
tool_timeout_sec = 60
```

Codex only loads project configuration for trusted projects. Restart the MCP
connection/client after adding it, then check `/mcp`. Configuring a server does
not inject new tools into an already-running agent turn. Do not replace your
user-level Codex config. An unbundled bridge supports `--app /absolute/Tracker.app`.
See [the official Codex connection documentation](https://learn.chatgpt.com/docs/extend/mcp#connect-codex-to-an-mcp-server).

Initializing the bridge launches the bundled app with `--agent-control` if Tracker
is absent, then waits up to 15 seconds. It reuses a running instance. A manually
launched app starts with control disabled: choose **Allow Agent Control** in its
bottom bar. A running older app without an endpoint returns `app_not_ready`;
restart the newly built app. Disabled control is never automatically reenabled.
Diagnostics go to stderr, and stdout is reserved for MCP JSON-RPC.

## Workflow and commands

Every mutating tool requires a UUID `request_id`. Every session mutation also
requires `session_id` and `expected_revision` from the latest session inspection.
Tracks are addressed by `track_id`; tracking/review/cancellation by `job_id`.
Commands do not resolve their target through the selected tab.

| Tools | Purpose |
| --- | --- |
| `tracker_list_sessions`, `tracker_get_session` | Inspect stable IDs, revisions, source, settings and state. |
| `tracker_import_video`, `tracker_open_project` | Open explicit local paths; return an operation ID. |
| `tracker_select_session`, `tracker_seek` | Update native selection/viewer; seek returns an operation ID. |
| `tracker_get_frame` | Inspect an independent, timestamped PNG without seeking the viewer. |
| `tracker_add_track`, `tracker_mark_point` | Add a track and mark/correct a paused source frame. |
| `tracker_set_calibration`, `tracker_configure_tracking` | Supply a known scale reference and validated tracking settings. |
| `tracker_start_tracking`, `tracker_get_job`, `tracker_cancel_job` | Start, inspect or cancel app-owned work, including queued jobs. |
| `tracker_get_review`, `tracker_review_match` | Inspect unrecorded uncertainty, explicitly accept or skip, then follow the continuation job ID. |
| `tracker_undo`, `tracker_redo` | Use the same scientific history as native controls. |
| `tracker_get_measurements` | Paginate observations with exact frame stamps and provenance. |
| `tracker_save_project`, `tracker_export_csv` | Write explicit output paths; poll the returned operation. |

A typical sequence is import → poll → inspect frame → add track → calibrate →
seek/poll → mark seed → configure/start → poll/review → save/export → reopen.
Generate a new request UUID for each new mutation. Retrying the *same* UUID and
arguments returns its original result, including the original operation ID or
error, even after reconnecting. Reusing it with different arguments returns
`request_id_conflict`. A stale revision returns `stale_revision`; inspect again
and submit a **new** request UUID. Revisions increase on scientific edits,
settings changes and delivered tracking batches. Seek operations have a separate
generation; superseded seeks cannot silently succeed or cancel a later seek.

Operations are retained for the app's lifetime. Inspect `state`, `error`, `result`,
and the current session revision through `tracker_get_job`. File/import/seek
states include queued, running, completed, failed and cancelled. Tracking also
has awaiting_review and stopped. Cancelling completed work is harmless.
Request deduplication and operation records do not survive app termination;
restart with newly inspected session IDs and new request IDs. Jobs are not
resumed from project files.

## Scientific coordinates and review

Image operations use **orientation-corrected original source pixels**, x right,
y down, origin top-left. `get_frame` chooses the actual source frame at or before
the requested time. It returns its rational frame stamp and seconds, source
size, crop rectangle, output dimensions and resize scale. The default maximum
edge is 1280 pixels, with a 1600-pixel ceiling to bound response memory. Images
are never enlarged. An optional crop is integer source pixels and must be inside
the image. Optional overlays show observations and the supplied calibration.

Convert image coordinates before editing:

```
source_x = image_x / scale_x + crop_x
source_y = image_y / scale_y + crop_y
```

Frame inspection returns the analysis revision captured with its overlays. It
does not select a tab, pause playback, or change the viewer's displayed frame.
To mark, explicitly seek the native viewer, wait for its operation, inspect the
current revision, and supply the displayed frame's **actual source time**.
Predictions are never inserted as observations.

Calibration requires two in-image source endpoints, a positive known length,
and an explicit unit (`m`, `cm`, `mm`). Equal endpoints and implicit scale are
rejected. Measurements expose both top-left source pixels and the app's
bottom-left, y-up analysis coordinates. With no reference, units remain px.
CSV uses the analysis coordinates and includes origin and match score.

A low-quality/ambiguous match pauses the current segment. `get_review` returns
its owning job/track IDs, candidate position, frame stamp, reason, score and
competing-candidate separation. Scores are heuristics, **not probabilities**.
The candidate remains unrecorded until `review_match(action:"accept")`.
`skip` records no point. Both create a continuation job; inspect that returned
ID. A manual correction uses `mark_point` and the shared undo history. The
existing fixed grayscale template and single shared tracking slot are unchanged.

## User control, cancellation and files

The native bottom bar shows allowed/stopped control and connection count.
**Recent Agent Actions** retains the latest 30 actions. **Stop Agent Control**
revokes new requests and in-flight frame delivery; already accepted app-owned
operations remain observable. Existing tracking is stopped with the native
Autotracker Stop button. Disconnecting Codex never cancels analysis jobs.
Bridge heartbeats update a 30-second connection lease; a crashed bridge's status
may take that long to disappear.

Cancellation targets an explicit operation. Cancelling a queued/running track
uses its original execution token, so it cannot cancel a replacement job.
Cancelling an import closes its newly created session only while it is still
loading; an existing or ready session is preserved. Cancelling a seek affects
only that seek generation. File writes commit atomically once begun: a
cancellation racing a committed write can still report completed. Inspect the
terminal result before retrying.

Agent workflows use absolute paths and never show open/save/relink dialogs.
Missing footage returns `relink_required`; `open_project(relink_path:...)` uses
only the supplied replacement. Verify it is the original footage; dimensions
alone do not prove identity. Native UI actions retain their dialogs.
Existing destinations require `overwrite:true`. Writes first create a temporary
file in the destination directory, then atomically link without replacement or
rename with explicit overwrite. No check-then-write race silently replaces a
file. Output cannot replace the source footage path.

The socket lives under the user's private temporary directory in
`tracker-control-<uid>`, with directory mode 0700 and socket mode 0600. Both peers
check the other process's Unix UID. A held lock prevents competing app instances
from replacing a live endpoint. The local protocol is version 1; an incompatible
version returns `incompatible_protocol`. Connections have 30-second I/O timeouts,
a 1 MiB request limit and 16 MiB response limit, with four concurrent requests.
This isolates other users, not other programs running as the same user. There
is no remote authentication surface.
