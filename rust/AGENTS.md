# Rust port instructions

This is an independent port of the sibling `../swift/` Tracker application,
introduced by TkmanDevs in commit `a4e5137`; it does not supersede Swift development.
Read `README.md` and `DESIGN.md` before architectural or interface changes.

Keep scientific algorithms, media, scheduling, commands, UI and MCP separated.
Preserve exact timestamps, explicit calibration, manual corrections and run
provenance. Scores are not probabilities. Bound execution and memory, and keep
expensive work off the UI thread. UI and MCP use the same validated commands.

From this folder run `cargo test --release --locked`, then
`cargo build --release --locked -p tracker-mcp -p tracker-app -p tracker-ui` and
`python3 scripts/mcp-e2e.py`. Report missing-media skips separately from passes.
Validate trajectories, ambiguity, cancellation and stale edits. Record release
performance and native UI evidence before making performance or usability claims.
The shared license is `../LICENSE`. Do not copy upstream code/assets without
checking licensing and attribution. Do not change Swift to accommodate this port
without an explicit reason and verification of both projects.
