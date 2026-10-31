# rustic_core 0.13.0, patched

The crates.io release of `rustic_core` 0.13.0 (upstream `rustic-rs/rustic_core` at `1030be45`, path `crates/core`),
swapped in through the root `Cargo.toml`'s `[patch.crates-io]`. Everything here is the published package byte for
byte, apart from the changes below and the one file left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.

## The changes

- `Cargo.toml` and `Cargo.toml.orig`: the `zstd` dependency moves from `0.13.3` to `0.14.3`. The `zstd` 0.13 line (with
  `zstd-safe` 7) carries RUSTSEC-2026-0391 and 0395–0406, and 0.14 clears them. No API change reached rustic_core's
  code: it builds unchanged, and Cmdr's restic tests, which read real zstd-compressed v2 repos, pass
  (`cargo nextest run -p cmdr-restic`, 2026-10-26).
- `Cargo.toml`: the `no_mangle_generic_items = "warn"` lint setting goes. Rust removed that lint, so it only printed a
  warning on every rebuild, which a path dependency's uncapped lints let through (rustc 1.99.1, `pnpm dev`, 2026-10-31).
  `Cargo.toml.orig` inherits its lints from upstream's workspace, so it has no such line.

## Dropping it

Upstream `main` makes both changes in `rustic-rs/rustic_core#629` (merged 2026-10-30, a breaking change), unreleased as
of 2026-10-31. When a rustic_core release depends on `zstd` 0.14 or later, delete this folder, its `[patch.crates-io]`
line, and its `exclude` entry. `desktop-rust-vendor-patch-applied` fails as soon as a bump past 0.13.0 makes cargo
resolve crates.io again.
