# Platform seams

How Rust code that differs per OS is shaped, in the app and in every crate. One shape everywhere: a feature's platform
split lives in its own `platform/` directory, one file per OS, behind a `mod.rs` that pins what every arm must answer.
Business logic above the seam asks once and never names an OS.

Why: the code that came before used six different dispatch patterns (sibling modules, `use … as platform` aliases,
`#[path]` swaps, one IPC command per OS, hand-mirrored `stubs/`, inline `cfg` in business logic), nothing checked that
the arms agreed, and `not(target_os = "macos")` arms meant "Linux" only by accident. Evidence:
`docs/notes/windows-port/app-audit.md` § "How platform dispatch is done today".

## The shape

```
system_memory/
  mod.rs              # the feature: types, the IPC command, portable helpers, tests
  platform/
    mod.rs            # picks the arm, pins its signatures, re-exports
    macos.rs
    linux.rs
    windows.rs
```

`platform/mod.rs` (from `apps/desktop/src-tauri/src/system_memory/platform/mod.rs`):

```rust
use super::SystemMemoryInfo;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> SystemMemoryInfo = system_memory_info;

/// This OS's memory breakdown, in non-overlapping categories that sum to the total.
pub(super) use imp::system_memory_info;
```

- **One `const _` pin per exported fn.** It fixes the exact signature, so an arm that drifts fails to compile on its own
  OS with an error that names the seam, and a reader sees the whole contract in one place. The pin names the re-exported
  item, not `imp::…` (`unused_qualifications` flags the qualified form).
- **The contract's docs sit on the `pub use` lines.** Each arm's fns carry only how that OS answers.
- **Arm fns are `pub(crate)`.** A `pub(super)` item can't be re-exported at `pub(crate)` (E0364), and the arm module
  itself is private, so nothing outside the seam can reach past `mod.rs` anyway.
- **The pin counts as a use.** A fn only one OS's callers ask (`platform::macos_at_least`) needs no `dead_code` allow on
  the others.
- **Every OS gets a file, even when its answer is "nothing yet".** `windows.rs` returning `None` is explicit,
  reviewable, and typed by the same pin; a silent "other" arm is what this replaces.
- **A stateful backend uses a trait** (`managed_policy::source::ManagedPrefsSource`, `secrets::SecretStore`), and the
  seam's job shrinks to picking the implementation.

Variations, all inside `platform/`:

- **Code two arms share** goes in a family file declared beside `imp`, `#[cfg(unix)] mod unix;`, and each arm re-exports
  from it (`pub(crate) use super::unix::volume_space;`). `apps/desktop/src-tauri/src/diagnostics_snapshot/platform/` is
  the example.
- **An arm that grows submodules becomes a directory**: `#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]`, and
  its children resolve beside that `mod.rs`.
- **An OS-only extra** that only same-OS code can use is re-exported under a positive gate in `mod.rs`
  (`#[cfg(target_os = "linux")] pub(crate) use imp::distro::LinuxDistro;`), never stubbed on the others.
- **A feature that IS the platform** (`apps/desktop/src-tauri/src/platform/`, which OS and version this is) has the arms
  directly under its own `platform/` directory.

## Unknown targets

Each crate root refuses a target nobody wrote arms for:

```rust
#[cfg(not(cmdr_host))]
compile_error!("Cmdr builds for macOS, Linux, and Windows only; see docs/guides/platform-seams.md");
```

"Unknown" means an OS outside macOS, Linux, and Windows. Windows is known on purpose, before it has arms: a Windows
build fails, where it fails, at each seam still missing its `windows.rs`, and that per-seam list is exactly what the
Windows CI lane is for (`docs/tooling/ci.md` § Windows lane). Making Windows "unknown" until it's done would hide all of
it behind one error.

A crate that also builds for the browser (`cmdr-fs`, `cmdr-listing`, …) widens the gate to
`not(any(cmdr_host, target_family = "wasm"))`. The app's root carries the gate today; crates gain it as they take the
named cfgs.

## Named cfgs

The workspace's named platform cfgs are set by one build-dependency crate, `crates/cmdr-platform-cfg/`, whose `emit()`
each member's `build.rs` calls. It declares every name for `check-cfg` and sets the ones that hold. The set is one name:

- **`cmdr_host`**: the target is macOS, Linux, or Windows, a desktop OS, as opposed to the browser build. It replaces
  `not(target_family = "wasm")` with a positive name, and it's the one list of supported OSes the unknown-target gate
  asks.

Deliberately not named:

- **`cmdr_apple`**: macOS is the only Apple OS Cmdr builds for, so it would be a second spelling of
  `target_os = "macos"`, which is also the arm's file name.
- **`cmdr_unix_host`**: `unix` already excludes `wasm32-unknown-unknown`, the only browser target the workspace builds.
  POSIX code asks `cfg(unix)`; code that merely needs a host OS asks `cmdr_host`.

A new name earns its place when a predicate is asked in many places and means one thing; it goes in `cmdr-platform-cfg`
so every member gets it at once.

Where the emitting lives, and why:

- ❌ A copy of the logic in each member's `build.rs`: each copy carries the OS list, and copies drift.
- ❌ `--cfg` flags in `.cargo/config.toml`: `RUSTFLAGS` from the environment replaces config rustflags rather than
  adding to them, so CI or a developer's shell would silently drop the cfg, and every flag change rebuilds the whole
  dependency graph.
- ❌ `include!` of a shared file from each `build.rs`: it works, but couples each member to another's directory layout.
- ✅ A build-dependency crate with no dependencies of its own: the standard Rust shape (the `cfg_aliases` crate is the
  public version), one definition, tested on its own, and each member's script stays one call. A member that names a cfg
  adds `cmdr-platform-cfg` under `[build-dependencies]` and calls `cmdr_platform_cfg::emit()`. A crate the browser demo
  links also adds the crate to the demo image's inputs (`crates/cmdr-listing-wasm/DETAILS.md` § The build).

## Enforcement

`desktop-rust-platform-cfg-placement` (in `--fast`) counts, per production file, negated OS predicates (refused
everywhere, seams included) and OS cfgs outside a `platform/` directory, against a per-file allowlist that only goes
down. The fix for a file over its number is to name the OSes an arm is for (`target_os = "linux"` and
`target_os = "windows"`, never `not(target_os = "macos")`) and move the split behind a seam. Test code is exempt. What
it counts exactly, and why: `scripts/check/checks/DETAILS.md` § "Platform cfg placement".

The first seams in this shape: `platform/`, `system_memory/platform/`, and `diagnostics_snapshot/platform/` under
`apps/desktop/src-tauri/src/`, with the Linux distro reader at `platform/linux/distro.rs`.
