# Interface inspection and prototype review

Date: 2026-11-13. Scope: initial macOS UI, not an autotracking release.

## Reference evidence

- Inspected https://opensourcephysics.github.io/tracker-website/ and
  https://opensourcephysics.github.io/tracker-website/help/interface.html.
- Viewed the documentation's `help_images/interface.gif` screenshot. Observed a
  large video view on the left, plot and table panes on the right, a compact
  analysis toolbar, playback controls, and video tabs.
- Inference/design choice: translate that pane structure into native SwiftUI,
  AppKit, AVKit, and Charts rather than reproduce Swing styling or every control.
- No binary was examined and no upstream implementation/assets were copied.
  REA binary/runtime providers were unnecessary for this documentation/screenshot
  inspection. This does not establish parity with the current shipped Tracker.

## Implementation

The Swift package has no external dependencies. A release build is packaged and
ad-hoc signed as `build/Tracker.app` by `scripts/build-app.sh`. The minimum macOS
deployment setting is 14; runtime inspection used the available macOS 27.2 host.

The interface includes native video import, tabs, transport and seeking, point
tracks, mouse marking and numeric position entry, two-endpoint calibration with
numeric alternatives, coordinate axes, x/y position plots, a measurement table,
point removal, and CSV export. Inactive tabs pause playback. No autosave exists;
export measurements before closing. Autotracker availability is explicitly labeled.

## Observed validation

- Release build completed and the application launched.
- `swift test`: 2 tests passed, covering letterbox coordinate mapping and invalid
  calibration geometry. These are not full tracking/scientific validation.
- Opened a synthetic 960 × 540, 30 fps, five-second silent MOV through the native
  chooser. The fixture is a static blue square on a grey grid, not experimental
  data and not an application background asset.
- Play advanced time; frame stepping moved to 0.033333333 s; returning to start
  restored 0 s. A mouse mark was observed through the first CUA inspection.
- Open Computer Use completed numeric marking using keyboard input: source
  (480,270) at t=0 appeared in the table and position plot.
- CSV export was exercised through the native save panel. The actual file held
  `time_s,x_px,y_px,source` and `0.0,480.0,270.0,manual`.
- Numeric calibration from (0,0) to (100,0) with known distance 1 m yielded
  100 px/m. Numeric marking at (480,270) then yielded (4.80,2.70) m.
- x/y plot switching, axes toggle, point removal, scale reset, tab selection, and
  closing an invalid-media tab changed the visible state as expected.
- Two valid videos and an invalid `.mov` were loaded together. Invalid media
  showed a readable error and disabled analysis/transport controls; closing its
  tab restored a valid video without crashing.
- Screenshots were inspected in the user's dark system appearance. The video
  stayed the focal point. Replaced segmented selectors with explicit native
  buttons for more reliable accessibility actions; fixed the crowded plot label,
  empty-view sizing, and constrained data-column widths.

## Anti Slop delivery review

Mode: during, explicitly selected by the user. Direction and decision reasons are
in `DESIGN.md`. The following PASS entries are limited to the stated evidence.

- R-01/R-04/R-06/R-07/R-08/R-09/R-10/R-12/R-13/R-14/R-19/R-22 PASS:
  source review shows native panels and operation-specific SF Symbols; no decorative
  gradients, glow, cards, typography effects, badges, or entrance animations.
  The fixture grid belongs to test video content, not the UI.
- R-02/R-15/R-16/R-17/R-18/R-23/R-24/R-28/R-36/R-38 PASS: visible copy names
  operations and actual state; no invented statistics, identities, logos, testimonials,
  empty navigation destinations, or unsupported performance claims.
- R-26/R-27 PASS for prototype scope: controls have actions, unavailable export and
  transport states are disabled; loading, empty, invalid-media, and export-error
  paths exist. Core live action checks are recorded above.
- R-31/R-37 and liveliness PASS: explicit ENERGY 1 / RHYTHM 2 / MOTION 1 direction;
  reference-derived panes, a dominant source image, adjacent coordinate/time
  readouts, and meaningful accent usage are documented and visible.
- R-33 PASS: features were written directly in Swift source using patches, not
  generated through source-rewriting helper scripts.
- R-34 PASS for scope: no separate theme toggle or forced dark theme; surfaces and
  native controls follow system appearance. Both themes need broader runtime QA.
- R-35 PASS for build/launch and the recorded functional checks; the complete
  production accessibility/theme/resize matrix has not been certified.
- R-39/R-40 PASS for native control semantics and media sizing: labeled native
  controls, visible keyboard input focus, video aspect fitting, and no decorative
  animation or third-party assets. Numeric input provides alternatives to spatial
  marking/calibration. This is not a complete accessibility-conformance claim.
- R-25 checked pairings: white on #171717 = 17.93:1; #a3a3a3 on #282728 = 5.90:1;
  #a3a3a3 on #333333 = 5.01:1; #222222 on white = 15.91:1. Over-video axis labels
  have solid black backing and crosshairs have black/white strokes. Native system
  colors and all appearance combinations still need formal accessibility QA.
- Craftsmanship review: every pane supports the analysis workflow; no landing-page
  sections, fabricated measurements, or concealed implementation claims are present.

## Remaining verification limits

The full R-03/R-25/R-32/R-35 accessibility, window-size, focus-order, and theme
matrix remains unverified. Do not describe this prototype as fully accessibility
certified or production-ready. Some CUA calls failed with ScreenCaptureKit
-3811/-3812 errors; Open Computer Use allowed inspection and most remaining
functional checks. Background coordinate clicks were unreliable; use numeric
entry for reproducible automation. The tool surface exposed no audio-interpretation
operation. ScreenCaptureKit's audio/video capture error is not evidence of audio
understanding or of a video-decoder failure in this application.

Autotracking, MCP integration, save/reopen, large-data behavior, long-running
concurrency, and performance targets remain future work. These were deliberately
excluded from the initial interface request.
