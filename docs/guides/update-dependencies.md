---
description: How to update project dependencies
---

1. Frontend: `ncu` to see them, then `ncu -u && pnpm install` to apply. Then check with `pnpm check --svelte`.
2. Rust: `cd apps/desktop/src-tauri && cargo update && cargo outdated` (update within semver ranges; check for newer
   versions) If updating major versions, edit `Cargo.toml` manually, then do `cargo build`. Then check with
   `pnpm check --rust` and `pnpm check desktop-e2e`.

3. Either way, finish with `pnpm check third-party-notices`. It rewrites `THIRD-PARTY-NOTICES.md` and
   `third-party-packages.gen.json` from the resolved graph, and both are committed and ship to users. Locally the check
   regenerates them; in CI it only VERIFIES, so a bump that skips this step lands green on your machine and reds
   `Desktop (Rust)` on `main` (2026-08-26).

Verifying a bump before you take it: download both `.crate` files from `static.crates.io` (the crates.io API refuses
unauthenticated downloads) and `diff -ru` them, so you review what cargo actually installs rather than a GitHub compare.
Then `cargo update -p <crate> --precise <version>`, which touches exactly that one lockfile entry instead of dragging
transitive deps forward.

## Version constraints

- Node, pnpm, Go: See `.mise.toml`
- Rust: stable channel (see `rust-toolchain.toml`)
- Frontend deps: See `package.json`
- Rust deps: See `apps/desktop/src-tauri/Cargo.toml`

We try to use the latest of everything.
