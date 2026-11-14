# Tracker for macOS: purpose and architecture

Status: native Swift analysis baseline with source-frame marking, versioned project
save/reopen, undo/redo, fixed-template autotracking, uncertain-match review, and
bounded job admission. Initial synthetic release benchmarks are recorded in
`docs/tracking-baseline-2026-11-14.md`. Adaptive tracking, dynamic parallel
scheduling remain future work; real-footage performance is unvalidated. A local
Swift MCP bridge now controls the shared application commands; connection and
verification details are recorded in `docs/mcp.md`.

## Purpose

Build a native macOS video-analysis application for Apple Silicon Macs that
preserves Tracker's familiar workflow while improving responsiveness, concurrent
autotracking, resource efficiency, and agent control through MCP.

The primary outcome is accurate, inspectable motion data produced through a smooth
interactive workflow. Language choice is subordinate to that outcome.

## Product decisions

- macOS on M-series hardware is the required platform. Windows and Linux are
  optional future possibilities, not current delivery requirements.
- Start with Swift, SwiftUI/AppKit, and AVFoundation. Keep scientific algorithms
  and job execution separate from the interface. Rust is an optional optimization
  or portability decision that requires evidence; it is not a mandatory backend.
- Support multiple videos autotracking concurrently inside one application.
  Separate OS processes are not required for each video. Additional app processes
  may be investigated later; benchmark claims must identify which configuration
  was tested.
- Do not impose an arbitrary product limit on open sessions or requested jobs.
  Schedule active work according to real hardware capacity and memory pressure.
- Make Codex the first MCP client used for integration and acceptance testing.
- Defer Tracker `.trk`/`.trz` compatibility. A reliable internal save format is
  still required for the first usable release.

## Acceptance criteria

### Performance and resource use

- Keep decoding, tracking, fitting, file I/O, and export off the UI execution path.
- Preserve responsive input, scrubbing, overlays, and cancellation during analysis.
- Bound decoded-frame memory, pending frame queues, and preview/result delivery.
- Reuse valid computation and avoid unnecessary decoding, frame copies, analysis,
  and UI refreshes. Recompute only affected dependencies after an edit.
- Measure release builds on recorded M-series configurations and representative
  videos. Record tracking throughput, input latency, render stalls, peak memory,
  CPU usage, memory pressure, and sustained performance under thermal load.
- Initial engineering targets, pending baseline measurement: p95 input feedback
  within 100 ms, cancellation acknowledgement within 250 ms, and 60 Hz visual
  interaction where the display supports it. These are targets, not guarantees.
  Video playback follows source timing; it need not run at 60 frames per second.
- Measure cancellation completion and resource release separately from its UI
  acknowledgement. A busy decoder must not make the interface appear frozen.

Low resource use means avoiding waste for a given workload, not keeping CPU usage
low while maximum throughput is requested. The scheduler may use spare capacity
aggressively while reserving enough capacity for interaction and system health.

### Familiar interface and clear inputs/outputs

Retain Tracker's recognizable arrangement: video tabs, central video with editable
overlays, analysis toolbar, playback and frame controls, calibration and axes,
track controls, an autotracker inspector, and resizable plots and tables. Adapt
these to macOS conventions without requiring users to relearn the analysis flow.

Every session must show its source video, clip range, calibration and units,
selected track, tracking parameters, job state, uncertain results, and export
status. Tracking on inactive tabs must remain observable. Save/reopen, undo,
manual correction, and error recovery are part of the core workflow.

### Concurrent analysis without an arbitrary session cap

Users may open sessions and submit jobs without a fixed numeric product cap.
This does not imply unlimited simultaneous decoding, memory allocation, or CPU
execution. Jobs may be queued or temporarily throttled when resources are busy;
the interface must explain the state and allow prioritization and cancellation.

Benchmark 1, 2, and 4 active videos as initial comparison points, then increase
load until resource constraints become visible. These counts are test points,
not supported-session limits. Report both aggregate throughput and per-job
fairness, as well as foreground responsiveness. Stress tests must show controlled
backpressure rather than runaway allocation or starvation.

### Intelligent, scientifically accountable autotracking

- Establish a baseline of template matching, subpixel localization, predictive
  search, keyframes, controlled template evolution, and drift limiting.
- Evaluate candidate ambiguity, local match quality, appearance changes, and
  motion consistency before automatically accepting a position.
- For uncertain frames, perform a bounded secondary assessment: compare alternate
  candidates, reconsider the search region, and check stable keyframe references.
  Additional work must respect cancellation and resource budgets.
- Pause for review or follow the explicitly selected skip/continue policy when
  ambiguity remains. Never silently turn a motion prediction into an observation.
- Avoid updating adaptive templates from untrusted matches. Preserve enough state
  to recover or rerun from a keyframe after correction.
- Expose candidate positions, scores, rejection reasons, and supporting overlays
  so a user or agent can assess low-confidence frames.
- Treat scores as algorithm-specific evidence, not probabilities unless validated
  against labeled data. Detection of a particular object is not guaranteed under
  arbitrary noise or occlusion.
- Test against manually annotated clips containing noise, blur, distractors,
  lighting changes, occlusion, and abrupt motion. Report localization error,
  false acceptance, loss, recovery, and sensitivity to chosen thresholds.

### Agent operation through MCP

Codex must be able to import a video, inspect session state, view selected frames,
set clip ranges and calibration, define a tracking target and search region,
start/pause/cancel jobs, inspect uncertain matches, correct results, and export.

Video rendering remains native inside the application. MCP exposes commands,
structured state, and requested frame images with optional overlays; it is not
the native video renderer and must not continuously ship full-resolution video
to a model. Agent visual understanding depends on the connected client's image
capabilities. The interface must show agent-triggered changes and live progress.

Calibration requires a known distance, unit, or other explicit physical scale
reference. An agent may select endpoints or propose a calibration from supplied
evidence, but must not invent physical dimensions from pixels alone.

## Component boundaries

```text
SwiftUI/AppKit interface             Codex MCP client
          |                               |
          |                        local MCP adapter
          |                               |
          +----- Application commands ----+
                          |
                 Session/project store
                          |
                 Resource-aware scheduler
                    /               \
         AVFoundation media       Scientific engine
             adapters          tracking / calibration /
                    \           numerical analysis
                     +-- results and progress --+
                          |
               Native overlays, plots, exports
```

### Native application

Own windows, tabs, input, file access, playback presentation, and visual overlays.
Use main-actor isolation for UI state, not for CPU-heavy work. Coalesce progress
updates and prioritize the selected session. Plot and table refresh rates must
not determine tracking speed.

### Application commands and session store

Provide one command layer used by both the interface and MCP. Validate operations
and update authoritative session state here, rather than maintaining independent
agent and UI models. Identify sessions, tracks, jobs, and revisions explicitly.

Represent a job with observable queued, running, paused, awaiting-review,
completed, failed, and cancelled states. Define transitions and what pause/resume
means before exposing these operations. Snapshot settings for each run and attach
its revision to results. Reject stale edits and discard late results after a
session closes or its inputs change. Preserve user edits through undoable commands.

### Media pipeline

Use AVFoundation for source inspection, timestamp-aware frame access, and native
playback. Preserve orientation, source timestamps, and pixel-coordinate mapping.
Handle variable frame rate explicitly; do not assume time equals frame number
divided by nominal FPS. Analysis must process requested frames even when previews
are dropped to reduce rendering work.

Use small reusable buffer pools and bounded queues. Define frame ownership and
pixel format before attempting copy reduction. Keep a bounded reusable frame
cache and evict it under pressure; do not retain whole decoded videos in RAM.

### Scientific engine

Accept frame buffers, timestamps, calibration, and tracking parameters; return
positions, evidence, uncertainty indicators, and updated tracking state. Keep
domain calculations independent of SwiftUI and AppKit. Start with CPU execution;
add Accelerate, Metal, or Rust only when representative profiling justifies the
change, including transfer/setup overhead and numerical validation.

Adaptive tracking generally has sequential dependencies within a track. Exploit
independence across videos first, and parallelize search work only where it
preserves results. Keep calibration, filtering, differentiation, and fitting
testable independently. Mark predictions, interpolations, and manual corrections
distinctly from automatically accepted image measurements.

### Resource-aware scheduler

Current baseline: jobs queue behind one shared tracking execution slot. There is
no session cap. This is a measured starting budget, not the final adaptive scheduler
or a claim of simultaneous CPU processing. Increase admission after representative
video and interactive-latency measurements justify it.

Own a shared execution budget across sessions, rather than allocating an
unrestricted worker pool per video. Coordinate decoder admission, CPU tasks,
frame-buffer budgets, and preview delivery. Use fairness with foreground priority
so background sessions continue making progress.

Adapt active work using measured throughput, interaction latency, memory pressure,
and thermal state. Avoid oscillating concurrency settings. Queue or suspend work
under pressure, release eligible caches, and communicate why. Do not create an
unbounded number of tasks simply because sessions have no fixed product cap.

### Persistence

Use a versioned project schema containing source references, timestamps, clip
settings, units, calibration, track settings, accepted/rejected results, manual
corrections, and keyframe state needed for reproducibility or resume. Store source
videos separately by default. Persist large results incrementally when needed;
retain compact session metadata rather than every frame in memory.

Use atomic saves/checkpoints and explicit source relinking. Changing an axis or
scale should recompute derived coordinates without rerunning image matching when
the matching inputs are unchanged. Changing a keyframe or adaptive template must
invalidate the affected downstream tracking state. Preserve run settings and
algorithm version so exported data can be traced to its generation method.

### Local MCP adapter

Prototype a local stdio MCP bridge that connects to the running app through a
restricted local IPC channel. Keep tool definitions and protocol handling outside
the scientific engine. No publicly listening service is required.

Expose compact operations such as import video, inspect session, get frame,
set calibration, configure track, start job, get job status, review candidates,
apply correction, cancel job, and export results. Final tool names are deferred.

- Return job identifiers promptly for expensive operations and provide status
  polling; use notifications where the selected SDK and client support them.
- Include session/track identifiers, coordinate space, frame timestamp, and
  revision in relevant inputs and results.
- Make retry behavior explicit: use request identifiers for non-idempotent
  operations so reconnects do not duplicate imports or tracking jobs.
- Offer scaled frame snapshots and requested detail crops, with explicit mappings
  back to original pixels. Do not make an agent infer coordinates from UI zoom.
- Record agent actions, route them through the same validation and undo model as
  human actions, and allow the user to stop agent-started jobs from the interface.
- Validate file access and output destinations through the app's access model.
  Handle reconnects, app shutdown, stale revisions, and export collisions explicitly.

## Delivery plan

1. Record hardware, macOS baseline, representative clips, annotations, and Tracker
   comparison measurements. Review upstream source and licensing before reuse.
2. Build a native import/playback/frame-stepping prototype with overlays and
   instrumentation; confirm timestamp and coordinate correctness.
3. Add session state and shared commands, calibration, manual marking, save/reopen,
   and CSV export. Establish scientific correctness tests.
4. Implement baseline autotracking and uncertain-frame review on one video;
   compare against labeled frames and upstream behavior.
5. Add shared scheduling and stress concurrent sessions beyond 1/2/4 test points;
   measure latency, resource stability, fairness, and sustained throughput.
6. Add the MCP bridge and validate an end-to-end Codex workflow, including frame
   inspection, supplied-scale calibration, tracking, correction, and export.
7. Complete familiar plot/table panes, recovery paths, and release-build acceptance
   checks. Optimize demonstrated bottlenecks and publish measured limitations.

## Validation and unresolved details

Test malformed media, missing source files, orientation changes, variable frame
rate, ambiguous targets, occlusion, session closure during tracking, stale agent
commands, repeated requests, interrupted saves, and memory/thermal pressure.
Scientific changes require known-trajectory and annotated-image validation;
performance changes require representative release-build comparisons.

The precise minimum macOS version, benchmark Mac/RAM, normal video sizes, numerical
error tolerances, and final latency/resource budgets remain to be established.
Do not claim universal lag-free operation, unlimited physical capacity, or improved
tracking accuracy before measurements support those claims.

## Reference behavior and protocols

- [Tracker interface](https://opensourcephysics.github.io/tracker-website/help/interface.html)
- [Tracker autotracker](https://opensourcephysics.github.io/tracker-website/help/autotracker.html)
- [Tracker source and license](https://github.com/OpenSourcePhysics/tracker)
- [Apple AVFoundation](https://developer.apple.com/av-foundation/)
- [MCP tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [MCP resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
