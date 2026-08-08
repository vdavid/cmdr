# Linux

## 1. Prequisites

For Ubuntu 24.04:

```
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  librsvg2-dev \
  libacl1-dev \
  patchelf
```

## 2. Bundle

Build:
```bash
pnpm --filter @cmdr/desktop tauri build --bundles deb
```

Install and run:

```bash
sudo dpkg -i target/release/bundle/deb/Cmdr_*_amd64.deb
Cmdr                    # /usr/bin/Cmdr
```

Remove:

```bash
sudo apt remove cmdr
```

## 4. `pnpm check` on Linux

**Gotcha: `bindings-fresh` is platform-dependent.** `pnpm bindings:regen` exports `bindings.ts` from
whatever the host platform's `cfg` gates compile, so on Linux every macOS-only IPC command regenerates as its
`#[cfg(not(target_os = "macos"))]` stub, doc comment and all. Running that check on Linux rewrote 331 lines of the
committed macOS-generated file.

## 5. What's actually broken on Linux

### Live file watcher never starts

`crates/cmdr-index/src/indexing/watch/watcher.rs:283` does `watcher.watch(root, RecursiveMode::Recursive)` over the
whole volume. The `notify` crate walks the tree to add inotify descriptors and errors out on the first directory it
can't read, so `DriveWatcher::start` returns `Err` and the volume gets **no live updates at all** until the next full rescan:

```
WARN indexing::lifecycle::manager  Failed to start DriveWatcher (scan will proceed without watcher):
  Failed to create watcher: inotify watch: Permission denied (os error 13) about ["/usr/share/ollama/.cache"]
```

One root-owned cache dir killed watching for the entire disk. macOS never hits this: an FSEvents stream is a
system-level subscription with no per-directory descriptor, so the two backends are far less symmetric than
`crates/cmdr-index/src/indexing/watch/CLAUDE.md` reads. Any Linux box with one unreadable directory anywhere under `/`
gets a dead watcher, which is most of them. Fix shape: walk and add per-directory, skipping `EACCES` instead of
aborting.

### `Cmd+` accelerators bind to Super, not Ctrl

`menu/linux.rs` declares `Cmd+F`, `Cmd+A`, `Cmd+I`, `Cmd+,`, `Cmd+1`. muda maps `"CMD"` to `Modifiers::META`
(`muda-0.19.3/src/accelerator.rs:536`), and META is the Super key on GTK. `CmdOrCtrl` is the string that becomes Ctrl
off macOS. So the menu advertises Super chords to Linux users.

The comment at `menu/linux.rs:223` claims these "map to Ctrl+digit on Linux". They don't.

It isn't broken in practice, because the frontend keydown layer accepts `metaKey || ctrlKey` and picks Ctrl up anyway
(verified: Ctrl+A selects all, Ctrl+T opens a tab). The bug is the label.

### The copy is written for macOS

504 occurrences of `⌘`, `Finder`, `macOS`, or "your Mac" in `src/lib/intl/messages/en/`. Live examples seen on screen:

- Settings > Language: "follows your Mac's language".
- Viewer status bar: `W wrap · F tail · ⌘F search`.
- The MCP server's own tool instructions describe Cmdr as "a keyboard-driven two-pane file explorer for macOS".

The i18n plumbing is already there, so this is a per-string platform-variant problem.

## 6. Loose observations

- `nusb` logs `ERROR interface is busy (errno 16)` at startup when an MTP phone is plugged in and claimed by another
  process.
- GTK prints `Theme parsing error: gtk.css:7:21` once per web process. Comes from the system theme, not from Cmdr.
- Under Wayland, GNOME does not let `xdotool windowactivate` focus the app, so synthetic keys go nowhere. Launch with
  `GDK_BACKEND=x11` when you need to drive it. The app itself runs fine natively on Wayland.
- The app's own MCP server is the best Linux driving surface: the `tauri-plugin-mcp-bridge` used by the Tauri MCP tools
  is `#[cfg(debug_assertions)]`, so a release build has no bridge.
