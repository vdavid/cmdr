# Virtual MTP device in dev

Exercise MTP flows (drag-and-drop, transfers, conflict dialogs, rename, delete) in a normal `pnpm dev` session without
plugging in a real Android phone. A fake MTP device shows up in the volume picker, pre-populated with a few folders and
files.

```bash
CMDR_VIRTUAL_MTP=1 pnpm dev
```

The device appears as **Google Virtual Pixel 9** with two storages: **Internal Storage** (writable) and **SD Card**
(read-only). Both are backed by local directories under `/tmp/cmdr-mtp-e2e-fixtures/`, seeded with:

```
Internal Storage/
  Documents/report.txt, notes.txt
  DCIM/photo-001.jpg
  DCIM/Burst/burst-001.jpg
  Music/  (empty)
SD Card/
  photos/sunset.jpg
```

So you can immediately drag a file onto it, copy out of it, trigger a conflict dialog, and so on.

It's also **the test rig for drag-out file promises** (dragging a phone/NAS file to Finder downloads it). The Finder
drop leg can't be automated honestly (Finder owns the gesture), so the virtual device is how you exercise that feature
by hand: drag `DCIM/photo-001.jpg` from the virtual pane onto the Desktop and watch it download under Finder's chosen
name, with a completion toast. The full manual protocol (the 11 Finder-leg checks) and the architecture live in
`apps/desktop/src-tauri/src/native_drag/DETAILS.md` § "Manual verification (the Finder leg)" and
`apps/desktop/src-tauri/src/native_drag/CLAUDE.md`.

## Running it alongside your real dev session

`CMDR_VIRTUAL_MTP=1 pnpm dev` reuses the default `dev` instance (data dir, ports). If you already have a plain
`pnpm dev` running, give the virtual-MTP session its own instance so they don't collide:

```bash
CMDR_VIRTUAL_MTP=1 pnpm dev --worktree mtp
```

See `instance-isolation.md` for what `--worktree` separates (data dir, Vite / MCP ports, Dock label).

## Custom backing dir

`CMDR_VIRTUAL_MTP=1` (or `true` / `yes` / `on`) uses the default `/tmp/cmdr-mtp-e2e-fixtures/` root. Pass a path instead
to back the device with your own directory tree (this is how the Playwright checker gives each run its own
`/tmp/cmdr-mtp-e2e-fixtures-<pid>/`, since the MTP shard wipes whatever root it's given):

```bash
CMDR_VIRTUAL_MTP=/tmp/my-android-mock pnpm dev
```

Whatever lives under `<dir>/internal/` and `<dir>/readonly/` becomes the two storages. The dir is wiped and reseeded
with the default fixtures on startup, so point it at a throwaway path, not real data.

## First-build cost

`CMDR_VIRTUAL_MTP` makes the wrapper add `--features virtual-mtp` to the dev build. Changing the Cargo feature set
triggers a full-ish recompile, so the **first** `CMDR_VIRTUAL_MTP=1 pnpm dev` after a plain `pnpm dev` takes a couple of
minutes. Switching back to plain `pnpm dev` recompiles once more. Within either mode, hot reload is unaffected.

## How it works

Two pieces, both already in the repo for E2E:

- **Compile-time gate.** The virtual device lives behind the `virtual-mtp` Cargo feature
  (`crates/cmdr-mtp/src/virtual_device.rs`), so release `pnpm build` binaries never contain it. The wrapper
  (`apps/desktop/scripts/tauri-wrapper.ts`) appends `--features virtual-mtp` to the dev build only when
  `CMDR_VIRTUAL_MTP` is set.
- **Runtime gate.** At startup `activate_from_env_if_requested()` decides whether to register the device. It registers
  when either we're under an E2E run (`CMDR_E2E_MODE=1`) **or** `CMDR_VIRTUAL_MTP` is set, and never when
  `CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP` is set (the override non-MTP E2E shards use). A `virtual-mtp`-compiled binary
  launched with none of those vars behaves like a plain build, so the dev opt-in never changes what E2E sees.

This is the same device the Playwright MTP specs drive (see `apps/desktop/test/e2e-playwright/CLAUDE.md` and
`apps/desktop/src-tauri/src/mtp/DETAILS.md` § "Virtual MTP device (dev + E2E activation)"). The fixture tree matches
`apps/desktop/test/e2e-shared/mtp-fixtures.ts`.

## Running the Rust suites against it

MTP cells live on both sides of the crate boundary, and one command runs them all:

```bash
cargo nextest run --workspace --features cmdr/virtual-mtp -E 'package(cmdr-mtp) + test(mtp)'
```

`pnpm check rust-tests` covers the same cells: it runs the whole workspace suite with the same feature flag, so the
filter above is the way to run only the MTP ones while you're iterating.

Three things about that command are load-bearing:

- **The feature is package-qualified** (`cmdr/virtual-mtp`, never a bare `virtual-mtp`). A bare one changes meaning as
  soon as more than one package is selected, and the qualified spelling resolves the same way the shared lane resolves
  it, so flipping between the two triggers no rebuild.
- **The app's `virtual-mtp` forwards to the crate's `virtual-device`.** That's the only route: `cmdr-mtp` names its own
  feature `virtual-device` (which in turn forwards to `mtp-rs/virtual-device`), and the app re-exports it under the name
  the wrapper and the Playwright checker already know. Without it, `cargo nextest` silently filters every virtual-device
  cell out and they protect nothing while looking like coverage.
- **`cargo test` is not a substitute.** Every fake phone registers under one serial, so two cells in one process share a
  connection. `virtual_device_test_lock()` covers that, and nextest's process-per-cell model makes it moot; `cargo test`
  does not.

Both sides reach a device through the same doors, parametrized by which manager they hand it:

- A cell inside `cmdr-mtp` calls `cmdr_mtp::testing` (gated on `testing` plus `virtual-device`) with the crate's own
  shared manager over a detached host.
- An app-side cell calls `apps/desktop/src-tauri/src/mtp/test_support.rs`, which shadows each of those entry points with
  a no-argument version over the manager the APP parked, so the listing cache, the index, and the volume registry see
  what the device reports.

What a cell must not break, and which side a cell belongs on: `crates/cmdr-mtp/DETAILS.md` § "Three properties a cell
must not break" and § "Which side a test lives on".

## Limitations

- macOS auto-suppresses `ptpcamerad` for real devices; the virtual device skips USB entirely, so that path doesn't run.
- The device is registered once at startup. To reset its contents, restart the session (each launch wipes and reseeds
  the backing dir).
