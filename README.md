# Tracker projects

Two independent applications share this Git repository. Swift remains Akas’s
native macOS project; Rust is the friend’s port, introduced by TkmanDevs in
commit `a4e5137`. Neither is designated as the replacement for the other.

| Project | Source and instructions | Build and run from the repository root |
| --- | --- | --- |
| Swift | [swift/README.md](swift/README.md) | `cd swift && zsh scripts/build-app.sh && open build/Tracker.app` |
| Rust port | [rust/README.md](rust/README.md) | `cd rust && cargo build --release --locked && ./target/release/tracker` |

Each project owns its dependencies, tests, documentation and build output.
The Swift app bundle is `swift/build/Tracker.app`; the Rust bundle, when built
with `zsh rust/scripts/bundle-macos.sh`, is `rust/target/Tracker.app`.
The two applications have separate bundle identifiers and project formats;
interchange is limited to what their individual documentation explicitly supports.
Both use the root [MIT license](LICENSE).

The project-local Codex MCP configuration targets the Swift app at its new path.
Restart its connection after rebuilding. Rust MCP setup is in its own README.

See [the Rust source review](docs/rust-port-review-2026-11-15.md) for findings and
verification limits.
