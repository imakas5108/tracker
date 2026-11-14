# Interface direction (Rust app)

Reading this as a scientific desktop workspace that should feel familiar to people who
learned Java Tracker, plus the agent visibility the Swift app introduced.
ENERGY 1 / RHYTHM 2 / MOTION 1, kept from the Swift app's direction.

## What is mimicked from Java Tracker, and what is not

Kept, because teachers and students already know where things are:

- Pane topology: main video view on the left with the player bar under it; two stacked
  view panes on the right, each with a chooser (Plot or Table); all splits resizable.
- Native menus in Java's order: File, Edit, Video, Tracks, Coords, plus an Agents menu.
- A toolbar row in Java's order: open/save, clip settings, calibration, axes, new point
  mass, mark, autotrack, then export and datasets.
- A track bar under the toolbar instead of a sidebar: track chips with their color, and
  live readouts (t, x, y, provenance) for the selected track on the current frame.
- Shift-click marks in any mode; in Mark mode the video steps forward after each mark.
- Calibration by clicking both ends of a known length; axes origin by clicking.
- Template and search boxes drawn around the seed, as in the autotracker wizard.

Not copied:

- No Java Tracker icons or artwork. The project is MIT and Java Tracker's assets are
  GPL-3.0. The toolbar uses text labels; any icon set is a separate decision for the
  owner (antislop R-23: no assets created without confirmation).
- No Swing look. Controls follow a light, neutral macOS-style workspace.
- Not every Java control: only operations the Rust core actually implements appear.
  Nothing in the window is a placeholder (antislop R-26).

## Additions carried over from the Swift app

- Status line: what to do next, unsaved state and the undo step, point count and unit,
  and the tracking scheduler's load (running, queued, slots).
- Agent bar: whether agent control is allowed, which agents are connected, the latest
  action, an Agent Activity panel listing recent actions by agents and by you, and a
  Stop Agent Control button that takes effect immediately.
- Uncertain-match review strip: reason, score and separation (labelled "not a
  probability"), and Accept, Correct, Skip Frame, Stop. When a job pauses, the viewer
  jumps to the uncertain frame so the candidate is visible (agents never depend on the
  viewer, so this does not affect them). The candidate is drawn as a
  yellow circle and is never recorded until accepted.
- Provenance everywhere: table rows carry M (manual), A (automatic) or R (reviewed);
  in plots, manual and reviewed points are hollow squares and automatic ones are filled.

## Visual decisions and reasons

- Light neutral surfaces (#e9ebee window, #f7f8f9 panels, white data surfaces) so the
  video and the track colors carry the visual weight. The video surround is near-black
  to separate footage from controls and to handle letterboxing.
- One accent blue (#1f5fbf) marks the active tool, the selected track and the current
  time. Track colors come from a fixed six-color set chosen to read on lab footage.
- Magenta axes and cyan calibration tape match the roles Java users expect (visible on
  most footage and distinct from track colors).
- Warning amber is reserved for uncertain frames; red for failures. Text always states
  the condition, so color is never the only signal.
- System font. Monospaced digits are not forced in the UI yet; readouts use fixed
  decimal places to limit jitter.
- No gradients, glow, decorative cards or entrance animations. The only motion is video
  playback.

## States

- Empty: what the app does, how agents fit in, and one Open Video button.
- Loading: "Opening video and indexing frame timestamps" over the video area.
- Error: the media error in words over the video area, and in the status line.
- Plot and table empty states explain what produces data.

## Known gaps (not yet verified)

- Screen-reader labels: GPUI exposes AccessKit, but this window does not set roles or
  names on its custom controls yet.
- Keyboard: Space, Left/Right, Escape, menu shortcuts and dialog Tab, Shift-Tab, Enter
  and Esc work (dialog fields are also clickable); toolbar buttons are not yet
  reachable with Tab.
- Like most macOS apps, a click on an inactive Tracker window only brings it to the
  front; the click does not also press the control under the pointer.
- Dark mode: the palette is light-only for now.
- Text fields in dialogs are minimal (typing, Backspace, Tab, Enter, Esc; no cursor
  movement or paste).
