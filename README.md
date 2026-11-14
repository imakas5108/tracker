# Tracker for macOS

A native Swift interface prototype for video analysis on Apple Silicon.
See [ARCHITECTURE.md](ARCHITECTURE.md) for product goals and [DESIGN.md](DESIGN.md)
for interface decisions.

## Build and run

Requires macOS 14+, Xcode command-line tools, and Swift 6+. There are no external
package dependencies. macOS 14 is a provisional deployment floor, not a tested
compatibility claim.

```sh
zsh scripts/build-app.sh
open build/Tracker.app
```

Open `Package.swift` in Xcode to develop, or run `swift run Tracker`.
Run tests with `CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test`.

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

This build does not implement autotracking, project save/reopen, configurable
coordinate origins, MCP, or batch scheduling. Switching tabs pauses playback.
Use Enter Position (Command-Shift-M) for keyboard entry of source pixel coordinates.
Menus, transport, selectors, and dialogs use native controls. Full scientific,
accessibility, large-data, and sustained performance validation remain later work.
CSV file writing runs in the background; large result formatting still needs
profiling before production use. Measurements are held in memory until exported;
closing a video or quitting discards them.

The interface was informed by Tracker's public documentation and screenshot;
no upstream code or assets were copied. Reference and implementation boundaries
are recorded in [docs/interface-review.md](docs/interface-review.md).
