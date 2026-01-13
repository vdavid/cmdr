---
description: How to update project dependencies
---

1. Frontend: `ncu` to see them, then `ncu -u && pnpm install` to apply. Then check with `./scripts/check.sh --svelte`.
2. Rust: `cd src-tauri && cargo update && cargo outdated` (update within semver ranges; check for newer versions) If
   updating major versions, edit `Cargo.toml` manually, then do `cargo build`. Then check with
   `./scripts/check.sh --rust` and `./scripts/check.sh --check test:e2e`.

## Version constraints

- Node, pnpm, Go: See `.mise.toml`
- Rust: stable channel (see `rust-toolchain.toml`)
- Frontend deps: See `package.json`
- Rust deps: See `src-tauri/Cargo.toml`

We try to use the latest of everything.
