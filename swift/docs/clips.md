# Source-frame clip selection

F01 selects an analysis trial without modifying the source video. Open **Clip
Settings…** below the viewer, inspect source frames, choose start/end bounds and
step size, then Apply. Set Start/Set End copy the displayed preview frame index.
Reset to Full Video restores full-source analysis. Undo/Redo also restore selection.

## Scientific contract

- Frame indices are zero-based, orientation-independent, inclusive source bounds.
  Start 3, end 12, step 2 selects frames 3, 5, 7, 9, 11. The summary reports the
  actual last sampled frame; the requested end bound remains 12.
- The selection carries original rational start/end/last-sample timestamps and
  source frame count. Times, velocities and accelerations use original source
  time. No physical time origin or capture-rate calibration is inferred.
- Tracking seeds and manual observations must be on selected steps. Native seek
  clamps to the clip and snaps down to its sampling grid. The settings preview
  and MCP `get_frame` inspect the full source independently of the viewer.
- Matching stops at the last selected frame. Skipped frames bypass orientation
  conversion, grayscale conversion and template matching. Codec dependencies
  can still require decoding intervening compressed frames.
- All observations remain stored. Plots, tables, derived rows and CSV filter to
  selected source steps. Derivative neighborhoods use consecutive selected steps;
  a missing selected observation remains a gap. Values outside the selection
  never supply boundary derivatives.
- Native playback uses the bounded AVPlayer timeline for step 1. With sampling,
  one asynchronous preview task displays selected frames at their source times.
  Late display previews may be dropped; tracking never drops selected samples.
- Changing a clip stops playback and affected queued/running tracking, invalidates
  pending viewer reads and clears uncertain review. Existing accepted observations
  remain undoable. Job records retain the clip under which they ran.

## Persistence and automation

UI and MCP call the same `VideoSession.setClip` / `resetClip` validation and history.
MCP mutations require explicit session IDs, revisions and retry UUIDs. Tools return
retained operation IDs for viewer refresh; cancellation does not undo an already
accepted selection. Invalid or unchanged selections do not edit history/revision.

Non-default selections require project version 4. Versions 1–3 default to full
source, step 1. Reset canonicalizes selection to nil and permits version 2/3 saves
according to matching provenance. Saving always retains all source observations.
Relinking checks source dimensions, frame count and rational boundary stamps;
these checks do not establish complete file identity or detect every substituted
video with an identical timeline.

`get_session` returns `clip`; `get_measurements` reports selected `total`, all
`retained_total`, clip and derivative metadata. Standard/provenance CSV appends
clip start/end/step after tracking method, for 22 columns. Compact Excel CSV keeps
eight numerical columns. Saved projects carry clip metadata independently of CSV.

## Verification

Release tests cover rational/VFR timing, inclusive and unaligned bounds, extreme
integers, legacy/version-4 persistence, retained points, undo/redo, sampled
derivatives and real gaps, source-frame seeking, sampled playback, bounded
matching, and clip changes during queued/running/review states.

Packaged acceptance script: `python3 scripts/test-clip-mcp.py`. It uses generated
fixtures and writes only `build/audit/clips`. Set `TRACKER_CONTROL_DIRECTORY` to a
private test instance to avoid editing the user's open sessions. Detailed observed
results and UI/performance limits are recorded below.

### Observed delivery results — 15 November 2026

- `zsh scripts/build-app.sh` passed; ran an identical signed copy with a separate
  bundle ID/private socket to protect user sessions. Final release suite passed
  **43 XCTest cases and 27 Swift Testing tests**, with no fixture skips. It covers
  native AVPlayer stopping at the inclusive end frame as well as sampled playback.
  Test log: `/tmp/tracker-clip-tests-final2.log`. SDK language-mode warnings remain
  the documented Swift 6.4 / SDK 0.11.0 workaround, not strict Swift 6 validation.
- Packaged stdio checks passed: base 18, motion 17, clip 9 acceptance groups.
  Clip groups include cancellation/reconnect without rolling back the accepted
  range, retained job snapshots, stale pagination, source-boundary mismatch with
  a clear media error and no chooser, and regional Excel/Standard CSV output.
  Reports: `build/audit/clips/acceptance.json`, `build/mcp-e2e/report.json`,
  `build/clip-acceptance/test-motion-mcp.py.log`.
- Open Computer Use accessibility verified invalid text disabling Apply, committed
  fields producing five samples with last frame 11, native Apply producing the
  same clip in MCP, and Set Start/Set End copying the displayed preview frame.
  Evidence: `build/clip-acceptance/native-invalid.txt`,
  `native-valid-committed.txt`, `native-applied-final.txt`,
  `native-mcp-applied.json`, and `native-preview-boundaries.txt` in that directory.
  Direct AX writes to unfocused fields initially changed exposed field values
  without committing all bindings; focusing/tabbing and checking the computed
  summary resolved the Apply check. Field values alone were not counted as proof.
- The native outside-range preview sequence could not be completed reliably with
  automation. Subsequent Open Computer Use calls failed with NSPOSIX code 2;
  the alternate computer-use capture failed with ScreenCaptureKit −3811 after
  a long timeout. Native reset click, pixel-level layout/contrast, and sustained
  UI render latency remain unverified. Domain/native-session/MCP reset and
  independent full-source image inspection pass their automated checks.

### Bounded real-footage check

`scripts/test-clip-dji.py` imported the authorized 1,494,019,426-byte DJI HEVC file
in **1.092 s** in a fresh app session with warm filesystem caches. This opens and
indexes the source; it does not load 1.5 GB of decoded frames into RAM. No cold
disk/first-ever launch claim is made.

The saved trial selects source frames **2526–2586**, step 1, **84.2842–86.2862 s**
(61 possible samples). A previously visually selected seed at source pixel
(903, 318), SSD radius 12/search 64, thresholds .90/.015, stopped for review after
16 inspected frames in **0.342 s**. It retained one manual seed and 15 automatic
positions. The candidate at **84.8180667 s** remained unrecorded: competing-match
score .924859, separation .001060. These scores are not probabilities.

Version-4 save, pixel-unit CSV and reopen preserved the selection and observations.
No physical calibration was applied. This confirms range enforcement and safe
uncertainty handling; it does not validate those automatic positions against
ground truth or improve unattended real-object accuracy. Report and preview:
`build/audit/clips-dji/acceptance.json`, `release-preview.png` in that directory.

### Release regression smoke comparison

Apple M4, 16 GiB RAM, macOS 27.2.1, Swift 6.4.2. Compared the preserved pre-clip
release `build/controlled/upstream-bench-current/Tracker.app` with the delivered
build copied to `build/controlled/clip-current/Tracker.app`. Two repetitions per
variant/count, alternating order; 16 trials, 60 synthetic video runs. Each video
has 72 samples, 1920×1080, 24000/1001 fps. Full-source default SSD settings and
the existing **single active tracking slot** were retained: other requested jobs
queued, rather than executing eight matchers in parallel.

| Requested videos | Before median auto frames/s | Clip build median auto frames/s | Clip build MCP p95 ms | Clip build sampled peak RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| 1 | 73.46 | 71.54 | 14.43 | 220.2 |
| 2 | 78.86 | 76.38 | 14.25 | 222.4 |
| 4 | 79.90 | 78.29 | 12.75 | 203.9 |
| 8 | 81.29 | 79.33 | 12.81 | 207.9 |

Source timestamps and observation origins matched exactly; position/time doubles
matched within 1e-6 across all runs. The short comparison shows approximately
2–3% lower throughput, not an efficiency win. Two repetitions cannot isolate the
cause or establish sustained performance. Maximum sampled MCP reply was 34.46 ms.
RSS is periodic app-process sampling, excluding WindowServer/decoder services;
the nonmonotonic values are not evidence that more sessions need less memory.
A single system memory-pressure query reported 48% free memory; peak pressure
was not measured. Native input latency, render stalls, sustained memory growth
and cancellation latency were not profiled. Queued/running/review cancellation
behavior was functionally verified in release tests.

Raw paired outputs and summary: `build/controlled/clip-regression/raw.json` and
`summary.json`. Harness: `scripts/controlled-benchmark.py`. The existing P01
performance work remains open.

Physical time recalibration (F02), stronger matching (F03 onward), and exporting
a physically trimmed video copy (F10) remain separate backlog items.
