---
description: How to add a Rust dependency to the project
---

1. **Vet it first** (`AGENTS.md` § Dependencies): check crates.io for the latest stable version, pin one at least three
   days old, and check the repo is alive (recent commits, not archived or superseded by a fork).
2. **Add it to the crate that uses it**, not by habit to the app: `cargo add <crate>@<version> -p <package>` (for
   example `-p cmdr-fs`, or `-p cmdr` for `apps/desktop/src-tauri`). A crate in `index-crate-isolation`'s list must not
   pull in `tauri` or `cmdr` (`docs/guides/adding-a-crate.md`). Comment each new line in `Cargo.toml` with why it's
   there, like its neighbors.
3. **Check**: `pnpm check desktop-rust-cargo-deny` (licenses, advisories, crates.io-only sources),
   `pnpm check third-party-notices` (regenerates the notices; commit its rewrite), `pnpm check clippy`, the tests you
   touched, and `pnpm check --fast`. `pnpm land`'s CI runs the rest, `cargo-machete` included.
4. **Commit** the `Cargo.toml`, `Cargo.lock`, and notices changes with the code that uses the dep, impact first.
