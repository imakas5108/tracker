# Audit resolution, 14 November 2026

The five audited defects were corrected in the native Swift implementation.
The fixed grayscale matcher, single active execution slot, MCP input schemas,
project version, platform scope and visual layout are unchanged.

## Changes and regression evidence

| Finding | Correction | Executed validation |
| --- | --- | --- |
| Rounded frame requests selected preceding fractional-rate frames | Pass FrameStamp/CMTime directly to image generation and reader startup. Use CoreMedia output timing for composition offsets. Ignore empty edit spans. | Paused and sequential frames agree across 12 exact frames each in clean, 90-degree rotated and 0.5-second-offset fixtures; original VFR regressions pass. Codex retrieved exactly 0.125125 s and 0.625125 s. |
| Selecting a track cancelled tracking and changed history/revision | Keep live selection outside scientific AnalysisState/history. Persist selection only when making a project snapshot; normalize loaded/saved scientific state. | Queued selection changes preserve revision and jobs; undo/redo preserve a valid selection. Native keyboard selection during a queued job left revision 25 unchanged and the job subsequently completed with 72 observations. |
| Retained operations kept closed sessions alive | Use weak session references and retain lightweight job records plus terminal operation metadata. | Both import and tracking operations remain inspectable after weak-reference deallocation checks confirm the session is released. Replaying the original import request does not reopen the session. |
| Each new automatic observation scanned the full track on the main actor | Add sorted lookup and a chronological batch-append path, preserving manual/reviewed points. | Ordered merge regression preserves both authoritative origins; optimized model benchmark compares identical results against the previous scan loop. |
| MCP could start from an old paused snapshot during playback | Reject starts while playing or seeking in the shared command. | Actual AVPlayer playback start is rejected without a job or revision change. Native controls retain their existing disabled-state behavior. |

The disconnect test now removes the client that actually started the job, and a
separate regression verifies that disconnected work finishes. Queued cancellation
waits for following capacity admission rather than a fixed delay. Running-engine
cancellation waits for an engine event rather than assuming a 20 ms delay means
execution has begun.

## Executed checks

- Release build: `zsh scripts/build-app.sh`.
- Release suite: 33 tests passed, zero failures and no fixture skips, with the
  documented Swift 5 language-mode flags for SDK 0.11.0.
- Packaged stdio workflow: all 17 checks in `scripts/test-mcp.py` passed.
- Actual ephemeral Codex client: inspected source/preview mapping, verified the
  fractional and offset timestamps, calibrated, marked, tracked 72 observations,
  saved/exported/reopened with stable track ID, reviewed the missing target and
  cancelled without accepting it. Artifacts: `build/audit/resolution/codex-summary.txt`,
  project JSON, CSV and tool-event JSONL.
- Open Computer Use: observed native calibrated results, and used explicit list
  focus plus Up to select another track while tracking was queued. The queued
  job retained revision 25 and subsequently completed with 72 observations.
  Accessibility activation alone did not change the row and was not counted.
- Patch whitespace and bundle signature checked at delivery.

The final rerun also passed all 33 tests after the cancellation-test adjustment.
Swift's default build engine stalled before compilation in macOS working-directory
file access; the native build engine subsequently stalled generating debug symbols.
The successful final command was
`CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test --build-system native -c release -debug-info-format none -Xswiftc -swift-version -Xswiftc 5`.
This changes verification tooling only; the packaged app had already built with
the normal script. A later Computer Use readback failed to connect to its helper;
the native-selection evidence above was captured before that failure. MCP still
confirmed selection of the saved, calibrated 72-observation acceptance session.

## Synthetic ball-toss validation

Run `python3 scripts/make-balltoss-fixtures.py`. It generates 1920x1080, 72-frame
clips at 24000/1001 fps, analytic/rasterized coordinate annotations, an occluded
variant hiding frames 28 through 35, and rotation/offset variants. The reference
ruler is explicitly constructed as 100 px per metre. These are original synthetic
fixtures, not the upstream BallTossOut sample or real camera footage.

With the existing default matcher settings:

| Clip | Automatic observations after seed | Maximum error against rasterized truth | Review |
| --- | ---: | ---: | --- |
| Clean | 71 | 0.000780 px | None |
| Occluded | 27 | 0.000772 px | First hidden frame, 28028/24000 s |

The occlusion job holds 28 stored observations including its manual seed. The
candidate remains unrecorded. Small errors on a translated synthetic texture do
not establish real-world accuracy under lighting changes, blur or deformation.

## Optimized update benchmark

Apple M4, 16 GiB, macOS 27.2.1, Swift 6.4.2. Compile
`swiftc -O Sources/Tracker/Measurement.swift scripts/benchmark-tracking-updates.swift -o /tmp/tracker-update-benchmark`
and run the executable. Five repetitions; medians shown. The reference path is
the per-observation scan from f5d5470. Outputs are checked for equality.

| Existing observations | Incoming batch | Previous loop | New append path |
| ---: | ---: | ---: | ---: |
| 1,000 | 250 | 0.417 ms | 0.0225 ms |
| 10,000 | 250 | 3.147 ms | 0.0428 ms |
| 100,000 | 250 | 15.237 ms | 0.595 ms |

This measures model update work on the main actor, excluding view rendering,
full session/history bookkeeping, video decoding and file I/O. It does not imply
an equivalent whole-app speedup.

## Native release workload

`scripts/benchmark-mcp-workload.py` submits synthetic 1080p clips with template
radius 12, search radius 64, minimum score 0.90 and separation 0.015. The shared
execution budget remains one; 1/2/4/8 are requested jobs, not parallel decoders.
Each comparison opens additional sessions, so later RSS also includes earlier
sessions. Each processed video retains 72 observations and is saved.

| Videos | Automatic frames | Wall time | Aggregate throughput | MCP RTT p95 | Sampled app RSS peak |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 71 | 1.044 s | 68.0 frames/s | 16.71 ms | 165.1 MiB |
| 2 | 142 | 1.778 s | 79.9 frames/s | 15.93 ms | 175.6 MiB |
| 4 | 284 | 3.832 s | 74.1 frames/s | 16.28 ms | 187.8 MiB |
| 8 | 568 | 7.197 s | 78.9 frames/s | 15.92 ms | 167.8 MiB |

Queued work was observed at 2/4/8 jobs. The largest sampled MCP round trip was
141.49 ms in the 4-job comparison. Queued cancellation acknowledged Stopped in
27.27 ms; running cancellation acknowledged in 19.50 ms. These are protocol
acknowledgements, not decoder teardown timings. Main-actor reply latency is a
responsiveness proxy, not direct mouse-to-pixel latency. System free-memory
percentage was 31% at the final snapshot, not a sustained pressure test.

Raw data: `build/audit/resolution/workload-report.json` and memory-pressure.txt.

## Instruments observations and limits

A Tracker-only SwiftUI trace covers approximately 21.1 seconds of the workload.
Hang triage returned no entries; that is not proof that no stalls are possible.
The hitch table contains 45 events, median 16.666 ms and maximum 133.328 ms.
There is no matching before trace, so no frame-rendering improvement is claimed.
The remaining render hitches warrant a separate trace-driven investigation.
Long-track model updates were benchmarked separately; the native videos hold
72 frames each, so this trace does not validate charts containing 100,000 points.

The installed analyzer could not parse xctrace's table of contents because its
export contained raw ANSI control characters in environment metadata. Direct
exports of the SwiftUI updates and hitches tables were parsed instead. Nested
SwiftUI durations are not additive CPU totals. The trace, exports and summary
are local build artifacts, not committed assets.

The LaunchServices-started app initially stalled in AVFoundation file-open calls;
a focused thread sample captured that behavior. Launching the same bundled
executable directly under the development runner resolved it and enabled the
end-to-end checks. No system privacy settings were changed. Automatic app launch
reached the native workspace, but automatic-launch media import is not counted
as passing in this run. The underlying launch-environment cause remains open.

## UI delivery gate

PASS: behavior. Existing native controls were exercised with actual MCP jobs;
selection preserves active work and the app's scientific edit revision.

PASS: purpose and identity. No visual surfaces or assets were added; the compact
Tracker panes, calibration and adjacent results remain as defined in DESIGN.md.

PASS: consistency. No layout or theme changes; keyboard list selection was
verified through Open Computer Use and native state matched tool results.

Full accessibility, long-duration pressure, real-footage accuracy and sustained
thermal performance remain unverified. No zero-lag or unlimited-capacity claim is
made. No Rust migration, new matcher or wider scheduler was introduced.
