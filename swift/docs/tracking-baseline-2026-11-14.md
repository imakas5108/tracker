# Swift tracking baseline: 2026-11-14

## Delivered behavior

- Paused marking uses the displayed decoded CGImage and its actual source-frame
  presentation timestamp. Frame stepping uses indexed source timestamps, including
  variable frame rates. New seeks invalidate older requests; marking is blocked
  until the matching snapshot arrives.
- Analysis models, history, project persistence, media access, matching, admission,
  and UI controls are separated into files. The scientific engine has no SwiftUI
  dependency. Swift remains the only application language.
- Version-1 JSON projects retain source references, stable track/measurement IDs,
  pixel coordinates, timestamps, calibration, origin/score, algorithm identifier,
  and tracking settings. Saves validate before atomic replacement and run off the
  main actor. Missing source files trigger relinking; dimensions are checked.
- UI/MCP-facing future commands have one existing session mutation path. Edits
  invalidate running jobs; run tokens reject stale results after edits or closure.
- Undo/redo covers manual edits, replacement marks, calibration, and each delivered
  tracking run. Latest-time point removal is separate from undo.
- The baseline matcher uses a fixed grayscale template and squared pixel error.
  It compares the best candidate with a spatially separated alternative, checks
  quality and search boundaries, and estimates a local subpixel offset. It rejects
  featureless templates and pauses for uncertain matches. It does not use a learned
  model, adaptive template evolution, or predictive motion.
- Uncertain candidates are outlined, explained, and left unrecorded. Accept records
  reviewed provenance and starts a new fixed-template segment; Skip leaves a gap
  and continues from the previous accepted reference. A manual mark overrides a
  candidate. Navigating away clears review evidence to avoid comparing the wrong
  source image. Scores are not calibrated probabilities.
- Tracking streams frames with bounded decoder state and coalesces updates at
  about 10 Hz. Several jobs can be submitted, but the initial shared execution
  budget is one. Queued cancellation does not starve the next job. There is no
  fixed product/session cap and no claim of simultaneous multi-engine execution.

## Reproduce

```sh
python3 scripts/make-tracking-fixtures.py
CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test -c release
zsh scripts/build-app.sh
open build/Tracker.app
```

Fixtures require ffmpeg. They are generated under ignored `build/fixtures`, not
bundled in the app. The script creates 60-frame, 160 × 120 synthetic clips with
alternating 0.04/0.08-second frame durations. A textured target follows known pixel
coordinates. Variants add deterministic noise, remove the target at frame 20,
or introduce an identical competing target at frame 20. These are development
fixtures, not experimental measurements.

## Automated validation

Release suite: 15 tests passed with no skipped tests in the recorded run. Tests
cover letterboxing, calibration geometry, point replacement undo/redo, project
round-trip and invalid-version rejection, preservation of an existing file after
an invalid save, cancellation, queue recovery, template ambiguity, occlusion,
source-reader/snapshot agreement, VFR timing, stale-seek rejection, and restored
project IDs/calibration/source frame.

Clean and noisy clips each produced all 59 post-seed positions. Maximum localization
error was 0.000 px on the clean clip and 0.012684 px on the noisy clip. Both occlusion
and distractor variants stopped at 1.200 s, leaving 19 automatic points plus the
manual seed, and did not record the uncertain frame.

### Resource baseline

Hardware: Apple M4, 16 GiB RAM. macOS 27.2.1. Release tests run through the available
Xcode/Swift toolchain. These are one-run, small-fixture observations, not statistical
performance guarantees or comparisons with upstream Tracker.

| Submitted jobs | Active tracking budget | Post-seed frames | Wall time | CPU time | Aggregate frames/s | Test-process peak RSS |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 1 | 59 | 0.1501 s | 0.1349 s | 393.0 | 128,040,960 B |
| 2 | 1 | 118 | 0.3019 s | 0.2719 s | 390.9 | 128,237,568 B |
| 4 | 1 | 236 | 0.6428 s | 0.5706 s | 367.2 | 128,532,480 B |

Peak RSS is `getrusage`'s cumulative high-water mark for the test runner, including
other tests. It is not incremental per-job memory or full-app memory. CPU seconds
cover each workload interval. Decoder/OS caches may be warm; test order affects
timing. A separate cancellation observation completed in 0.0067 s after requesting
cancellation. Input latency, rendering rate, thermal stability, large-media scaling,
and full-app peak memory have not been measured.

## Live debugging

Used Open Computer Use with granted Accessibility/Screen Recording, as requested.
Numeric fields used keyboard events when AX value changes did not update SwiftUI
bindings. No locked-use settings or permissions were changed.

Observed in the running app:

- Opened clean/occlusion fixtures and entered source point (30,45) at t=0.
- Started autotracking and observed Awaiting review at 1.200 s with 20 recorded
  points; the candidate remained unrecorded, with score/separation and explanation.
- Saved the project through the native panel, read the actual JSON file, closed
  its tab, reopened it through Open Project, and observed the same 20 points and
  1.200 s frame time.
- Undo restored the manual seed; Redo restored the 20-point result and saved-state
  comparison. Found and fixed a stale Awaiting review label after undo/stop.
- Added explicit origin codes and chart symbols so manual and automatic positions
  are inspectable without relying only on color.
- On the final WindowGroup build, opened the seeded clean project and completed
  all 59 subsequent frames. Removed a point, requested native window close, and
  used Escape to cancel the save prompt. The same process retained the 59-point
  unsaved session; Undo restored 60 points. The earlier native-close tool action
  instead exercised Save/close (the project file contained all 60 points), so that
  observation alone was not treated as proof of cancellation. The final check used
  an explicit cancel key and verified retained state.

## Interface review

- Layout/direction PASS: preserved the video-centered split panes from `DESIGN.md`;
  tracking controls occupy a compact strip and review controls appear contextually.
- Functional baseline PASS: release build, listed automated checks, and recorded
  live tracking/save/reopen/undo checks succeeded. No dummy tracking controls remain.
- Honest state/provenance PASS: native labels explain queue/review/seek/save states;
  uncertain matches remain outside accepted data; M/A/R and chart symbols identify
  origin. No unsupported speedup or accuracy claims are shown in the app.
- Accessibility baseline: native labeled controls, keyboard point/calibration
  entry, cancellable dialogs, existing semantic system colors, and black/white
  over-video markers are retained. Full theme, resize, focus-order, and assistive
  technology certification is still unverified; this is not a full accessibility
  conformance report.

## Limits and next decisions

- Tiny synthetic clips are insufficient to choose a final concurrency budget or
  establish suitability for noisy experimental footage. Benchmark representative
  1080p/4K clips and actual UI input latency before increasing parallel admission.
- Rust remains optional. No Rust implementation or same-algorithm comparison has
  been measured. The present results do not establish a language bottleneck.
- Projects reference footage rather than embedding it. Relinking checks dimensions,
  not a cryptographic content identity; users must choose the original footage.
- Pending candidates, in-flight jobs, undo history, and snapshot images are not
  persisted. Reopened projects retain accepted observations and settings; a new
  run needs a marked source frame. Crash recovery/autosave is not implemented.
- Timestamp indexing retains frame metadata, not decoded frames. Result/history
  growth still needs large-project profiling. The first engine has no adaptive
  keyframes, automatic global reacquisition, or motion model.
- Accepting a wrong match is possible and is an explicit reviewed observation,
  not a guarantee of correctness. The user can undo/correct it. Automatic rows
  never overwrite manual or reviewed rows already present at a timestamp.
- Performance tests are intentionally optional when fixtures are absent and
  explicitly skip in that case. Generate fixtures to reproduce the complete suite.
- MCP, upstream `.trk`/`.trz` compatibility, and non-macOS releases remain deferred.
