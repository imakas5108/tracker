# Motion-analysis delivery verification

## Implemented behavior

Configurable static axes, position, vx/vy, speed and ax/ay now share one Swift
calculation pipeline across the native plot/table, CSV and MCP. Three-point
quadratic differentiation is the default; 5/7/9-point local quadratic windows
optionally smooth derivatives. Source pixels remain unchanged. Rational stamps
take precedence over rounded legacy seconds. Gaps, endpoints and ill-conditioned
windows do not produce fabricated derivatives.

Axes, calibration and derivative settings are undoable and preserve active or
queued image tracking. Append-only calculation reuses the unchanged prefix and
recalculates the derivative tail. Generation checks reject stale UI results;
MCP validates revisions before returning computed rows. New projects save version
2; version-1 projects load with compatible defaults. CSV retains its first five
columns and appends derived results and processing metadata.

The native controls reference Java Tracker's axes toolbar, plot-axis variable
menus and selectable table columns. The reference images were inspected, not
bundled. [The guide](motion-analysis.md) links the official documentation and
defines numerical differences and limitations.

## Executed checks

- Normal package build and ad-hoc signing: `zsh scripts/build-app.sh` succeeded.
- Exact documented release command succeeded:
  `CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test -c release -Xswiftc -swift-version -Xswiftc 5`.
  **35 XCTest tests and 11 Swift Testing tests passed**, including parameterized
  cases. Required media fixtures were present; no media checks skipped.
- Scientific coverage includes stationary/constant-velocity, ballistic and
  circular trajectories, irregular timestamps, exact-stamp precedence, axes,
  calibration, smoothing, gaps, short tracks, corrections, cache cancellation,
  version migration and incremental/full-calculation equality for all four windows.
- Queued-job commands verify that axes, calibration and method changes preserve
  tracking and update its reported revision. Retry/conflict/stale-edit and Undo
  behavior are checked through shared commands.
- All **18 packaged MCP checks** passed: the existing 17 plus terminal-job revision
  retention after a later axes edit.
- All **13 additional motion MCP checks** passed, covering independent finite
  differences, rotation, correction invalidation, smoothing, CSV/API equality,
  version-2 reopen, version-1 defaults, gaps and rounded legacy timestamps.
- Python syntax, patch whitespace and bundle signature verification passed.

The initial 33-test release baseline passed before implementation. Existing
Swift 5 mode, SDK pin and dependency graph are unchanged. The system emitted
existing contactsd/CoreData diagnostics during media tests; tests still passed.

## Actual Codex and native controls

An ephemeral Codex client operated a private native acceptance app: import,
decode/inspect frame, explicit calibration, seed, autotracking, data inspection,
axis rotation/undo, smoothing, save/export and reopen. It confirmed 72 observations,
stable track ID, exact fractional stamps and restored scientific settings. One
start request used an incorrectly advanced revision after a no-op configuration;
the server rejected it, and Codex refreshed/retried successfully. No acceptance
failures remained.

At t = 1.25125 s, a tracked synthetic sample gave:

| Window | vx (m/s) | vy (m/s) | ax (m/s²) | ay (m/s²) |
| ---: | ---: | ---: | ---: | ---: |
| 3 | 4.55549 | 1.31863 | -0.00382 | -5.74643 |
| 7 | 4.48695 | 1.25018 | -0.00053 | -3.01083 |

The generator's vertical acceleration is -3 m/s² after y inversion and supplied
100 px/m calibration. This illustrates pixel quantization amplified by derivatives,
not general evidence that a particular smoothing window is optimal.

Native checks used the previously requested Open Computer Use workflow with
screenshots disabled. Executed actions showed Axes fields, entered 90 degrees
with real keyboard events and Return, selected vx in the plot and table menus,
undid the rotation, and selected the three-point method. Accessibility state
showed the updated angle, plot variable, table values, missing endpoint cells and
method. MCP independently confirmed native rotation/Undo/method state. Direct AX
set-value followed by AX button activation did not commit the numeric draft;
it was not counted as a successful edit. Keyboard entry and submission did commit.

ScreenCaptureKit repeatedly failed with error -3811, including through the unified
computer-use tool. Consequently screenshots, pixel-level layout/contrast review,
dragging and comprehensive accessibility are **not verified**. Accessibility and
Screen Recording permissions were already granted; no privacy or Locked Use
settings were changed. Native accessibility evidence is distinct from screenshot
review and from decoded video-frame inspection.

Local artifacts under `build/audit/motion/` include mcp-report.json, codex-summary.txt,
codex-events.jsonl, projects/CSV, native-axis-command-state.json,
native-method-command-state.json and native accessibility JSON snapshots.

## Release measurements

Apple M4, 16 GiB, macOS 27.2.1, Swift 6.4.2. Standalone numerical code compiled
with `swiftc -O`; three repetitions, median full-calculation times:

| Observations | Window 3 | Window 7 |
| ---: | ---: | ---: |
| 1,000 | 3.14 ms | 2.83 ms |
| 10,000 | 23.44 ms | 25.93 ms |
| 100,000 | 238.22 ms | 265.79 ms |

Appending 250 observations to a cached 100,000-row track with window 7 took
10.86 ms median, including prefix validation and result-array assembly. This
excludes SwiftUI rendering and session history. Concurrent numerical workloads
of 1/2/4/8 tracks (10,000 rows each, window 7) took 26.60/53.78/36.94/49.33 ms;
the isolated benchmark process's cumulative RSS peak reached approximately
50.8 MiB. These are numerical tasks, not simultaneous video decoders.

The native app workload used synthetic 1920x1080, 72-frame clips at 24000/1001 fps,
template radius 12, search radius 64, score 0.90 and separation 0.015. Requested
jobs used the unchanged single execution slot. Comparisons accumulate open
sessions, so later RSS includes earlier projects and acceptance sessions.

| Requested videos | Wall time | Automatic frames/s | MCP RTT p95 | Sampled app RSS peak |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1.793 s | 39.6 | 21.73 ms | 236.5 MiB |
| 2 | 3.390 s | 41.9 | 20.23 ms | 240.6 MiB |
| 4 | 6.643 s | 42.7 | 13.65 ms | 241.7 MiB |
| 8 | 13.184 s | 43.1 | 13.05 ms | 253.0 MiB |

Queued work was observed for 2/4/8 requested jobs. Maximum sampled MCP RTT was
39.65 ms. Queued/running cancellation acknowledgement took 52.24/42.17 ms;
decoder teardown was not measured separately. Final system free-memory percentage
was 56%, not a sustained pressure test. RTT is a main-actor responsiveness proxy,
not measured input-to-render latency.

Throughput is lower than the earlier audit report's 68–80 frames/s. Starting
session mixes and background activity were not controlled across those runs;
there is no contemporaneous A/B test to establish the cause or rule out a
regression. No whole-app performance improvement is claimed. Prior render hitches,
huge-track video overlays, long-duration pressure/thermal behavior and real-footage
accuracy remain follow-up work before increasing parallel admission.

Raw data: numerical-benchmark-final.txt and workload/workload-report.json.
Reproduction scripts: benchmark-motion.swift and benchmark-mcp-workload.py.

## Test isolation and delivery gate

The regular app initially had control disabled. A separate test bundle and private
TRACKER_CONTROL_DIRECTORY allowed acceptance without replacing its endpoint.
TMPDIR alone was ignored by macOS. Runtime override validation retains private
ownership/permissions, symlink rejection, peer UID checks and exclusive locking;
directory regressions passed. No normal Codex configuration was modified.

The user subsequently enabled control. The regular app was observed empty, but
automatic approval review rejected a restart attempt because quitting could affect
workspace state. No restart occurred. The built bundle is updated; the original
running instance is preserved. Restart it manually after saving to load the build.
Reconnect MCP clients to discover the two new tools.

- Functional gate PASS for the executed native keyboard/menu paths and shared
  command tests above. Screenshot and drag verification remain explicitly open.
- Purpose/identity gate PASS: source review and inspected Java references place
  real scientific controls beside video, plots and table; no copied assets or
  decorative UI were added.
- Consistency gate PASS for system controls, shared units/calculations, source
  provenance and explicit unavailable states. Pixel-level theme/contrast and
  full accessibility review remain unverified rather than reported as passing.

There is no Rust/GPU addition, new matcher, wider tracking scheduler, remote
control service, automatic gap fill, uncertainty estimate or zero-lag claim.
