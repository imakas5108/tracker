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
- Current scope adds project save/reopen, undo/redo, and a compact autotracker strip
  above the video. Uncertain-match evidence and review controls appear only when
  needed, keeping the video dominant. Dashed rectangles indicate measured template
  and search settings, not decoration. Chart symbols and table source codes
  distinguish manual, automatic, and explicitly reviewed results.
- No invented sample measurements. Synthetic validation footage is labeled as such
  in development reports and stays outside the application bundle.

Reference inspected: Tracker's public interface documentation and its interface.gif
screenshot on 2026-11-13. No upstream source or image assets are bundled.

https://opensourcephysics.github.io/tracker-website/help/interface.html

Agent control adds a compact bottom status bar with native Allow/Stop buttons.
It stays outside the video and analysis panes so the scientific workspace keeps
its hierarchy. Connection counts and recent plain-language actions communicate
real state. A native popover holds the bounded activity history and an honest
empty state; there are no decorative agent icons, animations or invented metrics.
