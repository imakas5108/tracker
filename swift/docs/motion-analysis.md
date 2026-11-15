# Motion analysis

The native workspace calculates position, velocity components, speed and
acceleration components from accepted source observations. SwiftUI, CSV and MCP
use the same coordinate conversion and numerical pipeline. No image matching is
rerun when changing axes, scale or derivative settings.

## Workflow and reference

Show **Axes** in the toolbar. Enter the origin's orientation-corrected source
pixel coordinates and the counterclockwise angle of the positive x-axis, then
choose **Apply Axes** or press Return. In Navigate mode, dragging the origin
handle moves it on release. **Reset** restores the bottom-left origin and zero
angle. Origin and angle are static for the entire video.

Choose a variable from either plot-axis label. The available variables are t, x,
y, vx, vy, v (speed), ax and ay. The table's **Columns** menu independently selects
visible quantities; time remains visible. The plot header's derivative menu
selects the numerical window. Changing plot variables or visible columns is view
state; axes, calibration and derivative settings are undoable scientific edits.

This follows Java Tracker's documented [axes toolbar](https://opensourcephysics.github.io/tracker-website/help/axes.html),
[plot-axis selectors](https://opensourcephysics.github.io/tracker-website/help/plotview.html)
and [table-column selection](https://opensourcephysics.github.io/tracker-website/help/tableview.html).
The official axes_fields.gif, plot_select_variable.gif and datatable_dialog.gif
were inspected as visual references. Upstream images and code are not bundled.
This implements the core workflow, not Java Tracker's full feature set or exact
numerical defaults. Java Tracker documents a different default acceleration
window in its [point-mass help](https://opensourcephysics.github.io/tracker-website/help/pointmass.html).

Selectable table columns require macOS 14.4; on earlier supported systems the
table retains t/x/y. Plot selection, calculations, CSV and MCP remain available.
That older-system fallback has not been device-tested.

## Coordinates and units

Original observations remain top-left source pixels. For origin (ox, oy), scale
s in pixels per selected unit and angle theta counterclockwise from horizontal:

```text
dx = (pixel_x - ox) / s
dy = (oy - pixel_y) / s
x = dx*cos(theta) + dy*sin(theta)
y = -dx*sin(theta) + dy*cos(theta)
```

Positive y is 90 degrees counterclockwise from positive x. An unset origin uses
(0, source height), preserving the original bottom-left convention. Calibration
still requires an explicit known distance in m, cm or mm. Uncalibrated results
are px, px/s and px/s². Setting an origin never supplies a physical scale.
Nonfinite axes, invalid settings, unsupported calibration units in project files,
and transforms exceeding finite numerical range are rejected.

## Derivatives

- Default: **three-point-quadratic-v1**, window 3, no smoothing. Fit a quadratic
  separately to each coordinate using the actual three source times. On a uniform
  timeline this is the centered first difference and centered second difference.
- Optional: **local-quadratic-v1**, windows 5, 7 or 9. Fit a local quadratic by
  least squares. These wider windows smooth derivatives; x/y and original pixels
  remain unchanged. Time is centered/scaled and the small fit is solved with QR.
- Exact rational frame stamps are authoritative when present, including older
  projects whose floating-point seconds were rounded. Legacy observations
  without a stamp use their stored seconds and source-timeline lookup.
- A window must contain consecutive selected clip steps. VFR intervals are supported;
  unrecorded selected steps break the window. There is no automatic interpolation,
  extrapolation, nominal-FPS timing or special treatment of unaccepted candidates.
  Intentional every-nth-frame sampling uses the original selected timestamps;
  outside-clip observations cannot fill derivative windows. See [clip controls](clips.md).
- The first/last half-window observations lack derivatives. Invalid intervals,
  missing source timing and ill-conditioned windows also produce unavailable
  derivatives, never fabricated zeros. Native cells show a missing-value mark
  with a reason on hover; MCP returns null plus derivative_unavailable_reason;
  CSV leaves the numerical fields empty and includes the reason.

Differentiation amplifies pixel noise. Smoothing trades temporal detail for lower
noise and does not provide a measurement-uncertainty estimate. Speed is the
magnitude of the calculated velocity, not an additional observed quantity.

## State, performance and output

Calculations run on a per-session calculator actor, with one cached track/revision
per session. Append-only updates reuse the unchanged prefix and recalculate the
tail plus its derivative neighbors. UI requests coalesce for 80 ms, cancel obsolete
work, and reject late results using a generation token. Valid rows remain visible
with an updating indicator during append delivery; changed coordinates, methods
and corrections clear obsolete presentation. MCP rejects data changed during calculation;
clients should also pin expected_revision across pages. Plot rendering samples
at most about 2,001 markers and labels the reduction. Tables, calculations and
exports retain every row; video overlays have not been optimized for huge tracks.

Axes, scale and derivative changes preserve queued/running tracking. Such edits
split delivered tracking history into separate undo checkpoints; Undo/Redo still
stop active tracking. The source observations and matching settings remain
independent of the derived coordinate system.

Full-source SSD-only saves use project version 2. This app reads versions 1–4; missing axes and
motion settings get the legacy coordinates and three-point default. Older builds
reject version 2 instead of silently ignoring its scientific settings. Version 3
is required for normalized matching provenance, as described below. A non-default
clip requires version 4; versions 1–3 open with full-source sampling. Derived
arrays are recomputed, not serialized. Project files retain source observations,
exact stamps, calibration, axes and method settings.

Standard CSV retains its original first five columns and appends vx/vy/speed/ax/ay, missing
reasons, method, window, source-pixel origin, angle and exact frame-stamp fields.
For metres, names include vx_m_s and ax_m_s2. MCP get_measurements preserves its
existing fields and adds the same quantities, units, missing reasons and motion
metadata. New tools set_axes and configure_motion share the native commands,
revision checks, retry deduplication, history and agent action log.
Provenance CSV now appends `tracking_method` after those original fields. MCP
measurements return nullable `matching_method`. New normalized-correlation
observations require project version 3 even if current settings return to SSD;
their recorded method survives settings changes and reopening.

Native Export CSV now defaults to a compact Excel profile with explicit regional
separators. Full precision remains the default, and rounding is opt-in. The
Standard profile preserves the original machine-readable schema. See
[CSV export](csv-export.md) for details and executed Excel checks.

See [the delivery verification report](motion-delivery-2026-11-14.md) for executed tests, measured performance
and the native screen-capture limitation.
