# Tracker feature backlog

Last checked: 15 November 2026.

This records documented Java Tracker features that the Swift port lacks or only
partly implements. It is a working checklist for delivering one feature at a
time, not a claim that either application automatically produces accurate results
on every video. The order below is proposed, with clip controls and tracking
reliability first. IDs stay stable when priorities change.

## How to use this checklist

Select one ID, define its implementation scope, implement it through the shared
native/MCP command layer, and record its verification here. Check an item only
when its acceptance criteria pass. If part of it remains unfinished, leave it
unchecked and describe the remaining work. Update the date when rechecking status.

- Completed first item: **F01 — clip ranges and sampling**; see [delivery evidence](clips.md).
- Suggested next item: **F03 — editable tracking keyframes**, followed by F04/F05
  against annotated real clips. F02 remains available for experiments requiring
  physical timing different from the encoded video timeline.
- Completion evidence should include tests, relevant sample clips, native UI/MCP
  checks, and any limitations. Report skipped or unavailable checks explicitly.
- Follow [ARCHITECTURE.md](../ARCHITECTURE.md) and [development instructions](../AGENTS.md).
  Target Swift on Apple Silicon macOS; additional backends require measured reasons.

## Existing foundation

The port already implements manual point marking/correction, fixed-template SSD
tracking, experimental normalized correlation, uncertainty review, x/y, velocity,
speed and acceleration, static scale/axes, basic plots/tables, CSV export,
versioned JSON save/reopen, undo/redo, and local Codex MCP commands.

These are partial foundations for feature parity. In particular, normalized
correlation did not establish an overall unattended-success improvement on the
supplied samples. Derivative smoothing is not position filtering; numeric
velocity components are not graphical vector tools; seeking is not clipping.

Evidence: [DJI stress test](dji-stress-2026-11-15.md),
[supplied-sample evaluation](upstream-samples-2026-11-15.md),
[motion analysis](motion-analysis.md), and [MCP commands](mcp.md).

## 1. Trial selection and tracking reliability

- [x] **F01 — Clip ranges and sampling.** **Implemented, 15 November 2026.** Persisted start/end
  source frames and an every-nth-frame step size, following Java Tracker's
  [clip workflow][videos]. Done when native controls and MCP agree on boundaries;
  playback, tracking, plots, derivatives and exports consistently respect them;
  undo/reopen preserve them; and source timestamps remain available. Selecting
  an analysis range must not delete source footage or observations outside it.
  Native Apply and MCP agree; release tests cover both playback paths, selection,
  derivatives, history and persistence. Packaged MCP and bounded DJI checks pass.
  Pixel-level UI review and the native outside-range preview/reset click sequence
  remain unverified because computer-use capture/runtime failed. Performance
  smoke checks show a small throughput cost, not a speed improvement.
  [Semantics, evidence and limits](clips.md).

- [ ] **F02 — Physical time calibration.** **Missing.** Java Tracker allows a
  time origin, true capture rate and frame interval for slow-motion/time-lapse
  footage. Our port currently uses encoded source timestamps. Done when an
  explicit physical-time mapping is validated, reversible, saved and exported
  alongside source time, including correct derivative units. Never infer physical
  timing from playback rate or a strobe composite alone. [Reference][videos]

- [ ] **F03 — Editable tracking keyframes.** **Partial: manual reseeding only.**
  Support multiple explicit reference frames, reference inspection and changes
  across a run. Done when corrections can establish a new trusted reference,
  previous references remain inspectable, and save/reopen and MCP preserve their
  identities and effects. [Reference][autotracker]

- [ ] **F04 — Adaptive templates and tethering.** **Missing.** Java Tracker can
  evolve appearance and tether it to the original reference to limit drift.
  Done when adaptation is bounded, uses supported observations, pauses on
  ambiguity, and demonstrates an accuracy benefit on held-out clips without
  increasing known false acceptance. Longer runs alone do not satisfy this item.
  [Reference][autotracker]

- [ ] **F05 — Predictive search positioning.** **Missing.** Add explicit
  look-ahead, follow-along and fixed search modes. Done when prediction uses
  actual observation times, handles gaps and abrupt changes, respects resource
  bounds, and only guides image searches; predicted locations must not be
  recorded as observations. [Reference][autotracker]

- [ ] **F06 — Template shapes, target offsets and search constraints.**
  **Partial: centred square templates/local search.** Support elliptical and
  rectangular masks, a tracked point offset from the matched feature, editable
  search regions, and optional one-dimensional axis-constrained search. Done
  when source-pixel mappings, overlays, saved settings and MCP commands agree.
  [Reference][autotracker]

## 2. Media preparation and export

- [ ] **F07 — Configurable video preprocessing.** **Missing.** Java Tracker
  provides deinterlacing, brightness/contrast, background subtraction and
  frame sum/averaging. Done when a documented filter chain preserves originals,
  records parameters and frame mappings, remains cancellable, and is evaluated
  for measurement effects. Deinterlacing must state field/timing assumptions and
  lost spatial resolution. [Reference][filters]

- [ ] **F08 — Perspective and lens-distortion correction.** **Missing.**
  Current orientation handling is not geometric calibration. Done when explicit
  perspective/radial transforms can be inspected and saved, and source versus
  corrected coordinates remain distinguishable throughout marking, calibration
  and export. Validate against known geometry. [Reference][filters]

- [ ] **F09 — Still-image and image-sequence analysis.** **Missing.** Support
  single JPG/PNG inputs and ordered/zipped image stacks, with explicit timing for
  sequences. The supplied JPEG failed video import; animated-GIF parity also
  needs separate verification. Done when ordering, missing frames, dimensions,
  timestamps and reopening are validated without loading an entire large stack
  into RAM. [Reference][videos]

- [ ] **F10 — Trimmed and annotated video export.** **Missing.** Java Tracker
  exports clips, filters and tracking overlays as video or image sequences.
  Done when a separate output copy has verified boundary frames, dimensions,
  timestamps and optional overlays, with explicit overwrite handling, progress,
  cancellation and source-to-export mapping. Depends on F01. [Reference][videos]

## 3. Coordinates, mechanics and measurement tools

- [ ] **F11 — Moving reference frames and time-varying calibration.**
  **Partial: one static origin, angle and scale per session.** Support explicit
  frame-dependent transforms and reference frames following a point mass or
  centre of mass. Done when relative positions/derivatives are validated against
  known trajectories, source observations remain unchanged, and transforms
  survive edits and reopening. [Reference][coordinates]

- [ ] **F12 — Mass properties and centre-of-mass tracks.** **Missing.** Add
  explicit masses/units, derived centre-of-mass tracks and mass-dependent
  quantities such as momentum and net force. Done when units and dependencies
  are validated and mass/position edits correctly update derived results without
  changing observed positions. [References][features] and [point-mass help][pointmass]

- [ ] **F13 — Graphical vectors and vector sums.** **Partial: numeric velocity
  and acceleration already exist.** Add vector overlays and independent vector
  tools, including tip-to-tail sums. Done when arrow components, scale, origin,
  reference frame and numerical results agree and remain distinguishable from
  observed point positions. [Reference][pointmass]

- [ ] **F14 — Tape measure, protractor and circle fitter.** **Missing.**
  These are separate instruments from the current two-endpoint scale tool.
  Done when known distance/angle/circle fixtures pass, units and source pixels
  are explicit, and instruments can be edited, undone, saved and controlled
  through shared commands. [Reference][tracks]

## 4. Data analysis and visualization

- [ ] **F15 — General curve fitting.** **Missing.** Add fitting to all or a
  selected subset of observations, with explicit functions, parameters and
  fit-quality evidence. Done when known synthetic fits and failure cases pass,
  fit range/method/results are retained, and fitted predictions are separate
  from observed data. Existing local quadratic derivative calculations do not
  provide this workflow. [Reference][plots]

- [ ] **F16 — Custom calculated columns.** **Missing.** Java Tracker's Data
  Builder supports expressions using parameters and existing columns. Done when
  expression validation, units, missing values, dependency updates and project
  persistence work; raw observations remain unchanged. [Reference][plots]

- [ ] **F17 — Multiple and comparison plots.** **Partial: one selected-track
  plot.** Add stacked plots, synchronized axes and multi-track comparisons.
  Done when selections, axes, legends and units are unambiguous and rendering
  remains responsive on large tracks without dropping exported rows.
  [Reference][plots]

- [ ] **F18 — Position-filtered copies.** **Partial: derivative smoothing only.**
  Java Tracker offers separate moving-average, Savitzky–Golay and Butterworth
  filtered clones. Done when raw positions are preserved, filter parameters and
  edge behavior are recorded, and known signals demonstrate the smoothing and
  distortion introduced. [Reference][pointmass]

- [ ] **F19 — Kinematic/dynamic models and model overlays.** **Missing.**
  Java Tracker's Model Builder supports position equations, force-based numerical
  models and synchronized model/video comparison. Done when model parameters,
  solver settings, units and initial conditions are inspectable and saved, and
  model predictions are visibly distinct from observations. [Reference][models]

## 5. Optical analysis

- [ ] **F20 — Line profiles and spectroscopy.** **Missing; video import works.**
  Add brightness/RGB sampling along a line with adjustable spread, then explicit
  wavelength calibration and relative-intensity plots/exports. Done when known
  image fixtures verify sampling and averaging, reference wavelengths and source
  pixels are retained, and camera intensity limitations are stated. Opening a
  spectroscopy video alone is not completion. [Line-profile help][lineprofile]
  and [spectroscopy demonstration][spectroscopy]

- [ ] **F21 — Time-dependent RGB regions.** **Missing.** Measure image-region
  colour/brightness over time. Done when region geometry, averaging, timestamps,
  edits and exported channel values are validated and saved; these values must
  not be presented as calibrated physical intensity without a response reference.
  [Reference][features]

## 6. Compatibility, collections and presentation

- [ ] **F22 — Original TRK/TRZ compatibility.** **Deferred.** Our port uses
  `.tracker.json` referencing external footage. Java Tracker supports `.trk`
  tabs and self-contained `.trz` packages with media/documentation. Done when an
  explicit compatibility subset has fixtures, missing-media handling, preserved
  supported scientific state and clear errors for unsupported content. Review
  upstream licensing before copying code or assets. [Reference][gettingstarted]

- [ ] **F23 — Digital Library Browser.** **Missing.** Browse/search local and
  online collections of videos and projects. Done when supported resources,
  metadata, caching, downloads and failure states are clear; importing a
  collection must not silently change scientific state. [Reference][library]

- [ ] **F24 — Annotations, notes and supporting documents.** **Partial: track
  marks and scientific overlays only.** Add editable drawings/labels and project
  notes/support documents. Done when these remain separate from measurements,
  survive reopening, and appear in exported media only when requested.
  [Reference][features]

## Project-specific requirements carried forward

These are our acceptance requirements, not claims that Java Tracker already
automates them or achieves unlimited performance.

- [ ] **P01 — Verified full-ruler calibration workflow.** Prefer a confirmed
  full-span reference with visible graduation endpoints. Preserve the reference
  frame, endpoint evidence and confirmation status. Never equate metal edges
  with graduation endpoints without evidence. If obscured, ask for manual
  identification. Keep the BallDrop/BallToss samples in pixels as requested;
  do not reuse the DJI ruler's length for them.
- [ ] **P02 — Resource-aware concurrent execution.** The current scheduler has
  one active tracking slot. Evaluate wider admission only with release benchmarks,
  bounded memory/queues, cancellation and interaction measurements at 1/2/4 jobs
  and beyond. No fixed product session cap or zero-lag promise.
- [ ] **P03 — Persistent review and run history.** Per-point method provenance
  and app-lifetime job settings already exist. Pending evidence, trusted
  references and complete run settings/history still need project persistence.
- [ ] **P04 — Held-out real-footage acceptance.** Maintain annotated validation
  segments separate from tuning clips. Report localization error, false
  acceptance, loss/recovery, interventions, throughput and resource behavior.
  A completed job on a persistent strobe image is not a physical tracking pass.

## Sources and verification records

The references establish documented upstream capabilities, not a runtime test
of every Java Tracker feature. Current port status comes from source inspection
and the linked local test reports. Future work should recheck both before
changing a status.

[autotracker]: https://opensourcephysics.github.io/tracker-website/help/autotracker.html
[videos]: https://opensourcephysics.github.io/tracker-website/help/videos.html
[filters]: https://opensourcephysics.github.io/tracker-website/help/video_filters.html
[coordinates]: https://opensourcephysics.github.io/tracker-website/help/coords.html
[pointmass]: https://opensourcephysics.github.io/tracker-website/help/pointmass.html
[tracks]: https://opensourcephysics.github.io/tracker-website/help/tracks.html
[plots]: https://opensourcephysics.github.io/tracker-website/help/plotview.html
[models]: https://opensourcephysics.github.io/tracker-website/help/particles.html
[lineprofile]: https://opensourcephysics.github.io/tracker-website/help/lineprofile.html
[spectroscopy]: https://opensourcephysics.github.io/tracker-website/download/AAPT_spectroscopy_poster.pdf
[gettingstarted]: https://opensourcephysics.github.io/tracker-website/help/gettingstarted.html
[library]: https://opensourcephysics.github.io/tracker-website/help/library_browser.html
[features]: https://opensourcephysics.github.io/tracker-website/
