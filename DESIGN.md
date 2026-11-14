# Interface direction

Reading this as a scientific desktop workspace for video analysis, using Tracker's
pane structure and native macOS controls. ENERGY 1 / RHYTHM 2 / MOTION 1.

- The video is the focal point; tools are compact and results remain adjacent.
- A narrow track list and a stacked plot/table inspector preserve the analysis
  hierarchy without importing every legacy toolbar item.
- System typography supports macOS familiarity and accessibility. Monospaced
  digits are reserved for timestamps and measurements to prevent layout jitter.
- System surfaces follow the user's light/dark appearance. A dark video surround
  separates the source image from controls and handles letterboxing.
- Blue identifies active tools and selected measurements; textual labels and
  crosshairs communicate state without relying on color alone.
- Dividers separate working regions; there are no decorative cards, gradients,
  logos, illustrations, or entrance animations. Native SF Symbols identify actual
  operations, always accompanied by labels or accessible names.
- The repeated motif is frame/time/coordinate readouts tied to real source data.
- Initial scope: import, video sessions, playback, manual marking, two-point scale
  calibration, axes, data inspection, and CSV export. Autotracking is explicitly
  unavailable in this UI prototype. No invented sample measurements.

Reference inspected: Tracker's public interface documentation and its interface.gif
screenshot on 2026-11-13. No upstream source or image assets are bundled.

https://opensourcephysics.github.io/tracker-website/help/interface.html
