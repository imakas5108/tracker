# Independent Tracker projects

This repository contains two independently developed applications:

- `swift/`: Akas’s native Swift macOS application. Follow `swift/AGENTS.md`,
  `swift/ARCHITECTURE.md` and `swift/DESIGN.md` for changes there.
- `rust/`: the friend’s Rust port, introduced by TkmanDevs in commit `a4e5137`.
  Follow `rust/AGENTS.md`, `rust/README.md` and `rust/DESIGN.md` for changes there.

Neither project replaces or freezes the other. Keep builds, dependencies, tests,
application commands and generated artifacts inside their owning project.
Do not assume project-file or MCP compatibility between them. Preserve the MIT
license and existing attribution. Shared comparisons belong in root `docs/`.
