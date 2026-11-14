# Tracker for macOS

A native Swift analysis baseline for video analysis on Apple Silicon.
See [ARCHITECTURE.md](ARCHITECTURE.md) for product goals and [DESIGN.md](DESIGN.md)
for interface decisions.

## Build and run

Requires macOS 14+, Xcode command-line tools, and Swift 6.1+. The MCP bridge uses
the official Swift MCP SDK pinned to 0.11.0; dependency resolution is included.
macOS 14 and Swift 6.1 are provisional floors, not tested compatibility claims.
The build script uses Swift 5 language mode for the pinned SDK on newer toolchains;
see [the connection guide](docs/mcp.md) for the measured compatibility issues.

```sh
zsh scripts/build-app.sh
open build/Tracker.app
```

Open `Package.swift` in Xcode to develop, or run
`swift run -Xswiftc -swift-version -Xswiftc 5 Tracker`.
Generate synthetic fixtures with `python3 scripts/make-tracking-fixtures.py`
and `python3 scripts/make-balltoss-fixtures.py`
(requires ffmpeg), then run
`CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test -c release -Xswiftc -swift-version -Xswiftc 5`.
Without fixtures, media integration/performance tests explicitly skip; unit tests
still run. Do not report skipped integration tests as validated.

## Current workflow

1. Open one or several videos with Open Video or Command-O; switch or close tabs.
2. Use playback, scrubbing, and previous/next frame controls to inspect the video.
3. Choose Mark points, then click a position. A point mass is created if needed.
   Each source timestamp holds one point per track; another click replaces it.
4. Choose Calibrate, click two endpoints, and enter their known distance and unit;
   or use Set Scale to enter endpoint pixel coordinates directly.
   Coordinates use a bottom-left origin, x right, y up. Without scale they use px.
5. Inspect x/y plots and the table. Remove Last removes the latest-time point.
   Export CSV writes the selected track's measurements.

## Autotracking and projects

Mark a target on the current paused frame, then choose Start Tracking. Settings
controls template radius, local search radius, minimum similarity, and separation
from competing candidates. The fixed grayscale template must contain detail;
uniform patches are rejected. A low-quality or ambiguous match stops processing
for Accept & Continue, Skip Frame, manual correction, or Stop. Accepting a candidate
records it as reviewed and starts a new fixed-template segment. Scores are not
confidence probabilities. No learned model, predictive motion, or adaptive template
evolution is implemented yet.

Save Project (Command-S) writes a versioned `.tracker.json` file referencing the
source video, with track IDs, source-frame timestamps, settings, calibration, and
measurement provenance. Open Project (Command-Shift-O) restores it and asks to
locate missing source footage. Relink only the original video; equal dimensions
alone do not establish file identity. Pending review state and active jobs are not
restored. Undo/Redo covers edits and each delivered tracking run as one operation.
Remove Last remains an explicit deletion, not undo.

Several videos may be open and jobs may be submitted without a fixed product cap.
The baseline admits one tracking job at a time; others queue and may be cancelled.
Dynamic parallel admission is future work. Switching tabs pauses playback.
Use Enter Position (Command-Shift-M) for keyboard entry of source pixel coordinates.
Menus, transport, selectors, and dialogs use native controls. Full scientific,
accessibility, large-data, and sustained performance validation remain later work.
CSV formatting and file writing run in the background. CSV includes measurement
origin and match score. Source-frame marking is blocked while a seek is pending;
the paused image and its actual source timestamp are the marking reference.
Save before closing; tab-close and application-quit prompts protect unsaved edits.
Configurable coordinate origins and Tracker file compatibility remain deferred. Full production accessibility and large-data validation remain open.

See [the tracking validation report](docs/tracking-baseline-2026-11-14.md) for measured
results and limitations. The current engine remains Swift; no Rust speedup is claimed.

The interface was informed by Tracker's public documentation and screenshot;
no upstream code or assets were copied. Reference and implementation boundaries
are recorded in [docs/interface-review.md](docs/interface-review.md).

## Codex MCP control

The bundled `TrackerMCP` executable automatically launches Tracker when needed
and exposes local session, frame, calibration, tracking/review, history and output
tools. Agent actions appear in the native workspace, with a visible stop control.
The app and MCP share validated scientific commands. Read
[connection, tool and coordinate documentation](docs/mcp.md) before enabling the
project connection. Run `python3 scripts/test-mcp.py` for the packaged stdio
workflow after generating fixtures and building the app. Third-party licenses
are in `THIRD_PARTY_NOTICES.txt` and the application bundle.

See [the MCP validation report](docs/mcp-validation-2026-11-14.md) for actual
Codex results, native control checks, compatibility workarounds and limits.

The audit fixes preserve rational frame times through media decoding, keep track
selection outside scientific history, and retain terminal MCP job records without
keeping closed video sessions alive. See [the resolution report](docs/audit-resolution-2026-11-14.md)
for regression coverage, native checks, workload measurements and remaining limits.
