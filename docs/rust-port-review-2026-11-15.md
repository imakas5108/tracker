# Rust port source review — 15 November 2026

Scope: friend’s port introduced in `a4e5137`. This is a correctness and architecture
review of the committed Rust source, not an independently reproduced performance,
security or native UI certification. Only project organization/documentation was
changed during this review; the Rust runtime implementation was left intact.

## Assessment

The port has useful component boundaries: core models, vision, media, datasets,
shared application commands, IPC, MCP and UI are separate crates. Exact rational
frame ordering uses integer cross products; automatic insertion protects manual
and reviewed measurements. Prediction selects search regions rather than being
stored as an observation. Tracking workers use bounded execution slots and batch
delivery. These are sound foundations, but do not establish production readiness
or an advantage over the current Swift implementation.

## Findings

### P1 — clip selection does not constrain scientific exports

`rust/crates/tracker-app/src/lib.rs:659–662` passes the whole track into
`kinematics` for CSV. `dataset_add_run` does the same at lines 699–704 and fits those
rows at line 732. Neither path filters by `analysis.clip` or applies its step.
A user can select an interval but export/fit points from other intervals. This
contradicts the `Clip` model’s stated export semantics. Filter the selected source
frame sequence before derivatives, fitting and exports, and test retained points
outside the interval and a non-unit step.

### P1 — datasets can misattribute the tracking algorithm and settings

`rust/crates/tracker-app/src/lib.rs:722–723` labels a stored dataset run using
`session.settings`, even when its points were generated before those settings
changed or by multiple jobs. Measurements retain a job UUID, but saved projects
contain no corresponding immutable run-settings registry (`tracker-core/src/model.rs`
and `project.rs`). Changing algorithms after tracking can therefore relabel the
dataset and reopening loses the information needed to reconstruct the original
settings. Persist immutable per-run settings and retain references through save,
reopen and dataset/CSV export; represent mixed or unknown provenance explicitly.

### P2 — native Save performs blocking disk work on the UI execution path

`rust/crates/tracker-ui/src/window.rs:254–258` directly calls
`core.save_project` inside the action handler. That command serializes and writes
the project synchronously (`tracker-app/src/lib.rs:638–645`). The save-as/export
handlers also invoke synchronous commands from `cx.spawn` continuations, without
explicit background execution. Large projects or slow storage can block native
interaction. Snapshot briefly, serialize/write in the background, then publish
completion using the snapshot revision. Verify input latency during a large save.

### P2 — imports bypass the bounded tracking scheduler

Every video import starts a new thread at `tracker-app/src/lib.rs:238`, and project
opening does the same at line 261. Both index media and fingerprint the source.
Tracking slots do not bound these workers, so a large batch of imports can start
many concurrent indexers and disk reads. Admit imports through shared bounded
execution with visible queueing and cancellation, and stress beyond 1/2/4 inputs.

## Compatibility and evidence limits

The Rust reader explicitly supports Swift version 1 only; current Swift saves
can be versions 2–4. Rust version 2 uses its own `tracker-rs-project` format. Treat
these as separate applications and do not assume interchangeable project files
or tool contracts. Existing Rust README test/performance statements are the port
author’s recorded evidence, not results independently reproduced in this review.

Release tests were attempted with `cargo test --release --locked`. The sandboxed
attempt could not resolve crates.io. An allowed network retry stalled with repeated
transfer timeouts; offline mode confirmed the `csv` dependency was unavailable.
The network attempt was stopped. Rust tests, native UI, MCP end-to-end and performance
were therefore not validated here. No missing-media skip is counted as a pass.

Before adopting the port for scientific work: resolve the findings, run release
unit/integration tests and the packaged MCP workflow, compare both implementations
against the same independently annotated clips, and measure UI latency, cancellation,
peak memory and throughput under concurrent loads. Keep tuning and held-out
acceptance data separate.

## Repository separation verification

Swift now lives in `swift/` and Rust remains in `rust/`, with scoped instructions
and independent build outputs. Shared license and Git history remain at root.
The project-local MCP configuration points to `swift/build/Tracker.app`.
Swift runtime sources and tests were moved without content changes. The relocated
Swift app built successfully; its release tests and 18 packaged MCP workflow
checks passed. Native UI layout and Rust runtime were not reviewed interactively.
The first MCP attempt raced app startup; waiting for the endpoint resolved it.
