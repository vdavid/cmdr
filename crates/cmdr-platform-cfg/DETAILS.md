# cmdr-platform-cfg details

## Why its own crate

A named cfg has to be set by each crate's own build script: cargo applies `rustc-cfg` to the package whose script
printed it, never to its dependents. So the choice is where the shared logic lives, weighed in
`docs/guides/platform-seams.md` § "Named cfgs": a build-dependency crate beats a copy per `build.rs`, `--cfg` flags in
`.cargo/config.toml` (silently replaced by an environment `RUSTFLAGS`), and an `include!` across directories.

## Who calls it

The app (`apps/desktop/src-tauri/build.rs`), for the crate root's unknown-target gate. A crate joins by adding
`cmdr-platform-cfg` under `[build-dependencies]` and calling `emit()` from its `build.rs`; one the browser demo links
also adds this crate to the demo image's inputs (`crates/cmdr-listing-wasm/DETAILS.md` § The build).

## The public surface is capped

One root promise, `emit` (`scripts/check/checks/index-crate-isolation-ceilings.go`). The cfg names and the OS list stay
private: a second public item is a second way to decide the platform, so argue it here first.
