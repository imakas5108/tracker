# Real-video agent stress test — 15 November 2026

The large video imports promptly, but the current fixed-template tracker does
not reliably follow this freely oscillating weight without frequent review.
This is a measured limitation, not a completed 21-minute tracking result.

## Configuration and evidence

- Apple M4, 16 GiB RAM, macOS 27.2.1, Swift 6.4.2, release build in Swift 5
  language mode. Swift/AVFoundation engine; one active tracking slot.
- Input: `DJI_20261021143159_0203_D.MP4`, 1,494,019,426 bytes (1.494 GB decimal),
  1920 × 1080 HEVC, 30000/1001 fps, 38,265 video frames, 1,276.7755 seconds.
  Source timebase 1/120000. The original was neither modified nor transcoded.
- Used the packaged stdio MCP bridge and an isolated native app with a private
  socket. Existing user workspaces were left open. Tests used the current CSV
  worktree on top of `c827d7c`; matching/scheduling algorithms were unchanged.
- Local artifacts: `build/stress-dji/`. Import, frame mappings, session/job
  records, per-run polling, measurements, PNG evidence, calibration notes,
  exports and saved/reopened project records are retained there. The private
  source footage and generated images are not committed to the repository.

## Requested capabilities

| Check | Observed result |
| --- | --- |
| Import approximately 1.5 GB instantly | Acknowledgement 12.5 ms; indexing and first frame ready in 2.541 s. Prompt asynchronous import, not instantaneous readiness. |
| Agent locates an analysis start | Agent inspected timestamped frames and chose 84.2842 s after a large pull and release. Native seek and explicit seed worked. |
| Clip the video | No persisted in/out range or MCP clip command exists. Seeking/seeding excludes the earlier frames from this run; it does not trim the video. |
| Recognize the weight | Agent vision identified the disk and supplied a source-pixel seed. The app itself has no semantic object detector. |
| Recognize the ruler and scale | Agent identified the ruler; the user explicitly confirmed 1 m with centimetre markings. Approximate calibration was applied from selected graduations; see limitations below. No built-in OCR/scale detector was tested. |
| Uncertainty and correction | Competing/changed-appearance matches paused visibly, remained unrecorded, and could be corrected through the shared commands. |
| Complete unattended tracking | Failed on the tested free-oscillation segments. Neither the remaining tail nor every trial in the full recording was tracked. |

The start is the first clearly unobstructed frame **among the refined samples of
the selected pull**, not proof of the recording's first release. Earlier handling
and smaller motions are present. The reviewed 84.1841 s sample still has a thumb
near the moving disk; at 84.2842 s it is clearly separated. This approximately
0.1-second bracket is not a frame-exact release annotation. Original source times
are retained; the timeline was not silently rebased to zero.

## Tracking results

Coordinates are original top-left source pixels. Template matching uses a fixed
grayscale squared-error patch; it has no rotation/blur adaptation or motion
prediction. Minimum score and separation are heuristic thresholds, not calibrated
probabilities.

| Attempt | Seed | Settings: template/search radius, score/separation | Processed frames | Review time and reason | Observed wall time to review |
| --- | --- | --- | ---: | --- | ---: |
| Conservative | 84.2842 s, (903, 318) | 12/64 px, .90/.015 | 16 | 84.818067 s; competing matches (.9249 score, .00106 separation) | 1.311 s |
| Manual fresh reference | 84.818067 s, (906, 189) | 12/64 px, .90/.015 | 18 | 85.418667 s; changed appearance (.8878 score) | 1.281 s |
| Separate diagnostic | 89.9899 s, (912, 306) | 12/24 px, .85/.005 | 91 | 93.026267 s; competing matches (.9257 score, .00165 separation) | 2.305 s |

Each processed count includes the final unrecorded candidate. The conservative
track retains 34 observations: two manual seeds/corrections and 32 automatic
points, spanning 84.2842–85.3853 s. The explicitly named **DIAGNOSTIC relaxed
thresholds unvalidated** track has 91 observations: one manual seed and 90
automatic points, spanning 89.9899–92.9929 s. No uncertain candidate was accepted.
Automatically accepted locations have not all been independently annotated;
acceptance by these thresholds is not an accuracy certificate.

Short-run effective rates were approximately 12.2, 14.1 and 39.5 processed fps,
including startup and polling delay. These are not sustained-throughput
benchmarks. The smaller search and relaxed thresholds change both cost and
acceptance behavior, so the diagnostic is not a controlled speed comparison.
The weight changes appearance through rotation and motion blur, with additional
focus/framing changes. A reference refresh helped only briefly. Loosening
thresholds increased uninterrupted coverage but did not establish accuracy.

## Calibration and output limits

The user confirmed **1 metre, centimetre markings**. A test reference uses
visually estimated 20 cm and 40 cm graduation positions in the original frame:
(843, 238) to (842, 414), known distance 0.2 m, approximately 880.01 px/m.
Pixel placement is approximate (about ±2 px per endpoint by visual judgement),
not a measured statistical uncertainty. One pixel of separation error changes
this scale by about 0.57%. The enlarged ruler crop and exact command are retained
as `ruler-upper-enlarged.png` and `calibration-reference.json`.

Manually verify the graduation identities and endpoint placement before using
these exports scientifically. The ruler/weight depth difference, perspective
and small camera movements were not corrected. Calibration does not validate
the tracker. Default axes remain bottom-left, x right and y up: exported y is
not automatically displacement from equilibrium. Velocity/acceleration exist
but amplify localization noise and are not validated physical results here.

Saved and reopened `build/stress-dji/dji-stress.tracker.json`, with the source
reference, exact stamps, calibration and both tracks. Standard full-precision CSV
and Excel decimal-comma CSV rounded explicitly to six decimal places are saved
for each track. The project retains full precision. Notes are in the sidecar
report; the current project format does not serialize per-run settings history,
review evidence or calibration uncertainty. Preserve these notes with the project.
After save/reopen verification, only the saved view selection was changed to
the conservative track, then reopened in the final build. Its 34 observations,
the diagnostic's 91 observations and calibration were unchanged.

## Responsiveness and cancellation

- Sparse status samples during these short runs returned in about 5–79 ms.
  These are MCP round trips, not measured mouse-to-display latency or a UI p95.
- Peak sampled app RSS: 237,840 KiB (232.3 MiB), not an instrumented peak or
  a total-system memory measurement. The app also held a synthetic test session.
- An active diagnostic job was stopped after five processed frames. Cancellation
  acknowledged in 11.6 ms and its retained status immediately reported stopped.
  Samples after the call showed approximately 160 MiB retained RSS; CPU readings
  fell from 111.4% to 34.1% and then 0.6% after sequential waits totalling 1.5 s
  plus sampling overhead. macOS 100% CPU denotes roughly one core. This does not
  prove decoder cleanup completed within the acknowledgement time.
- Open Computer Use accessibility confirmed the native review reason, score,
  unrecorded-candidate controls, source time, scale and reopened table state.
  Screen capture previously failed with ScreenCaptureKit error -3811, so canvas
  appearance, rendered-frame latency, playback smoothness and thermal behavior
  remain unverified. No zero-lag or unlimited-capacity claim follows.
- Import was measured once after metadata inspection, not as a controlled
  cold-cache experiment. Import-time peak memory was not sampled. This test
  does not establish scaling across several simultaneous large videos.

## Actions justified by this test

1. Add persisted source-frame in/out ranges and shared native/MCP commands, with
   exact timestamps and optional trial-relative output time. Keep source files
   unchanged and distinguish analysis range from destructive trimming.
2. Provide an inspectable keyframe workflow and labeled real-frame acceptance
   set before changing the matcher. Evaluate normalized matching, bounded motion
   prediction, appearance changes and competing candidates against localization
   error and false acceptance, not just longer uninterrupted runs.
3. Add scale proposals with source-frame evidence, explicit user-confirmed
   units/distance and editable endpoints. Preserve reference uncertainty and
   warn about measured camera motion only when evidence supports it.
4. Until those pass: use manually verified scale endpoints, short reviewed runs
   and manual correction at appearance changes. A visible marker on the weight,
   fixed focus/camera and shorter per-trial source ranges could improve future
   recordings; their benefit has not been measured here.

No Rust backend, learned detector, optical-flow replacement, new scheduler or
automatic acceptance policy was introduced to make this stress test appear to pass.
