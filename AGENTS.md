# Development instructions

Read `ARCHITECTURE.md` before making architectural or product changes. It defines
the purpose, acceptance criteria, component boundaries, and delivery sequence.

## Priorities

1. Accurate, inspectable scientific results.
2. Responsive native macOS interaction during concurrent analysis.
3. High useful throughput with bounded memory and no redundant processing.
4. Familiar Tracker workflows and clear inputs, progress, uncertainty, and outputs.
5. Codex control through a local MCP adapter using shared application commands.

## Implementation rules

- The main line is now the Rust workspace in `rust/` (see `rust/README.md` and
  `rust/DESIGN.md`). It targets Apple Silicon macOS first and keeps platform code
  behind traits so Windows and Linux backends can be added later. The Swift app is
  frozen as the reference implementation and test oracle: fix only what blocks a
  comparison, and add new features in Rust. Do not add GPU processing or a second
  media backend without a documented, measured reason.
- Keep scientific algorithms, scheduling, media adapters, UI, and MCP protocol
  handling separate. UI and MCP must use the same validated command/state layer.
- Do not impose a fixed product cap on video sessions. Bound active execution,
  frame queues, and memory using a shared scheduler; explain queued/throttled work.
- Keep expensive work off the main actor. Avoid per-session unlimited worker pools,
  full-video RAM retention, and per-frame UI/MCP updates.
- Preserve source timestamps, units, coordinate mappings, manual corrections, and
  run provenance. Never present predictions as observed positions or uncalibrated
  confidence scores as probabilities.
- Make uncertain tracking inspectable and correctable. Do not infer physical
  calibration without an explicit scale reference.
- Use session/job identifiers and revisions. Define cancellation, retry, stale-edit,
  and shutdown behavior. Keep agent changes visible and user-controllable.
- Check upstream licensing and attribution before porting code or copying assets.
  Initial Tracker project-format compatibility is deferred.

## Verification and reporting

- Rust: from `rust/`, run `cargo test --release`, then
  `cargo build --release -p tracker-mcp -p tracker-app -p tracker-ui` and
  `python3 scripts/mcp-e2e.py` for the packaged MCP workflow. Media and workflow
  tests use the Java Tracker sample clips in `~/Tracker/videos` and print SKIPPED
  when they are missing; report that distinction.

- Build the local app with `zsh scripts/build-app.sh`; launch `build/Tracker.app`.
  Generate fixtures using `python3 scripts/make-tracking-fixtures.py` (ffmpeg), then
  run `CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" swift test -c release -Xswiftc -swift-version -Xswiftc 5`.
  The flags work around pinned SDK 0.11.0 compilation on Swift 6.4. See docs/mcp.md.
  Media tests skip explicitly when fixtures are missing; report that distinction.
  Xcode tools may require execution outside a restrictive
  sandbox. Do not report unrun checks as passing.
- For interface changes, read `DESIGN.md`. Keep UI review active during development
  as requested for this project; record evidence and verification limits.
- Validate scientific behavior against known trajectories and annotated clips.
  Test timestamps, calibration, ambiguity, drift, and manual corrections.
- Benchmark performance in release builds on a recorded Mac/video configuration.
  Start with 1/2/4 simultaneous videos, then stress beyond those comparison points.
  Report UI latency, throughput, peak memory, pressure, and cancellation behavior.
- Include end-to-end Codex MCP validation when tools or command contracts change.
- Report observed results and limitations. Do not promise zero lag, unlimited
  hardware capacity, or language-based speedups without evidence.
