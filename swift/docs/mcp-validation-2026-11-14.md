# Codex MCP validation, 2026-11-14

Scope: local Swift bridge, shared commands, image inspection and native user
control. Existing fixed-template scientific algorithms and the one-slot tracking
scheduler were preserved. Synthetic results do not establish real-footage
tracking accuracy or sustained UI performance.

## Environment and build

Apple M4, 16 GiB, Apple Silicon macOS 27.2.1, Swift 6.4.2. Built the release
application with `zsh scripts/build-app.sh`. The bundle contains Tracker,
TrackerMCP, and dependency license notices; both executable and bundle are
ad-hoc signed for local development. It is not a notarized distribution.
SDK 0.11.0 and its resolved dependencies are included in `Package.resolved`.
The documented Swift 5 language-mode workaround and narrow experimental-capability
adapter are required on this tested Codex/toolchain combination. Swift 6.1 and
macOS 14 deployment floors were not separately tested.

## Automated checks

All 26 release tests passed (20 app tests and 6 automation/protocol tests), with
no fixture skips. Release tests cover geometry, scale, observations/history/provenance, project
validation, VFR timestamps and orientation, known clean/noisy trajectories,
occlusion/distractor review, cancellation and the existing queued load baseline.
New tests cover explicit session targeting while another tab is selected,
request retry/conflict across client IDs, stale revisions, undo/redo, independent
frame inspection, source crop/resize and overlay coordinates, invalid calibration,
file-write failure retention, queued cancellation/disconnect, retained terminal
status, nonmodal tracking failure, JSON 64-bit timestamp preservation, actual
Codex capability normalization, socket framing/limits, peer UID, exclusive socket
ownership/modes and atomic no-clobber output.

`scripts/test-mcp.py` exercises the **packaged stdio executable** and native app.
Its 17 checks include initialization with Codex-shaped capabilities, import,
PNG image delivery with a 40×30 crop preview, unchanged viewer time after image
inspection, known scale, seed and 60-row clean tracking, reconnect while an
app-owned job completes, paginated results, duplicate/conflicting request IDs,
stale revision rejection, refused overwrite, save/CSV/reopen, unrecorded evidence,
explicit skip, cancel review, missing-source relink error and explicit relink.
Artifacts: `build/mcp-e2e/report.json`, PNG previews, project JSON and CSV.

## Actual Codex client

Codex CLI 0.166.0 ran an ephemeral client against the packaged bridge with
supported automatic approval review and only Tracker tools. It launched the
absent app and executed session/frame/calibration/mark/tracking/review/output
commands. No global Codex settings were changed. After build/protocol checks passed,
`.codex/config.toml` was created without replacing any existing file;
`codex mcp get tracker --json` confirmed the project server is enabled. The initial read-only client
correctly refused mutating tools; that refusal was not counted as a workflow pass.
A separate initial run exposed SDK 0.11.0's experimental-capability decoding issue;
the adapter fixed it and a regression test preserves that input.

Observed in the completed Codex run:

- Clean source frame showed the striped synthetic target near (30,45) in a
  160×120 image. The supplied 100-pixel reference represented 1 m.
- Tracking job `10B88000-F3BB-4384-BF91-A1477610C6E3` completed with 60 rows:
  one manual seed and 59 automatic observations. Initial analysis coordinates
  were (0.3,0.75) m. CSV export, project save and reopen completed with the same
  track ID `6F0DD792-B720-426B-80BB-A340C480FEFF` and 60 rows.
- Occlusion job `54BDB3A3-5150-4C86-A4B1-DB1799C6A229` paused at source time
  1.2 s with 20 observations. The image was blank where the target was hidden;
  candidate (25,25) remained unrecorded. Similarity was about 0.84757 and
  separation zero. These scores were not described as probabilities.
- Explicit acceptance, used solely to test the command on synthetic footage,
  recorded one `acceptedCandidate`, giving 21 rows. Continuation
  `CDAFF938-EB0F-4B2A-8D5C-798C101B3B07` then **failed** because the new template
  was uniform and lacked visible detail. This failure was retained and reported;
  it is not evidence of successful tracking through occlusion. Cancelling this
  already-failed continuation was harmless; active/queued cancellation was
  verified separately. The review project saved successfully.

Artifacts: `build/mcp-e2e/codex-summary.txt`, Codex JSONL tool events,
`codex-clean.tracker.json`, `codex-clean.csv`, and `codex-review.tracker.json`.
The synthetic accepted point is deliberately unsupported as a physical observation;
it must not be treated as measured ground truth or included in an accuracy claim.

## Native interface and lifecycle

Open Computer Use confirmed the Codex-created video tabs, selected track,
source timestamp, calibrated data and plot, 21-row test including the reviewed
R provenance, saved status and native job failure state. It also confirmed the
bottom control bar and recent-action popover. The final insufficient-detail
probe exposed its full error inline beside the tracking controls with no modal
alert. Error text uses the native primary label color for readability; failure
is also identified by the adjacent job-state text.

A live stdio probe was connected while the native **Stop Agent Control** button
was pressed. Subsequent tools returned `control_disabled`; a new initialization
returned a clear disabled-control error. Pressing **Allow Agent Control** restored
the existing bridge's tool access. Sessions/data remained present. Cleanly
quitting the saved test app and calling through that same bridge automatically
launched a new app and returned an empty workspace, with new app-lifetime IDs.
A disconnected bridge's lease can remain visible for up to 30 seconds.

## UI delivery gate

PASS: hard gates for the changed control surface. Build and native click-through
covered Allow, Stop and Recent Agent Actions; states match live MCP results.
Native buttons carry text/accessibility names and system focus/disabled behavior;
there are no new media assets, invented statistics, ghost links or decorations.
The supported desktop workspace keeps its minimum size; mobile layout is outside
this macOS application. Full production accessibility/theme audits remain open.

PASS: purpose. A single bottom status row keeps agent control visible without
replacing video/plot/table space; the bounded popover holds action history and
has an explicit empty state. Inline job errors expose real failure evidence.

PASS: project identity. The Tracker pane arrangement, source-frame readouts,
calibration, observations and provenance remain the interface's focal point.
ENERGY 1 / RHYTHM 2 / MOTION 1 from DESIGN.md; no animation was added.

PASS: changed-control behavior and consistency. Native Allow/Stop and the action
popover were exercised alongside actual MCP calls, preserving the native
palette and button language. This gate covers the MCP UI additions, not a claim
of exhaustive accessibility or full-app interaction testing.

## Limits

Operation/idempotency records last only for the app lifetime. Startup timeout
and hostile-peer failure paths were implemented but not fault-injected with a
second OS user or a deliberately hung app. Large/rotated real footage, sustained
resource pressure, latency percentiles, concurrent streaming and remote control
were not validated. Existing small synthetic throughput tests are regressions,
not new performance claims. No Rust backend, tracking-model upgrade or parallel
scheduler change was introduced.
