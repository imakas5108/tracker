# Supplied Tracker sample evaluation — 15 November 2026

All **79 MP4 files** opened and delivered a source frame. Broad unattended
autotracking remains unreliable: only SHMScale provided a useful completed
whole-video run here. BallTossOut was completed over a selected visible interval
with two manual corrections. A completed strobe job was **not** a physical-motion
analysis success. These results do not establish full Java Tracker feature parity.

## Inputs and execution

The user supplied `mechanics_videos.zip` (13 videos and one JPEG) and
`spectroscopy_videos.zip` (66 videos). Originals were preserved. Extraction checks
paths, media types and existing-file identity; it does not execute archive content.
The archives and extracted upstream media are not bundled with the application.
Their hashes and exact file lists are retained in
`build/upstream-samples/archives.json`.

Tests used Apple M4 / 16 GiB / macOS 27.2.1 / Swift 6.4.2, release builds in
Swift 5 language mode. Private app/socket instances isolated the batch from user
workspaces. Video import readiness ranged from 0.106–0.333 s, median 0.123 s,
for these small samples. This was one warm batch, not a cold-cache or sustained
performance benchmark. The scheduler remained single-active.

Every MP4 was imported through the real packaged stdio MCP bridge, then asked
for an original timestamped PNG. Every mechanics clip was given an explicit
visually selected source-pixel seed, tracking attempt, retained stopping evidence,
project save and CSV export. No uncertain candidate was automatically accepted.
Spectroscopy files received import/frame checks, not spectral measurements.

Results and source PNGs: `build/upstream-results/`. The batch script is
`scripts/test-upstream-samples.py`; supplying the two archives is required.
Inspection images and extracted media remain local development artifacts.

## Autotracking comparison

The following counts include the manual seed, then automatic observations before
the first review. They are **coverage counts, not accuracy scores**. Seeding starts
inside each video at a visible target; most attempts do not cover its entire file.
The exact seeds and source timestamps are in the script and raw reports.

- **SSD:** legacy default; radius 12, search 64, score .9, separation .015.
- **ZNCC strict:** only matching method changes; the same numerical thresholds
  are deliberately diagnostic, not equivalent calibrated acceptance criteria.
- **ZNCC tuned:** radius 6, search 64, score .7, separation .015. This is tuning on
  the evaluation clips, not a held-out validation result.

| Clip | SSD points | ZNCC strict | ZNCC tuned | Main observed limitation |
| --- | ---: | ---: | ---: | --- |
| BallDrop | 2 | 2 | 4 | Changing/comb-like ball appearance; review |
| BallTossOut | 5 | 4 | 5 | Pole distractor at crossing; review |
| BallTossUp | 3 | 2 | 3 | Changing appearance and ambiguity; review |
| BlockOnCart | 26 | 26 | 29 | Target approaches image/search boundary; review |
| CupsClips | 1 | 1 | 2 | Multiple accumulated images; unsuitable for ordinary live-target interpretation |
| FlexTrackRaces | 2 | 1 | 1 | Small targets, background and edited trials; early review |
| Pendulum | 8 | 1 | 11 | Background/appearance change; review |
| PucksCollide | 29 | 5 | 15 | Rotating patterned puck and ambiguity; review |
| PucksCollideSequence | 9 | 6 | 9 | Appearance/ambiguity; edited multi-trial footage |
| SHMScale | 60 | 60 | 60 | Completes from the selected frame to the end |
| TippingCylinders | 1 | 1 | 1 | Uniform body/ambiguous local feature; review |
| TossOutStrobe | 17 | 17 | 17 | Job completes while following an old persistent strobe image |
| WhichLandsFirst | 3 | 1 | 1 | Small changing target; review |

Raw records: `baseline/report.json`, `zncc/report.json`,
`zncc-tuned/report.json`, and `comparison.json` under the results directory.
Seeds were visually placed without a complete independent annotation; seed
quality can also affect coverage. The table's explanations are observed symptoms
and plausible contributors, not isolated causal proofs for every failure.
The old default was **not replaced**: this sample comparison does not establish
an overall unattended-success advantage for normalized correlation.

## Completed workflows

### SHMScale: whole video

Retried from frame zero with the seed at the bright mass centre (132, 93.5).
Normalized correlation with radius 12 / search 64 / score .9 / separation .015
recorded **all 63 source frames**, 0–2.066667 s: one manual seed and 62 automatic
observations, no intervening correction. Both methods had already completed the
60-frame suffix in the comparison; this is not evidence that ZNCC alone enabled
completion. Refining the initial centre removes an operator-selected offset.

Seven original-pixel crops were checked against independent bright-component
centroids, with target identity inspected visually. Median distance was 0.218 px,
maximum 0.349 px. These are limited segmentation spot checks, not a complete
independent human annotation, physical centre-of-mass validation or calibrated
measurement uncertainty. See `full-workflow/shm-spot-check.json`.

Verified bridge disconnect/reconnect during tracking, full-precision and Excel
CSV export, version-3 save/reopen with identical measurement rows, invalid-method
rejection, duplicate request handling and preservation of earlier method records
after changing current settings back to SSD. No physical length was invented.

Artifacts: `full-workflow/SHMScale.tracker.json`, `SHMScale-standard.csv`,
`SHMScale-excel.csv`, `shm-rows.json` and `shm-job.json`.

### BallTossOut: assisted visible interval

Tracked frames **12–30**, 0.4–1.0 s, as 19 consecutive observations: three manual
points (initial seed plus two corrections) and 16 automatic points. The method
was ZNCC with radius 6 / search 64 / score .7 / separation .015.

At frame 17 the candidate was on the striped pole rather than the ball; the
inspected centre was corrected to (288, 97). At frame 18 another off-target
candidate was corrected to (310, 98). The next segment continued automatically
to frame 30. The out-of-frame candidate at frame 31 remained unrecorded and the
job was stopped. Earlier frames were not tracked in this assisted result.

An initial visual concern about the final preview repeating a frame was ruled
out by timestamp/pixel comparison with an independent FFmpeg decode and a test
comparing sequential and preview frames. The previous frame contains the ball;
the final frame does not. No decoder change was justified by this check.

Saved/reopened rows matched exactly; standard and Excel CSV exports completed.
Native accessibility confirmed the saved BallTossOut workspace at 0.733 s with
19 points in pixels. This establishes assisted recovery, **not unattended
tracking or universal positional accuracy**.

Artifacts: `full-workflow/BallTossOut-assisted.tracker.json`,
`BallTossOut-standard.csv`, `BallTossOut-excel.csv`, `BallTossOut-rows.json`,
`ball-corrections.json`, `ball-final-review.json` and `native-saved-ax.txt`.

## Changes delivered

- Added an opt-in, fixed-template zero-mean normalized-correlation matcher in
  Swift. The mathematical definition follows the normalized correlation formula
  documented by [OpenCV](https://docs.opencv.org/4.8.0/df/dfb/group__imgproc__object.html).
  No OpenCV dependency or upstream implementation code was added. The method
  improves synthetic brightness/contrast invariance and small-target score
  dilution tests, but does not solve rotation, occlusion, aliasing or strobe semantics.
- Exposed the experimental method in native settings and shared MCP
  `configure_tracking`; omitted arguments preserve the session's existing method.
  Scores/separation remain method-specific heuristics, not probabilities.
- Retained immutable job settings/algorithm and per-observation method provenance.
  Later settings changes cannot relabel previous observations. Provenance CSV
  appends `tracking_method`; MCP rows add nullable `matching_method`. Unknown
  legacy methods remain unspecified.
- Added version-3 project protection for normalized observations/settings,
  including mixed-method histories. Earlier project versions retain SSD defaults.
- Added a repeatable full-sample audit harness, scientific regression tests and
  an optional local upstream tail-frame test. Archives are user-supplied and are
  not redistributed by the test suite.

## Remaining issues and next actions

1. **Tracking robustness:** fixed appearance and local point patches remain weak
   against rotations, changing backgrounds, comb-like/blurred images and uniform
   bodies. Prioritize trusted keyframes, bounded motion prediction, alternate
   evidence and drift controls against annotated held-out clips. Retain manual
   correction; do not merely loosen thresholds until a job completes.
2. **Clip ranges:** persisted frame-accurate in/out boundaries and trimmed-video
   export remain absent. This run used explicit seeds, not trimming. Implement
   this before the next round of multi-trial analysis; edited clips should not
   silently concatenate separate experiments.
3. **Calibration:** samples remain px / px/s / px/s² where a scale was not verified.
   Do not apply the DJI ruler's 1 m length to another video's unlabeled striped
   pole. Prefer verified full-span graduations with visible endpoints and retained
   evidence. On-screen labels can support proposals, but reference plane and
   endpoint identity still need checking. No guessed 20 cm calibration was reused.
4. **Strobe/time semantics:** an old strobe image remains a valid image match but
   does not represent the object's new physical position. Add a separate strobe
   observation/time-reference workflow, or require explicit manual interpretation.
   Encoded presentation timestamps alone do not prove physical sampling intervals
   in an edited/composite teaching video.
5. **Feature coverage:** all 66 spectroscopy videos open, but intensity/wavelength
   extraction and spectral calibration are not implemented. `MassOnIncline.jpg`
   fails with a retained `media_error` (“No readable video frames were found”);
   still-image/image-sequence analysis requires a distinct media adapter.
6. **Persistent review/history:** point method provenance now survives reopening,
   but complete per-run settings history and pending review evidence are still
   sidecar records rather than fully serialized project state.

## Verification limits

`zsh scripts/build-app.sh` succeeded. The final release suite passed **36 XCTest
cases and 22 Swift Testing definitions**, with media fixtures and the supplied
tail-frame sample present; no skips in this run. Without the user archives, the
upstream tail-frame test explicitly skips. Existing packaged MCP scripts passed
18 base checks and 17 motion/export checks. Additional sample workflows verified
the new method, per-point provenance, frozen job settings and version-3 reopening.

Open Computer Use captured native saved-state accessibility evidence. Subsequent
attempts to bind the settings dialog failed with native `NSPOSIXErrorDomain` code
2, so the new method picker's runtime appearance and pixel-level layout were not
verified. Screen capture was not used as a substitute for source-frame evidence.
No UI-latency percentile, pressure/thermal measurement, sustained throughput or
parallel-decoder improvement is claimed by this correctness-focused batch.

A separate bounded release smoke comparison checked the **unchanged SSD path**
before/after the feature, using identical 72-frame synthetic ball-toss input,
fresh private apps and two alternating repetitions per workload. No build ran
during these timings. Source stamps, origins and coordinates agreed (coordinates
within 1e-6). The single active tracking slot was unchanged.

| Submitted videos/jobs | Before median automatic fps | Current median automatic fps |
| ---: | ---: | ---: |
| 1 | 76.50 | 77.16 |
| 2 | 82.38 | 81.36 |
| 4 | 83.68 | 83.10 |
| 8 | 84.06 | 83.98 |

Median differences ranged approximately -1.24% to +0.86%; two short repetitions
do not establish a statistically reliable speed difference. Current sampled
peak RSS across these trials was about 221 MiB. Median per-trial status-call p95
ranged 16.0–20.7 ms; it is not a rendered-input latency measurement. These are
submitted/queued workloads, not simultaneous decoders, and do not benchmark
ZNCC throughput. Raw results and fixture identity:
`build/upstream-results/legacy-performance/{raw,summary}.json`.

The new matcher stays experimental. The sample set was used for diagnosis and
tuning; it is not a held-out acceptance corpus, and no full-set accuracy pass is
claimed.
