# Linux support plan

## Goal

Make Cmdr a fully functional file manager on Linux. Today the app compiles and runs on Linux (used for
E2E testing in CI), but all platform-specific features are stubbed out. This plan turns those stubs into real
implementations, milestone by milestone, so each milestone delivers a shippable improvement.

## Current state

- The app builds on Linux via `pnpm tauri build --no-bundle` (CI does this already).
- A stub layer in `src-tauri/src/stubs/` provides no-op or hardcoded responses for all macOS-gated commands.
- The frontend is fully cross-platform — no changes needed.
- 56 E2E tests run on Linux via WebDriverIO + tauri-driver + xvfb.
- The `cfg-gate` Go check enforces that macOS-only crate imports are properly gated.

## Architecture approach

Each macOS-gated module (volumes, write operations, indexing, MTP, network) currently lives in a
top-level `mod` guarded by `#[cfg(target_os = "macos")]`, with a corresponding stub in `stubs/`. The
Linux port follows the same pattern: create a Linux (or platform-neutral) implementation behind
`#[cfg(target_os = "linux")]`, keeping the public interface identical so the frontend doesn't change.

Where possible, prefer **platform-neutral pure Rust** over Linux-specific code — for example, the `trash`
crate supports both macOS and Linux and would replace both the current ObjC wrapper and a custom XDG
implementation.

### cfg-gate strategy

The codebase currently has two tiers: `#[cfg(target_os = "macos")]` for real implementations and
`#[cfg(not(target_os = "macos"))]` for stubs. Adding Linux creates a three-tier system:

1. **`#[cfg(target_os = "macos")]`** — macOS-specific code (ObjC, FSEvents, etc.). Unchanged.
2. **`#[cfg(target_os = "linux")]`** — Linux-specific code (`copy_file_range`, `/proc/mounts`, etc.).
3. **`#[cfg(not(any(target_os = "macos", target_os = "linux")))]`** — remaining stubs for any other
   platform. These shrink as milestones land.

For platform-neutral code (for example, the `trash` crate), use `#[cfg(any(target_os = "macos", target_os = "linux"))]`
or even make it unconditional if it compiles everywhere.

**Where to update when adding a Linux implementation:**
- The implementation module itself (new file or cfg-gated function body).
- `lib.rs` — add a `#[cfg(target_os = "linux")]` entry in `invoke_handler!` and narrow the stub
  entry to `#[cfg(not(any(target_os = "macos", target_os = "linux")))]`.
- `commands/mod.rs` — same pattern if the command module is gated there.
- The stub file — narrow its gate to `#[cfg(not(any(target_os = "macos", target_os = "linux")))]`.

The `cfg-gate` Go check (`./scripts/check.sh --check cfg-gate`) validates that macOS-only crate
imports are properly gated. It doesn't currently check Linux-only crates, but the same discipline
applies: Linux-only imports (for example, `libc::copy_file_range`) must be behind `#[cfg(target_os = "linux")]`.

### Directory watching

The `notify` crate (already a dependency) abstracts over platform-specific file watchers (FSEvents on
macOS, inotify on Linux). The file pane's directory watcher should already work on Linux through this
abstraction. Assume it works — if it doesn't, add it to the follow-up list.

## Milestones

### Milestone 1: Core file operations (trash, copy, move)

**Why first:** Without these, you can browse but not operate on files. This is the minimum for a usable
file manager.

#### 1a: Trash

**Current macOS impl:** ObjC `NSFileManager.trashItemAtURL` in `write_operations/trash.rs`.

**Linux approach:** Use the [`trash`](https://crates.io/crates/trash) crate (pure Rust, implements the
[FreeDesktop.org trash spec](https://specifications.freedesktop.org/trash-spec/latest/)). It handles
`.trashinfo` metadata, collisions, and cross-volume trash directories. This is a well-maintained crate
(1M+ downloads) that also supports macOS, so it could eventually replace the ObjC code too.

**Interface:** `move_to_trash_sync(path: &Path) -> Result<(), String>` — same signature as today.

**Scope:**
- Add `trash` crate as a `[target.'cfg(target_os = "linux")'.dependencies]` dependency (or unconditional
  if we want to unify macOS too — decide during implementation).
- Implement `move_to_trash_sync()` for Linux using `trash::delete()`.
- Update `supports_trash_for_fs_type()` for Linux filesystem types (ext4, btrfs, xfs, zfs → true;
  nfs, cifs, fuse.sshfs → false).
- Wire up in `lib.rs` command registration (replace stub).

#### 1b: Copy engine

**Current macOS impl:** FFI to `copyfile(3)` in `macos_copy.rs` with progress callbacks, plus APFS
`clonefile` for instant copies. Chunked fallback for network filesystems.

**Linux approach:** Two-tier strategy matching the macOS design:
1. **Fast path:** `copy_file_range(2)` syscall (Linux 4.5+) — kernel-side copy, supports reflinks on
   btrfs/XFS (equivalent to APFS clonefile). Falls back to in-kernel data copy on other filesystems.
   Use via `libc::copy_file_range()`.
2. **Fallback:** The existing chunked copy (`chunked_copy.rs`) already works cross-platform. Use it for
   network filesystems and when `copy_file_range` isn't available.

**Network filesystem detection:** Parse `/proc/mounts` to detect nfs, cifs, fuse.sshfs, smbfs.
The macOS impl uses `statfs.f_fstypename`; Linux has `statfs.f_type` (magic numbers) or
`/proc/mounts` (human-readable). Prefer `/proc/mounts`. Build this as a shared utility — milestone 2
(volume discovery) reuses the same `/proc/mounts` parsing for filesystem type detection.

**Metadata preservation:**
- Permissions: `std::fs::set_permissions()` (already cross-platform).
- Timestamps: `filetime` crate (already a dependency, cross-platform).
- Extended attributes: `xattr` crate (already a dependency, supports Linux).
- ACLs: `exacl` crate (already a dependency, supports Linux POSIX ACLs).
- The safe-overwrite pattern (temp + rename) is filesystem-agnostic — works as-is.

**Scope:**
- Create `linux_copy.rs` with `copy_file_range` wrapper + progress tracking.
- Implement `is_network_filesystem()` for Linux via `/proc/mounts` parsing.
- Update `copy_strategy.rs` to select Linux-native copy on `#[cfg(target_os = "linux")]`.
- The move engine (`move_op.rs`) uses `fs::rename` + cross-fs staging — already platform-neutral.

#### 1c: Delete

**Current impl:** `delete_files_with_progress()` uses `fs::remove_file` / `fs::remove_dir_all` —
already cross-platform. Verify it works on Linux and add E2E coverage.

### Milestone 2: Volume and location discovery

**Depends on:** Milestone 1b's `/proc/mounts` parsing utility (reuse it for filesystem type detection).

**Why second:** The sidebar is broken without this — users only see root, home, and three hardcoded dirs.

**Current macOS impl:** `volumes/mod.rs` uses `NSFileManager.mountedVolumeURLs` for volume enumeration,
`NSURL` resource keys for volume properties, and `libc::statfs` for filesystem type.

**Linux approach:**
- **Mount enumeration:** Parse `/proc/mounts` (or `/etc/mtab`) — each line is
  `device mountpoint fstype options`. Filter out virtual filesystems (proc, sysfs, devpts, tmpfs,
  cgroup, etc.).
- **Favorites:** Same approach as macOS — hardcode Desktop, Documents, Downloads from `dirs` crate.
  Detect XDG user directories via `dirs::home_dir()` + standard paths.
- **Cloud drives:** Check common locations: `~/Dropbox`, `~/Google Drive`, `~/.local/share/Nextcloud`,
  `~/OneDrive`. Could also parse `~/.config/` for provider configs.
- **Removable volumes:** Parse `/sys/class/block/*/removable` or use `lsblk --json` for USB drives,
  SD cards. Alternatively, watch `/run/media/$USER/` (systemd automount point).
- **Space info:** `libc::statvfs()` — already works in stubs.
- **Filesystem type:** Read from `/proc/mounts` directly (human-readable, no magic numbers).
- **Ejectability:** Volumes under `/run/media/` or flagged removable in sysfs.

**Volume watcher:** Use `inotify` on `/proc/mounts` (Linux signals mount changes by making this file
readable) or poll it on a timer. Much simpler than the macOS `DiskArbitration` approach.

**Scope:**
- Create `volumes/linux.rs` (or a platform-neutral implementation if simple enough).
- Implement `list_locations()` returning the same `LocationInfo` struct.
- Implement mount watcher using inotify on `/proc/mounts`.
- Wire up in `lib.rs`.

### Milestone 3: Drive indexing

**Why third:** Indexed search and recursive directory sizes are a power feature, not a blocker for basic
usage.

**Current macOS impl:** `cmdr-fsevent-stream` provides file-level FSEvents with event IDs for cold-start
replay. The indexing system does a full scan, then switches to live event processing.

**Linux approach — `fanotify`:**
- `fanotify` (Linux 5.1+ with `FAN_REPORT_FID`) provides system-wide filesystem event notifications at
  file granularity, similar to FSEvents. It doesn't require per-directory watches like `inotify`.
- Requires `CAP_SYS_ADMIN` or root for `FAN_REPORT_FID` mode — may need a privileged helper or user
  prompt. Alternative: fall back to `inotify` with dynamic watch management.
- No event IDs like FSEvents — use filesystem timestamps + full rescan on startup (the scan is fast
  since SQLite already has the previous state for diffing).

**Alternative — `inotify` with watch pool:**
- Add watches on directories as they're accessed. Cap at ~64k watches.
- On startup, do a full rescan (comparing against SQLite state).
- Simpler than `fanotify` but less comprehensive (won't catch changes in unwatched subtrees).

**Recommendation:** Start with `inotify` (simpler, no privilege requirements, covers the common case).
Add `fanotify` later as an optional enhancement for users who want system-wide coverage.

**Scope:**
- Create `indexing/linux_watcher.rs` implementing the `DriveWatcher` trait.
- Use `inotify` crate (or `notify` crate which abstracts over inotify/fanotify/kqueue).
- Adapt the scan + live event loop to work without FSEvents event IDs.
- Handle Linux-specific path normalization (bind mounts, overlayfs).

### Milestone 4: MTP (Android device support)

**Why fourth:** Niche feature (Android users only), and the underlying crates already support Linux.

**Current macOS impl:** `mtp-rs` (pure Rust MTP) + `nusb` (pure Rust USB). The only macOS-specific
part is `macos_workaround.rs` (killing `ptpcamerad` which grabs exclusive USB access).

**Linux approach:**
- `nusb` already supports Linux via udev. `mtp-rs` is pure Rust.
- The `ptpcamerad` workaround isn't needed on Linux.
- Main concern: USB device permissions. Linux requires either root or a udev rule granting access to
  the MTP device class. Document this and provide a `.rules` file.

**Scope:**
- Remove `#[cfg(target_os = "macos")]` gate from the MTP module, replace with
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`.
- Conditionally compile `macos_workaround.rs` only on macOS.
- Add udev rules file and documentation for Linux USB permissions.
- Test with a real Android device on Linux.

### Milestone 5: Network/SMB browsing

**Why last:** Large feature surface, and users can manually mount SMB shares in the meantime.

**Current macOS impl:** mDNS discovery (`mdns-sd`), SMB share listing (`smb` + `smb-rpc` crates),
Keychain auth (`security-framework`), native mounting (`NetFSMountURLSync`).

**Linux approach:**
- **mDNS discovery:** `mdns-sd` crate is pure Rust — should work on Linux as-is. Verify and remove
  the platform gate.
- **Share listing:** `smb` and `smb-rpc` crates are pure Rust — should work on Linux. The
  `smbutil` fallback needs replacement (macOS-specific binary); use direct SMB protocol instead.
- **Credential storage:** Replace Keychain with `libsecret` (GNOME Keyring / KDE Wallet) via the
  `keyring` crate, or use a simpler encrypted file-based store.
- **Mounting:** Use `mount.cifs` (requires root or sudo) or `gio mount` (GVFS, user-space).
  Alternatively, use `smbclient` for browsing without mounting. This is the hardest part — Linux
  SMB mounting is fragmented across distros.

**Scope:**
- Test `mdns-sd` and `smb`/`smb-rpc` on Linux, remove gates if they work.
- Implement credential storage via `keyring` crate (cross-platform).
- Implement mount via `gio mount` (user-space, no root needed, works on most desktop distros).
- Replace `smbutil` fallback with direct protocol approach.
- Handle the case where GVFS isn't available (server distros) — show a helpful error.

### Milestone 6: Polish and small features

**Scope:**
- **Accent color:** Read GTK/Qt theme color. Use `gsettings get org.gnome.desktop.interface accent-color`
  (GNOME 47+) or fall back to the hardcoded brand gold (current stub behavior).
- **Permissions:** `check_full_disk_access()` → always return true (Linux doesn't have macOS-style
  app sandboxing). Remove the "Open Privacy Settings" command on Linux.
- **Drag image:** The `drag_image_detection` and `drag_image_swap` modules are macOS-only (custom
  drag preview rendering via ObjC). On Linux, leave these gated — Tauri/WebKitGTK provides a default
  drag image from the dragged element. This is good enough for initial Linux support.
- **File icons:** The `macos_icons` module is macOS-only (uses `NSWorkspace` icon APIs). For Linux,
  don't implement native icon lookup — use a simple fallback (generic file/folder icons based on
  extension). This is sufficient for launch.
- **Filename validation:** Add Linux-specific rules. Linux is more permissive than macOS: only `/`
  and null byte (`\0`) are forbidden in filenames. The 255-byte name length limit and 4096-byte path
  length limit also apply. `.` and `..` are reserved by the filesystem.
- **Keyboard shortcuts:** Already handled at runtime (`isMacOS()` → Ctrl vs Cmd display).

## E2E test strategy

The existing 56 Linux E2E tests cover browsing, keyboard navigation, mouse interaction, dialogs,
settings, and the file viewer. They run on the stub layer today. As each milestone lands, these tests
should start exercising real implementations.

**Additional E2E tests needed per milestone:**

| Milestone | Tests to add |
|-----------|-------------|
| 1: File ops | Copy file + verify content, copy directory recursively, move (same fs), move (cross fs if possible), trash + verify in XDG trash dir, delete, conflict resolution (skip, overwrite, rename), cancel mid-copy |
| 2: Volumes | Sidebar shows real mounts, removable volume appears/disappears, space info matches `df` |
| 3: Indexing | Recursive size appears after indexing, new file detected by watcher, search returns indexed results |
| 4: MTP | Hard to E2E (needs USB device). Unit test the connection manager with mock USB. |
| 5: Network | mDNS discovers a test Samba container, list shares, mount + browse + unmount |
| 6: Polish | Accent color reads system theme (on GNOME), filename validation rejects `/` |

**Test infrastructure:** The existing Docker + xvfb setup can be extended. For milestone 5, add a
Samba container to the test docker-compose. For milestone 4, use USB passthrough or mock at the
`nusb` level.

## Follow-up (not in scope for this plan)

These are real gaps but not blockers for a usable Linux release. Track them separately.

- **Cloud sync status:** The `sync_status` module detects Dropbox/iCloud/OneDrive sync state on macOS
  via extended attributes. Linux Dropbox uses different xattrs (`user.com.dropbox.attrs`). Needs
  research into what each provider exposes on Linux. Keep the current non-macOS stub (returns empty
  map) until this is figured out.
- **Custom drag images:** Replace the OS-default drag preview with a custom rendered image matching
  the macOS experience. Low priority — the default works fine.
- **Native file icons:** Query `xdg-mime` or read icon themes to show file-type-specific icons instead
  of generic file/folder icons. Nice-to-have polish.
- **Directory watcher issues:** If the `notify` crate's inotify backend doesn't work correctly for
  directory listing updates, implement a fix. (We assume it works — see architecture approach section.)

## Risks and open questions

1. **`copy_file_range` availability:** Requires Linux 4.5+. All major distros ship 5.x+ kernels now,
   but verify. The chunked fallback handles older kernels.
2. **fanotify permissions:** `FAN_REPORT_FID` needs `CAP_SYS_ADMIN`. If we go this route, we need a
   privileged helper or a polkit prompt. Starting with `inotify` avoids this entirely.
3. **SMB mounting fragmentation:** `gio mount` works on GNOME/GTK desktops. KDE uses `kio`. Server
   distros have neither. We may need to support multiple backends or document requirements.
4. **inotify watch limits:** Default is 8192, tunable via `fs.inotify.max_user_watches`. Power users
   with huge trees will hit this. Document the sysctl and consider dynamic watch management.
5. **Distro matrix:** Ubuntu, Fedora, Arch are the priority. Test on at least these three.
6. **Packaging:** Tauri supports `.deb`, `.AppImage`, and `.rpm`. Need to pick what to ship and set up
   release CI. This plan doesn't cover packaging — it's a separate concern.

## Sequence for agent-driven implementation

Each milestone is self-contained and independently shippable. An agent should:

1. Read the relevant macOS implementation + stub + this plan.
2. Implement the Linux version behind `#[cfg(target_os = "linux")]`.
3. Ensure the `cfg-gate` check passes (`./scripts/check.sh --check cfg-gate`).
4. Add/update E2E tests.
5. Run `./scripts/check.sh --check rust-tests-linux` (compiles + tests with Linux target).
6. Run the full E2E suite (`pnpm test:e2e:linux:native` or Docker).
