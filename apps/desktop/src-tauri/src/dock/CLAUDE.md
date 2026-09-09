# Dock

Cmdr's relationship with the macOS Dock: getting the app's tile down there, and what
that tile's right-click menu says. macOS-only. The nudge that asks lives in the
frontend; every decision that has to look at the machine is here.

## Module map

- **`mod.rs`**: `pin_state()` and `pin_cmdr()`, plus the three IPC types (`DockPinState`,
  `DockPinBlocker`, `DockPinFailure`) behind `../commands/dock.rs`.
- **`entries.rs`**: `persistent-apps` as pure `plist::Value` data — build a tile, decide whether an
  app is in the array, put a tile first.
- **`prefs.rs`**: the CFPreferences boundary. **`restart.rs`**: asking the Dock to reload.
- **`menu/`**: the tile's context menu, with its own `CLAUDE.md`. Separate concern,
  separate rules — it runs inside an AppKit callback where a panic is undefined
  behavior and a blocking read beachballs the Dock.
- "Is this copy somewhere a pointer may survive?" is `crate::install_location`, shared with the reveal handler, which
  refuses to write a machine-wide key from a copy that's about to move.

## Must-knows

- ❌ **Never write to `com.apple.dock` from a test.** A test run would rearrange the machine's real
  Dock. Reading it is fine and two tests do; the write path is exercised against a per-test scratch
  domain that deletes itself.
- ❌ **Never touch `~/Library/Preferences/com.apple.dock.plist` directly.** `cfprefsd` owns that
  file and caches it, so a direct read goes stale and a direct write gets clobbered. Everything goes
  through CFPreferences, the same door `defaults` uses.
- **A tile is matched by `bundle-identifier` first, path second.** The URL in `file-data` is
  percent-encoded, carries a trailing slash, and can be a `/.file/id=…` reference that matches no
  path we can build. Either key matching answers "already there": a false yes costs a silent hint, a
  false no puts a second Cmdr tile in someone's Dock.
- **Both `/Applications` and `~/Applications` count; nothing else does.** A tile pointing into
  `~/Downloads`, a mounted disk image, or a translocated copy dies as soon as that copy moves.
  The rule is `crate::install_location`, ❌ not `bundle_location::classify`, which answers a
  different question (can the updater write here).
- **A dev build switches the whole feature off for free**: it runs out of `target/` with no `.app`
  ancestor, so `running_bundle()` fails and the state is `NotABundle`.
- **The write is only half of it.** The Dock holds `persistent-apps` in memory and re-reads it at
  start, so a write nobody follows with a restart is invisible until the next login.
- **Cmdr is already in David's own Dock**, so his manual check needs him to drag it out first.

The verified entry shape, why the POSIX-path form, what the `book` blob is and why we skip it, the
TCC findings, and the manual check: `DETAILS.md`.
