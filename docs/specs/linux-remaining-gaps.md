# Linux remaining gaps

Companion to `support-linux.md` (core file operations, volumes, indexing, MTP, network, polish) and
`linux-ui-foundations.md` (window decorations, shortcuts, menus, file actions). This spec covers everything
else needed for a complete Linux release.

Each section is self-contained and can be handed to an agent independently.

## Task list

- [ ] 1. Quick Look and Get Info (small) — unbind shortcuts, no-op error paths
- [ ] 2. Accent colors via XDG Desktop Portal (medium) — D-Bus call + live updates + fallback chain
- [ ] 3. Appearance settings opener (tiny) — DE-specific commands replacing broken `xdg-open`
- [ ] 4. Volume chooser shortcuts and key naming (small) — Alt+F1/F2, F2 rename fix, Super label
- [ ] 5. GTK menu mnemonics (small) — add `&` prefixes to `build_menu_linux()` labels
- [ ] 6. Linux-specific error messages (small) — audit and replace macOS terminology
- [ ] 7. Credential storage resilience (medium) — Secret Service → keyutils → encrypted file fallback
- [ ] 8. Media eject — deferred (not on macOS either)
- [ ] 9. High-DPI support — no code changes needed, just verification
- [ ] 10. Trash implementation (medium) — `trash` crate, wire up in lib.rs
- [ ] 11. Network filesystem detection for copy (medium) — `/proc/self/mountinfo` parser, copy strategy
- [ ] 12. File watching E2E verification (small) — add inotify E2E test
- [ ] 13. MTP USB permissions (small) — error messages + packaging metadata
- [ ] 14. SMB mounting completion (large) — `smbclient` fallback, auth prompts, cross-DE testing
- [ ] 15. Custom drag image — deferred (no WebKitGTK API)
- [ ] 16. Dropbox sync status (medium) — socket protocol + CLI fallback
- [ ] 17. Native file icons (medium) — test existing provider, fix threading if needed

## 1. Quick Look and Get Info (small)

**Current state:** macOS uses `qlmanage -p` (Quick Look) and AppleScript to open Finder's Get Info window.
On Linux, both return "only available on macOS" errors. The menu and command palette already hide these
(`showInPalette: isMacOS()`, `build_menu_linux()` omits them), but the keyboard shortcuts (Space, Ctrl+I)
still fire and hit the error path.

**There is no standard cross-DE equivalent.** GNOME has "Sushi" (Space to preview) but it's not callable
from external apps. Every file manager has a Properties dialog (Alt+Enter) but there's no universal CLI.

**What to do:**
- On Linux, unbind Space from `file.quickLook` (or remap it to open the file, matching most Linux file
  managers where Space/Enter opens).
- Remove the Ctrl+I binding on Linux (it maps from `⌘I`).
- Ensure the error paths never fire — if a user somehow triggers these commands, return a no-op rather
  than an error toast.
- No Linux implementation needed for v1. These can be revisited later if a cross-DE preview API emerges.

**Files to check:**
- `apps/desktop/src/lib/commands/command-registry.ts` — shortcut definitions for `file.quickLook` and `file.getInfo`
- `apps/desktop/src-tauri/src/commands/ui.rs` — the `quick_look()` and `get_info()` Rust functions
- `apps/desktop/src/routes/+page.svelte` — `handleCommandExecute` cases for these commands

## 2. Accent colors via XDG Desktop Portal (medium)

**Current state:** `accent_color_linux.rs` reads GNOME-only `gsettings get org.gnome.desktop.interface accent-color`.
Works on GNOME 47+ (~40% of Linux desktop users). KDE Plasma (~25%) is not covered. XFCE, Cinnamon, Sway
have no accent color concept (brand gold fallback is correct for them).

**The fix:** Replace the GNOME-only `gsettings` call with the XDG Desktop Portal D-Bus call, which works
on both GNOME and KDE with a single code path (~65% coverage):

```
D-Bus call:
  Bus:       session bus
  Service:   org.freedesktop.portal.Desktop
  Object:    /org/freedesktop/portal/desktop
  Interface: org.freedesktop.portal.Settings
  Method:    ReadOne
  Args:      ("org.freedesktop.appearance", "accent-color")
  Returns:   (f64, f64, f64) — sRGB values in [0, 1]
```

Test from command line:
```bash
dbus-send --print-reply --dest=org.freedesktop.portal.Desktop \
  /org/freedesktop/portal/desktop \
  org.freedesktop.portal.Settings.ReadOne \
  string:'org.freedesktop.appearance' string:'accent-color'
```

**Fallback chain:** XDG Portal D-Bus → `gsettings` (for older GNOME without portal) → brand gold `#d4a006`.

**Live updates:** The portal emits a `SettingChanged` D-Bus signal on the same interface. Subscribe to it
for live accent color changes (analogous to macOS `NSSystemColorsDidChangeNotification`).

**Rust implementation:** Use the `zbus` or `dbus` crate to make the D-Bus call. Convert `(r, g, b)` floats
to `#rrggbb` hex string.

**Which DEs support the portal `accent-color` key:**

| DE | Portal backend | Supported |
|----|---------------|-----------|
| GNOME 47+ | xdg-desktop-portal-gnome | Yes |
| KDE Plasma 5.23+ | xdg-desktop-portal-kde | Yes |
| XFCE | xdg-desktop-portal-xapp | No |
| Cinnamon | xdg-desktop-portal-xapp | No |
| Sway | xdg-desktop-portal-wlr | No |

**Files to modify:**
- `apps/desktop/src-tauri/src/accent_color_linux.rs` — replace `gsettings` with D-Bus call
- `apps/desktop/src-tauri/Cargo.toml` — add `zbus` dependency (if not already present)

## 3. Appearance settings opener (tiny)

**Current state:** `permissions_linux.rs` runs `xdg-open gnome-control-center` which is wrong — `xdg-open`
expects a URL or file path, not a program name. Also only attempts GNOME with no DE detection.

**Background for agents:** GNOME, KDE, XFCE, and Sway are desktop environments (DEs) — different "shells"
for Linux. Detect which is running via the `$XDG_CURRENT_DESKTOP` environment variable:

| DE | `$XDG_CURRENT_DESKTOP` | Open appearance settings |
|----|----------------------|------------------------|
| GNOME | `GNOME` or `ubuntu:GNOME` | `gnome-control-center appearance` |
| KDE | `KDE` | `systemsettings kcm_lookandfeel` |
| XFCE | `XFCE` | `xfce4-appearance-settings` |
| Sway | `sway` | No GUI — return descriptive error |

Use `contains()` on the uppercased env var (Ubuntu sets it to `ubuntu:GNOME`).

**What to do:**
- Replace the broken `xdg-open` call with DE-specific commands.
- Call the settings binary directly with the subpage argument so the right panel opens immediately.
- For unknown DEs, return a helpful error: "Appearance settings not available for your desktop environment."

**Files to modify:**
- `apps/desktop/src-tauri/src/permissions_linux.rs` — `open_appearance_settings()` function

## 4. Volume chooser shortcuts and key naming on Linux (small)

**Current state:** Several shortcut issues on Linux:

1. **Volume chooser shortcuts:** On macOS, `⌘F1` opens the left volume chooser and `⌘F2` opens the right
   one. On Linux, `⌘` maps to `Ctrl`, so these become `Ctrl+F1` / `Ctrl+F2`. Standard Linux convention
   uses `Alt+Fx` for panel switching (like Midnight Commander's `Alt+F1`/`Alt+F2`). The current mapping
   feels wrong on Linux.

2. **Right volume chooser has no shortcut in command-registry:** `pane.rightVolumeChooser` has an empty
   shortcuts array `[]`. On macOS, `⌘F2` works via the native menu accelerator (`menu.rs` line ~524),
   bypassing the command registry. On Linux, this path also works for the menu but the command registry
   gap means it won't appear in the command palette or shortcut settings.

3. **F1 is hard-coded in `DualPaneExplorer.svelte`:** The `handleFunctionKey()` function (line ~616)
   intercepts bare `F1` and toggles the left volume chooser. On Linux, bare `F1` should NOT trigger the
   volume chooser — it conflicts with the common "help" convention, and the modifier key should be required.

4. **F2 doesn't trigger rename on Linux:** `file.rename` is bound to `['F2', '⇧F6']` in command-registry.
   Since `handleFunctionKey()` only checks for `F1` and returns false for other keys, F2 should fall
   through to the normal shortcut handler. Investigate why rename doesn't fire — it may be a menu
   accelerator conflict or an event propagation issue in the Linux menu system.

5. **'Win' label:** `formatKeyCombo()` in `key-capture.ts` labels the `metaKey` as `'Win'` on non-macOS.
   Standard Linux terminology is `'Super'`.

**What to do:**
- Add `Alt+F1` / `Alt+F2` as shortcuts for left/right volume chooser on Linux (in command-registry and
  the menu). Keep `⌘F1` / `⌘F2` on macOS.
- Add `pane.rightVolumeChooser` to the command registry shortcuts array (currently empty).
- Guard the bare `F1` interception in `handleFunctionKey()` to macOS only, or require the modifier key.
- Debug why F2 doesn't trigger rename on Linux — likely related to the function key handling or menu
  accelerator registration. Fix whatever blocks it.
- Change `'Win'` to `'Super'` in `formatKeyCombo()`.

**Files to modify:**
- `apps/desktop/src/lib/commands/command-registry.ts` — volume chooser shortcuts, platform-aware
- `apps/desktop/src/lib/file-explorer/pane/DualPaneExplorer.svelte` — `handleFunctionKey()` guard
- `apps/desktop/src/lib/shortcuts/key-capture.ts` — `formatKeyCombo()` line with `'Win'`
- `apps/desktop/src-tauri/src/menu.rs` — menu accelerators for volume chooser on Linux

## 5. GTK menu mnemonics (small)

**Current state:** Menu labels are plain strings (`"File"`, `"Edit"`, `"View"`). No Alt+key navigation.

**Good news:** Tauri's `muda` library already supports mnemonics. Prefix the mnemonic character with `&`:
`"&File"` → GTK renders as **F**ile with Alt+F to activate. Muda auto-converts `&` to GTK's `_` syntax
and enables underline rendering. Mnemonics are ignored on macOS, so the same strings can be used on both
platforms (but since we build menus separately per platform, just update `build_menu_linux()`).

**What to do:**
- Update all menu labels in `build_menu_linux()` to include `&` before the mnemonic character.
- Ensure no mnemonic clashes within the same menu level.
- Suggested top-level mnemonics: `&File`, `&Edit`, `&View`, `&Go`, `&Help`.
- For submenu items, pick non-clashing characters within each menu.

**Files to modify:**
- `apps/desktop/src-tauri/src/menu.rs` — `build_menu_linux()` function, label strings only

## 6. Linux-specific error messages (small)

**Current state:** Several user-facing error strings reference macOS concepts that would confuse Linux users:
- "Quick Look is only available on macOS" / "Get Info is only available on macOS"
- "Full Disk Access" references in permissions code
- "Open System Preferences" / "Privacy & Security" references
- Stub error messages like "Volumes not available on this platform"

**What to do:**
- Audit all error strings in `src-tauri/src/stubs/` — replace macOS terminology with generic text.
- Audit `permissions_linux.rs` — remove Full Disk Access references (Linux has no app sandboxing;
  `check_full_disk_access()` already returns true, but any error text should be generic).
- For features genuinely unavailable on Linux, use neutral language: "This feature is not available on
  your platform" rather than mentioning macOS.
- Check the frontend too — search for "Finder", "macOS", "System Preferences" in `.ts`/`.svelte` files.

**Files to check:**
- `apps/desktop/src-tauri/src/stubs/*.rs` — all stub files
- `apps/desktop/src-tauri/src/permissions_linux.rs`
- `apps/desktop/src-tauri/src/commands/ui.rs` — error return strings
- `apps/desktop/src/lib/` — search for macOS-specific UI text

## 7. Credential storage resilience (medium)

**Current state:** `keychain_linux.rs` uses the `keyring` crate (Secret Service D-Bus API → GNOME Keyring /
KDE Wallet). Works on ~90% of desktop Linux. When no secret service is running, the crate returns an error
that propagates to the user.

**The `keyring` crate does NOT have a file-based backend in its stable release (v3).** The upcoming v4
(currently at RC3, Feb 2026) adds `db-keystore` — an encrypted SQLite-based credential store via
`keyring::use_sqlite_store()`. This is an unconditional dependency in v4, not a feature flag.

**Recommended approach:**
1. Try `keyring` v3 with Secret Service first (covers GNOME Keyring, KDE Wallet — ~90% of desktop Linux).
2. On failure, fall back to `keyring`'s `linux-native` feature (`keyutils` — kernel keyring, no D-Bus
   needed). Note: `keyutils` credentials are session-scoped and don't persist across reboots without
   Secret Service.
3. If both fail, implement a simple encrypted file store at `~/.local/share/cmdr/credentials.enc` using
   the `cocoon` crate (256-bit encryption, small dependency) or similar.
4. On first fallback to file store, show a one-time info toast: "Credentials stored locally (no system
   keyring detected)."
5. Never fail silently — if all methods fail, show the error clearly.

**Alternative:** Adopt `keyring` v4 RC which bundles `db-keystore` (SQLite-backed). This is the direction
the ecosystem is heading, but using an RC in production has risks. Evaluate stability before choosing.

**Sources:** [keyring crate](https://crates.io/crates/keyring),
[keyring v4 docs](https://docs.rs/keyring/4.0.0-rc.3/keyring/),
[db-keystore](https://docs.rs/db-keystore/latest/db_keystore/),
[cocoon](https://crates.io/crates/cocoon)

**Files to modify:**
- `apps/desktop/src-tauri/src/network/keychain_linux.rs`
- `apps/desktop/src-tauri/Cargo.toml` — add fallback dependency (`cocoon` or `keyring` v4)

## 8. Media eject

Deferred to "Later" task list. We don't have this on macOS either.

## 9. High-DPI support

**Current state:** Tauri/WebKitGTK handles DPI scaling automatically via `GDK_SCALE` and `GDK_DPI_SCALE`
environment variables. The app looked fine in a UTM VM with Ubuntu.

**What to do:**
- No code changes needed.
- If you want extra confidence, test at 1.5x fractional scaling (common on Linux laptops) — that's where
  rendering artifacts usually appear. Set `GDK_SCALE=1.5` before launching.
- Low priority — likely already works.

## 10. Trash implementation (medium)

**Current state:** Spec Milestone 1a in `support-linux.md` says to use the `trash` crate, but the code
still uses stubs on Linux. Users cannot delete files via trash.

**What to do** (from `support-linux.md` Milestone 1a):
- Use the [`trash`](https://crates.io/crates/trash) crate (pure Rust, implements the
  [FreeDesktop.org trash spec](https://specifications.freedesktop.org/trash-spec/latest/)). It handles
  `.trashinfo` metadata, collisions, and cross-volume trash directories. Well-maintained (1M+ downloads),
  also supports macOS so it could eventually unify both platforms.
- Interface: `move_to_trash_sync(path: &Path) -> Result<(), String>` — same signature as today.
- Add `trash` as a `[target.'cfg(target_os = "linux")'.dependencies]` dependency (or unconditional if we
  want to unify macOS too — decide during implementation).
- Implement `move_to_trash_sync()` for Linux using `trash::delete()`.
- Update `supports_trash_for_fs_type()` for Linux filesystem types (ext4, btrfs, xfs, zfs → true;
  nfs, cifs, fuse.sshfs → false).
- Wire up in `lib.rs` command registration (replace stub).

**Files to modify:**
- `apps/desktop/src-tauri/Cargo.toml` — add `trash` dependency
- `apps/desktop/src-tauri/src/write_operations/trash.rs` — add Linux implementation
- `apps/desktop/src-tauri/src/lib.rs` — command registration
- `apps/desktop/src-tauri/src/stubs/write_operations.rs` — narrow cfg gate

## 11. Network filesystem detection for copy (medium)

**Current state:** `is_network_filesystem()` is planned in `support-linux.md` Milestone 1b but not connected
to `copy_strategy.rs`. Without it, `copy_file_range` might be used on NFS/CIFS mounts where chunked copy
would be more reliable.

**Use `/proc/self/mountinfo`, not `statfs()`.** Both approaches were evaluated:

- **`/proc/self/mountinfo`** (recommended): single file read, correctly identifies FUSE-based network
  mounts (for example, `fuse.sshfs`, `fuse.rclone`) via fstype substrings, never blocks on hung mounts,
  doesn't trigger automounts. Richer than `/proc/mounts` — includes mount IDs, parent relationships,
  and unambiguous field parsing. This is what `findmnt` (the gold standard) uses.
- **`statfs()`**: per-path syscall, but all FUSE mounts collapse to a single `FUSE_SUPER_MAGIC` (can't
  distinguish `sshfs` from `ntfs-3g`), can **block for minutes** on hung NFS mounts, and triggers
  automounts as a side effect.

**Why this is the right call:** The codebase already needs `/proc/self/mountinfo` parsing for volume
discovery (sidebar mount listing). Reusing that same parsed data for network FS classification is zero
additional I/O. Parse once, derive both the volume list and a mount-point → filesystem-kind map.

**What to do:**
- Build a shared `/proc/self/mountinfo` parser (or extend the one in `volumes_linux/` if it exists).
- Classify fstypes: `nfs`, `nfs4`, `cifs`, `smbfs`, `9p` → network. `fuse.sshfs`, `fuse.rclone`,
  `fuse.s3fs`, `fuse.gvfsd-fuse` → network. `fuse.ntfs-3g`, `fuseblk` → local. Unknown `fuse.*` →
  conservative fallback (treat as potentially network, use chunked copy).
- Wire into `copy_strategy.rs` to select chunked copy for network filesystems.
- For a given path, find the longest-prefix-matching mount point in the map.

**Files to modify:**
- `apps/desktop/src-tauri/src/write_operations/copy_strategy.rs`
- `apps/desktop/src-tauri/src/volumes_linux/mod.rs` — shared `/proc/self/mountinfo` parser

## 12. File watching E2E verification (small)

**Current state:** Both file watching systems already work on Linux:

1. **File pane watcher** (UI refresh): Uses `notify` crate → inotify. Works, with one known edge case:
   rename-move doesn't always fire an inotify event (handled by a manual refresh fallback already in code).
2. **Drive indexing watcher** (background indexing): Already implemented in `indexing/watcher.rs` using
   `notify` with `RecursiveMode::Recursive` and synthetic event IDs.

**No journal replay on Linux.** inotify has no persistent log like FSEvents. On startup, Cmdr always does
a full rescan (keeping the old SQLite DB for instant enrichment). This is the standard Linux approach
(same as `mlocate`, KDE Baloo).

**What to do:**
- Add a Linux E2E test that:
  1. Navigates to a test directory.
  2. Reads the file list.
  3. Creates a new subdirectory (via `mkdir` shell command, not through the app).
  4. Waits ~4 seconds.
  5. Verifies the new directory appears in the file list.
- This confirms the `notify` → inotify path works end-to-end.
- No code changes needed for the watcher itself — just test coverage.

**Files to modify:**
- `apps/desktop/test/e2e-linux/` — add a new test file

## 13. MTP USB permissions (small)

**What are udev rules:** When you plug in a USB device on Linux, the `udev` daemon applies rules (text
files in `/etc/udev/rules.d/`) to set permissions, create device nodes, and trigger actions. Think of it
as Linux's version of macOS IOKit for device management, but configurable via files.

**Good news: no custom rules needed.** The `libmtp` package (available on all major distros) ships its
own comprehensive rules file (`69-libmtp.rules`) covering hundreds of MTP devices. Cmdr uses MTP via
`mtp-rs` (pure Rust), but the USB access permissions still come from the system-level udev rules.

**What to do:**
- Declare `libmtp` as a recommended/suggested package dependency in `.deb`/`.rpm` packaging.
- Document that the user must be in the `plugdev` group (standard on Ubuntu/Fedora).
- In the MTP connection error path, detect permission errors and show a helpful message:
  "Cannot access USB device. Make sure `libmtp` is installed and you're in the `plugdev` group."
- No udev rules file to ship — rely on `libmtp`'s.

**Files to modify:**
- MTP error handling in `src-tauri/src/mtp/` — add Linux-specific error guidance
- Packaging config (when packaging is set up) — add `libmtp` as recommended dependency

## 14. SMB mounting completion (large)

**Current state:** ~90% done. mDNS discovery, share listing, credential storage, and `gio mount` are all
implemented for Linux in `src-tauri/src/network/`.

**Remaining gaps:**
1. **`gio mount` password piping** — current approach (`echo '{pass}' | gio mount '{url}'`) works for most
   NAS/SMB servers but may fail on servers requiring interactive domain/username/password prompts.
2. **No `smbutil` fallback** — if `smb-rs` fails with a protocol error on an old Samba server, macOS falls
   back to `smbutil view`. On Linux, could fall back to `smbclient -L` (from `samba-client` package).
3. **Testing** — needs validation on GNOME, KDE, and systems without GVFS.

**What to do:**
- Test the existing `mount_linux.rs` implementation against a real SMB share.
- Add a `smbclient -L` fallback for share listing when `smb-rs` fails.
- Improve the `gio mount` interaction to handle multi-field auth prompts (or document the limitation).
- Add error handling for systems without GVFS: "SMB mounting requires GVFS. Install `gvfs-smb`."

**Files to check/modify:**
- `apps/desktop/src-tauri/src/network/mount_linux.rs`
- `apps/desktop/src-tauri/src/network/smb_client.rs` — add `smbclient` fallback

## 15. Custom drag image

**Current state:** macOS uses ObjC method swizzling on `WryWebView` to intercept drag operations and swap
the drag image. On Linux, WebKitGTK handles drag-and-drop internally with no hook point.

**Not feasible with current Tauri/wry architecture.** There is no clean API to customize the drag preview
on WebKitGTK. Would require upstream wry changes.

**What to do:**
- Accept the OS default drag image on Linux.
- The DOM overlay already provides in-window feedback for pane-to-pane drags.
- Defer to "Later" — revisit if wry adds a Linux drag image API.

## 16. Dropbox sync status (medium)

**Current state:** macOS uses `SF_DATALESS` kernel flags and NSURL resource keys — Apple-specific APIs.
On Linux, the stub returns an empty map.

**Linux Dropbox mechanism:** Dropbox exposes sync status via an **undocumented socket protocol** at
`~/.dropbox/command_socket`. Send a file path, receive status strings ("up to date", "syncing", etc.).
This is what the Nautilus Dropbox extension uses. Alternative: shell out to `dropbox filestatus <path>`.

**Risk:** The socket protocol is unofficial and could break with Dropbox updates. That said, the Nautilus
extension has relied on it for years and it remains stable in practice. Dropbox on Linux is niche but
Cmdr users who use it will notice missing sync icons.

**What to do:**
- Check if `~/.dropbox/command_socket` exists (Dropbox running).
- Implement socket-based status queries for batch efficiency.
- Map status strings to the existing `SyncStatus` enum.
- Fallback: if socket fails, try `dropbox filestatus <path>` CLI.
- Handle gracefully when Dropbox isn't installed (return empty map, same as today).

**Files to modify:**
- `apps/desktop/src-tauri/src/file_system/sync_status.rs` — add Linux implementation behind `cfg` gate

## 17. Native file icons (medium)

**Current state:** The `file_icon_provider` crate is already a dependency and wired up in `icons.rs`. On
Linux it uses GTK to query the system icon theme. It likely already partially works.

**Known issues:**
1. **Main thread constraint:** `file_icon_provider` on Linux must be called from the main thread. The
   current code uses rayon for parallel icon fetching (`refresh_icons_for_directory()`), which may conflict.
2. **Icon quality:** Varies across DEs and themes. May return generic fallbacks instead of rich themed icons.

**What to do:**
1. Test the current `file_icon_provider` on Linux — see what icons it returns. It may just work.
2. If threading is an issue, restructure icon fetching to dispatch to the main thread on Linux
   (for example, `tauri::async_runtime::spawn` on the main thread, or use `glib::MainContext`).
3. If icon quality is poor, consider the `freedesktop-icons` crate as a pure-Rust alternative that
   reads XDG icon themes without GTK dependency.
4. The macOS-specific `macos_icons.rs` module (app-bundle icons) can stay macOS-only — Linux doesn't
   have app bundles.

**Files to check/modify:**
- `apps/desktop/src-tauri/src/icons.rs` — icon fetching logic, rayon parallelism
- `apps/desktop/src-tauri/src/macos_icons.rs` — verify it stays `cfg(target_os = "macos")` gated
- `apps/desktop/src-tauri/Cargo.toml` — potentially add `freedesktop-icons` if needed

## Verification

After implementing all sections, run:
- `./scripts/check.sh` — all checks must pass
- `./scripts/check.sh --check rust-tests-linux` — Linux-targeted Rust tests
- `./scripts/check.sh --check cfg-gate` — verify macOS-only crate imports are properly gated
