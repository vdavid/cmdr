# cmdr-platform-cfg

The workspace's named platform cfgs, as a build dependency: a member's `build.rs` calls `cmdr_platform_cfg::emit()`,
which declares every name for `check-cfg` and sets the ones that hold for the target. Today that's `cmdr_host` (macOS,
Linux, or Windows), which the unknown-target `compile_error!` asks.

## Module map

- **lib.rs**: the whole crate. The host-OS list, `emit`, and the tests that pin its directives per target.

## Must-knows

- **Zero dependencies, on purpose.** It compiles before every member that names a cfg, so anything it pulls in slows all
  of them.
- **Members ask the cfg, never the list.** `HOST_OSES` stays private, so the supported set lives in one place.
- **A new name goes here, not in a member's own `build.rs`**, so every member gets it at once. Which names exist and why
  this few: `docs/guides/platform-seams.md` § "Named cfgs".
- The public surface is capped at `emit` by `index-crate-isolation`.

Why a crate rather than a copy per `build.rs`: `DETAILS.md`.
