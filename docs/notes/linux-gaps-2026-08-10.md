# What's missing on Linux

Cmdr builds, bundles, and runs on Linux, but Linux isn't advertised or supported, and nothing here is on a roadmap. This
is the ledger of what a Linux user actually hits, so the next person to look doesn't rediscover it.

Found by @symunona on Ubuntu 24.04 (GNOME, both X11 and Wayland), 2026-08-08, running a locally built `.deb`. Build and
install steps live in `CONTRIBUTING.md` § Linux testing.

## The live file watcher never starts

`DriveWatcher::start` (`crates/cmdr-index/src/indexing/watch/watcher.rs`) watches the volume root with
`RecursiveMode::Recursive`. On Linux `notify` walks the tree to add an inotify descriptor per directory and gives up on
the first one it can't read, so a single root-owned directory anywhere under the root fails the whole call:

```
WARN indexing::lifecycle::manager  Failed to start DriveWatcher (scan will proceed without watcher):
  Failed to create watcher: inotify watch: Permission denied (os error 13) about ["/usr/share/ollama/.cache"]
```

The volume then gets no live updates at all until the next full rescan. Most Linux machines have at least one unreadable
directory under `/`, so this is the common case, not an edge case.

macOS never hits it: an FSEvents stream is one system-level subscription with no per-directory descriptor. The two
backends are far less symmetric than `crates/cmdr-index/src/indexing/watch/CLAUDE.md` implies.

Fix shape: add watches per directory and skip `EACCES` instead of aborting the whole start.

## `Cmd+` menu accelerators bind to Super, not Ctrl

`menu/linux.rs` declares its accelerators as `Cmd+…` (`Cmd+F`, `Cmd+A`, `Cmd+I`, `Cmd+,`, `Cmd+1`, and the sort and zoom
chords). muda maps `"COMMAND" | "CMD" | "SUPER"` to `Modifiers::META` unconditionally, and META is the Super key on GTK;
`CmdOrCtrl` is the string that resolves to `CONTROL` off macOS (verified in muda 0.19.3, `src/accelerator.rs` lines 536
and 541, 2026-08-10). So the menu advertises Super chords to Linux users.

It isn't broken in practice: the frontend keydown layer accepts `metaKey || ctrlKey`, so Ctrl+A and Ctrl+T work as users
expect. Only the printed accelerator label is wrong. Switching those strings to `CmdOrCtrl` would fix the label, but it
also changes what the menu binds on macOS, so it needs checking on both platforms rather than a blind sweep.

## The copy is written for macOS

`src/lib/intl/messages/en/` carries 504 occurrences of `⌘`, `Finder`, `macOS`, or "your Mac". Seen on screen:

- Settings > Language: "follows your Mac's language".
- Viewer status bar: `W wrap · F tail · ⌘F search`.
- The MCP server's own tool instructions describe Cmdr as "a keyboard-driven two-pane file explorer for macOS".

The i18n plumbing is already in place, so this is a per-string platform-variant problem rather than an architectural
one.

## Loose observations

- `nusb` logs `ERROR interface is busy (errno 16)` at startup when an MTP phone is plugged in and another process has
  claimed it.
- GTK prints `Theme parsing error: gtk.css:7:21` once per web process. It comes from the system theme, not from Cmdr.
- Under Wayland, GNOME won't let `xdotool windowactivate` focus the app, so synthetic keys go nowhere. Launch with
  `GDK_BACKEND=x11` to drive it. The app itself runs fine natively on Wayland.
- The app's own MCP server is the best Linux driving surface. The `tauri-plugin-mcp-bridge` behind the Tauri MCP tools
  is `#[cfg(debug_assertions)]`, so a release build has no bridge.
