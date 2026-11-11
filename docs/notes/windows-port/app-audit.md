# Windows port: app crate audit (`apps/desktop/src-tauri/`)

Scope: the app's Rust crate only (`crates/` and frontend/packaging/CI are separate audits). Static read of the tree at
`7a199a9705`, 2026-11-04, on macOS; nothing was compiled for a Windows target (no `x86_64-pc-windows-*` std is installed
here; `cargo-xwin` is). Crate liveness checked on crates.io the same day. Counts come from `grep` over `src/` and
include tests unless noted.

## Numbers

- `target_os = "macos"`: 1,156 hits in 188 files. `target_os = "linux"`: 269 in 54 files. `target_os = "windows"` /
  `cfg(windows)`: zero.
- `#[cfg(not(target_os = "macos"))]`: 120. `any(target_os = "macos", target_os = "linux")`: 194 in 36 files.
  `not(any(macos, linux))` (the "other OS" arm): 41 in 23 files. `cfg(unix)`: 47, `not(unix)`: 15.
- `std::os::unix`: 88 uses in 65 files. `libc::`: 238 in 52 files, while `libc` is a `cfg(unix)`-only dependency
  (`Cargo.toml:285`).
- IPC: 635 `#[specta::specta]` commands; 63 of them are defined two to four times, once per `cfg` arm (`show_in_finder`
  and `open_in_editor` four times each, the six permission commands and `get_accent_color` three times). `stubs/` holds
  50 more commands in 1,027 lines with hand-mirrored types.
- Heaviest files: `lib.rs` (60 macOS + 34 Linux cfgs in the setup hook), `operations_host/backend_suites/mod.rs` (57,
  tests), `menu/mod.rs` (52), `ipc.rs` (32 + 17), `menu/file_context_menu.rs` (31), `file_viewer/open_with_extract.rs`
  (24), `system_strings.rs` (23).
- Heaviest modules: `file_system/` (210 macOS / 41 Linux), `menu/` (150 / 0), `commands/` (149 / 25), `tools/` (81 /
  15), `operations_host/` (69 / 55, mostly test suites), `network/` (55 / 23).

## How platform dispatch is done today (six patterns)

1. **Sibling modules picked in `lib.rs`**: `accent_color.rs` + `accent_color_linux.rs` (`lib.rs:25-28`),
   `permissions.rs` + `permissions_linux.rs` (`lib.rs:124-127`), `volumes/` + `volumes_linux/` (`lib.rs:195-198`),
   `macos_icons.rs` + `linux_icons.rs`. The pairs share a fn set by convention only; nothing checks the signatures
   match. `volumes/` and `volumes_linux/` even mirror file names (`cloud`, `fs_type`, `ids`, `mounts`, `smb`,
   `watcher`).
2. **Module alias by `cfg`**: `use crate::volumes as platform;` / `use crate::volumes_linux as platform;`
   (`commands/volumes.rs:16-18`, `file_system/volume/mount_registration.rs:33-35`), and a type alias
   `type SmbMountInfo = crate::volumes::SmbMountInfo` / `volumes_linux::SmbMountInfo` (`file_system/mod.rs:342-345`).
   Duck-typed: a third OS gets "no `platform` in scope".
3. **`#[path]` swap**: `network/mod.rs:82-88` (`mount.rs` vs `mount_linux.rs`), `file_system/mod.rs:30`
   (`tags_io_off_macos.rs`).
4. **Per-platform IPC commands**: the same `#[tauri::command]` written once per arm, for example `show_in_finder` at
   `commands/file_actions.rs:59,69,77,91`, plus `clipboard.rs:233-279`, `quick_look.rs:121-135`,
   `file_system/drag.rs:89-179`. The IPC manifest mirrors it with three-way groups (`ipc.rs:800-817` accent color,
   `ipc.rs:904-936` permissions).
5. **`stubs/` for "neither macOS nor Linux"** (`lib.rs:205`, `stubs/mod.rs`): `volumes`, `network` (578 lines), `mtp`,
   `permissions`, `accent_color`, plus three `not(macos)` ones. Its invariant says the JSON shapes are "aligned by
   hand".
6. **Inline `cfg` in business logic**: the `lib.rs` setup hook (`lib.rs:298-720`), `secrets/mod.rs:144-170`,
   `icons/mod.rs:332-370`, `space_poller/mod.rs:514`, `file_system/index_provider.rs:87-121`, the menu state struct's
   per-field gates (`menu/mod.rs:279-306`, `495-602`).

The good models already in the tree: `managed_policy/source.rs` (trait `ManagedPrefsSource` with `CfPrefsSource`,
`PlistFileSource`, `NoManagedPrefs`), `secrets::SecretStore` (trait, backend picked once), and `file_system/watcher.rs`
(one `WatchStream` enum, FSEvents vs `notify`, one `arm()` entry point). `platform.rs::macos_at_least` is the "ask
unconditionally, no `cfg` at the caller" idea in miniature.

## Why Windows breaks: four failure classes

The "other OS" arm exists but has never been compiled, so it has rotted.

1. **The "other" arm doesn't compile on Windows.** `stubs/volumes.rs:194` calls `libc::statvfs`, and `libc` isn't a
   dependency there. The stubs' own doc names `libc` as a dep (its C.md, since removed with it).
2. **Two arms, no third (27 files).** Items defined for macOS and Linux with no fallback, in modules that are NOT gated
   off on Windows:
   - `licensing/device_id.rs:31,87` (`read_platform_uuid`).
   - `icons/mod.rs:118,125` (`fetch_icon_for_path`).
   - `volume_listing.rs:21-34,61` (`LocationInfo`, `list_locations`, `network_mount_under`).
   - `listing_host/mounts.rs:18` (`mount_of`).
   - `space_poller/mod.rs:514`, `usb_speed.rs`.
   - `file_system/mod.rs:160-210` and `342-366`.
   - `commands/volumes.rs:16-18` (gated `any(macos, linux)`, so it compiles out, but see class 4).
3. **Ungated Unix-only std or deps in cross-platform code.**
   - `file_viewer/source.rs:13`: `std::os::unix::fs::FileExt` (`read_at`); the viewer is ungated. On Windows it's
     `FileExt::seek_read`.
   - `listing_index_sizes/count/measure.rs:121`: `MetadataExt` for inode dedup. On Windows `file_index()` is still
     unstable, so it needs `GetFileInformationByHandle` (the `windows` crate).
   - `file_viewer/open_with_copies.rs:31`: ungated `MetadataExt` import.
   - **The manifest**: `Cargo.toml:304-337` declares `url`, `semver`, `plist`, `pulldown-cmark`, `ammonia`,
     `unicode-normalization`, `urlencoding`, `smb2`, and `mdns-sd` for `any(macos, linux)` only. Ungated modules use
     them: `whats_new/`, `file_viewer/markdown_*`, `search/matcher.rs`, `multi_rename/`, `redact/`, `ai/`, `logging/`,
     and `managed_policy/`. On Windows the lib wouldn't resolve these crates at all. This is the cheapest and
     highest-leverage fix in the whole list.
   - Tests that use `std::os::unix` without `cfg(unix)`: `usage/ledger.rs:121`, `agent/memory/outcomes.rs:146`,
     `ai/state.rs:258`, `recents/persistence.rs:178`, `data_wipe/execute.rs:307`. The test build breaks, the lib
     doesn't.
4. **`not(macos)` that means Linux, or `any(macos, linux)` that means "everywhere real".** These compile but behave
   wrong on Windows:
   - `intl/mod.rs:195` `posix_ui_locale` reads `LC_ALL`/`LANG`, which Windows doesn't set, so the native menus are
     always English.
   - `priority/roots.rs:64` `MOUNT_POINT_PREFIXES = ["/media", "/mnt", ...]`. `file_system/mod.rs:117` names the root
     "Root" at `/`. `tools/resources/volumes.rs:465` hardcodes `"/"`.
   - `secrets/mod.rs:160-166`: off macOS and Linux it falls back to `PlainFileStore`, so passwords would sit in
     plaintext on Windows. That's a data-protection blocker.
   - `crash_reporter/mod.rs:357`: signal handlers are `cfg(unix)`, so Windows native crashes go unreported (panics still
     are).
   - `instance_lock.rs:483`: "already running" goes to stderr only, which is invisible in a GUI app on Windows.
   - **The big one**: 194 `any(macos, linux)` gates switch whole pure-Rust feature families to stubs on Windows. That
     covers ADB (`adb/` has zero internal platform cfgs), SFTP, WebDAV, S3, NFS, saved servers, server moves, server
     volumes, eject, and `managed_policy`. They're gated because they live next to OS-mount code, not because they need
     Unix. Gated set: `lib.rs:29-30,105-118,154-165`; `commands/mod.rs:15-81`.

## Feature inventory

Classes: (a) needs a Windows impl, (b) stub/hide on Windows, (c) works as-is (or once un-gated). "Linux:" says what
Linux has today.

- **Menu bar** (`menu/`): (c) with QA. Linux: the Tauri/muda window menu. Windows takes the same per-window menu path.
  `menu/mnemonics.rs` already handles Alt mnemonics. The macOS extras (`macos_appkit.rs`, display accelerators, the
  viewer menu swap at `menu/mod.rs:495-536`) stay (b).
- **File context menu** (`menu/file_context_menu.rs`): the muda popup is (c). The tag row, icons, header, Services,
  share submenu, and File Provider items are (b). Windows expectation: shell items via `IContextMenu` (the `windows`
  crate, `Win32_UI_Shell`) for third-party verbs. That's new (a), later.
- **Dock** (`dock/`, add-to-Dock nudge): (b). The Windows analogs are new features: taskbar progress (Tauri's
  `Window::set_progress_bar`, cross-platform in tauri 2.12.3; the app doesn't use it yet, and it's a cross-platform win
  for macOS Dock progress too) and jump lists (`ICustomDestinationList`).
- **Native drag out** (`native_drag/`, `commands/file_system/drag.rs`): macOS only. Linux: an error stub. Windows (a):
  OLE `DoDragDrop` with `CF_HDROP`. Virtual-file drags (the MTP/SMB promise equivalent) need
  `CFSTR_FILEDESCRIPTORW`/`FILECONTENTS`, which is M work. The `drag` crate (2.1.1, CrabNebula, 2026-05) covers plain
  paths cross-platform; the `windows` crate covers the rest.
- **Clipboard file copy/cut/paste** (`clipboard/`, `commands/clipboard.rs:233-279`): macOS only. Linux: no-op. Windows
  (a): `CF_HDROP` plus `Preferred DropEffect` for cut (`clipboard-win` 5.4.3, alive, or the `windows` crate). Text and
  images already go through `tauri-plugin-clipboard-manager`: (c).
- **Volumes and mounts** (`volumes/`, `volumes_linux/`, `volume_listing.rs`, `space_poller/`): Linux has a full arm.
  Windows (a):
  - Drive letters as root volumes (`GetLogicalDriveStringsW`, `GetVolumeInformationW`, `GetDiskFreeSpaceExW`).
  - A device watcher (`RegisterDeviceNotification` / `WM_DEVICECHANGE`).
  - Mapped and UNC drives (`WNetGetConnectionW`).
  - The single `/` root model (`file_system/mod.rs:115-118`, about 169 `"root"` volume-id refs) has to admit several
    roots.
- **Eject** (`file_system/volume/eject/`, gated `any(macos, linux)`): (a). `CM_Request_Device_EjectW` or
  `IOCTL_STORAGE_EJECT_MEDIA`. DiskArbitration's unmount approver (`volumes/unmount_approver/`) is (b).
- **Network, SMB (app half)** (`network/`): the `smb2` browse/connect path, mDNS (`mdns-sd` is cross-platform), the
  known/manual servers, and the SFTP/WebDAV/S3/NFS wiring are (c) once un-gated. The OS-mount half is (a) or (b):
  - `mount.rs` (NetFS) and `mount_linux.rs` (gio): Windows uses `WNetAddConnection2W`.
  - `smb_smbutil.rs` and `smb_smbclient.rs` (share listing via CLI): Windows can skip them, since `smb2` lists shares
    itself.
  - `netfs_wedge.rs` and `os_mount_notice.rs`: (b).
  - `keychain.rs` and `secrets/system_keychain_smb.rs` (Finder-saved SMB passwords): Windows reads Credential Manager
    `TERMSRV`/domain creds, (a) later.
  - NFS uses `libc::getuid` for AUTH_SYS (`saved_servers/nfs_places.rs:117`): Windows needs a configured uid/gid.
- **Secrets** (`secrets/`): (a), and a must before any Windows build ships. `windows-native-keyring-store` 1.1.0
  (2026-05, same `keyring-core` family the Linux arm uses at `Cargo.toml:294`) maps to Credential Manager. A small task.
- **Icons** (`icons/`, `macos_icons.rs`, `linux_icons.rs`): (a). `SHGetFileInfoW` or `IShellItemImageFactory` (the
  `windows` crate). `file_icon_provider` 1.0.2 exists but is niche (23k downloads), so prefer `windows` directly.
- **Quick Look** (`quick_look/`): (b). The type is already `Mutex<()>` off macOS (`quick_look/mod.rs:34`). Windows
  preview handlers (`IPreviewHandler`) are a far-later option.
- **Services menu** (`services_menu/`): (b), macOS by nature.
- **File Provider / sync status** (`file_system/file_provider_actions/`, `sync_status/`, `cloud_actions.rs`): (b) now.
  The Windows analog is the Cloud Files API (`CfGetPlaceholderStateFromAttributeTag`,
  `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`): (a) later, S–M.
- **Google Drive links** (`file_system/google_drive/`, `mirror_db.rs:64` uses `MetadataExt`): (b) for now. Drive for
  desktop on Windows mounts a `G:` drive, and the links logic might port.
- **Finder tags** (`file_system/tags*.rs`): (b). `tags_io_off_macos.rs` already exists.
- **Permissions / FDA** (`permissions.rs`, `fda_gate.rs`): (b). Windows has no TCC. The Linux arm (always granted,
  `permissions_linux.rs:11`) is the right shape. Rename `get_macos_major_version` (an IPC name).
- **Updater**: (c) on the Rust side. Off macOS, `tauri_builder.rs:139-146` already registers `tauri-plugin-updater`,
  which supports NSIS/MSI. The custom updater's extras (`updater/advisories.rs`, write blocker) are macOS-only; parity
  is a product call. `latest.json` Windows entries belong to the packaging audit.
- **CLI install** (`cli/`): (b) in-app. On Windows the installer adds `cmdr` to `PATH`. `cli/system_host.rs:63` uses
  Unix symlinks.
- **Reveal in Cmdr / Finder extension** (`reveal/`): (b) as is. The Windows analog is an Explorer verb ("Open in Cmdr",
  registry `Directory\shell`). Windows 11's top-level menu needs `IExplorerCommand` plus a sparse package (packaging
  work).
- **Show in file manager** (`commands/file_actions.rs:59-91`): (c) via `tauri-plugin-opener`'s `reveal_item_in_dir`
  (already registered, `tauri_builder.rs:24`), which does `explorer /select`. Collapse the four copies.
- **Open with / default app / text editors / terminal** (`file_system/open_with.rs`, `default_app.rs`,
  `text_editor*.rs`, `terminal.rs`): (a). `SHAssocEnumHandlers` / `AssocQueryStringW`. Terminal: `wt -d` with a
  PowerShell fallback. Linux has none of these either (macOS-gated, `ipc.rs:818-844`).
- **Share sheet** (`file_system/share.rs`): (b). Windows `DataTransferManager` is possible, low value.
- **Accent color** (`accent_color*.rs`): (a), small. `UISettings::GetColorValue(Accent)` from the WinRT surface of the
  `windows` crate, re-read on `WM_SETTINGCHANGE` "ImmersiveColorSet".
- **Text size** (`text_size.rs`): (a), small. `UISettings::TextScaleFactor` plus `TextScaleFactorChanged`.
- **Display accessibility** (`display_accessibility.rs`): (a), small. High contrast via `SPI_GETHIGHCONTRAST`; reduce
  transparency via `UISettings::AdvancedEffectsEnabled`.
- **Liquid Glass** (`glass_tint.rs`): (b). Mica/Acrylic via `window-vibrancy` 0.8.1 (tauri-apps, 2026-09) is the
  Windows-native look; a product call.
- **Intl** (`intl/`, `system_strings.rs`): (a). `GetUserPreferredUILanguages` for the UI locale and
  `GetUserDefaultLocaleName` for formats (or `sys-locale` 0.3.2, 1Password, stable but last released 2024-11). Live
  change via `WM_SETTINGCHANGE` "intl". `system_strings` (Finder labels) is (b).
- **Crash reporter** (`crash_reporter/`): (a). `SetUnhandledExceptionFilter`, or `crash-handler` 0.8.1 plus `minidumper`
  0.11.0 (EmbarkStudios, alive). `os_crash_report.rs` (`.ips`) and `objc_exception.rs` are (b). The panic path
  (`panic_courier`, `contain`) is (c).
- **Instance lock**: the lock is (c) (`File::try_lock` → `LockFileEx`, `instance_lock.rs:12`). The alert is (a), tiny:
  `MessageBoxW`.
- **llama-server / AI** (`ai/process.rs`): (c). `not(unix)` arms exist (`ai/process.rs:125,176,216`). `executable_of` is
  macOS-only. `ai/extract.rs:55-79` Unix symlinks are gated. The binary download is a packaging concern.
- **Licensing** (`licensing/device_id.rs`): (a), tiny. `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`
  (`windows-registry` 0.100, Microsoft). Side bug: the Linux arm returns `None` unconditionally
  (`licensing/device_id.rs:87-90`) though `/etc/machine-id` exists, so Linux fair-use tracking has no device id. That's
  a tradeoff call for David, so I didn't fix it.
- **Managed policy** (`managed_policy/`): (a), small. A `RegistryPolicySource` implementing `ManagedPrefsSource`
  (`HKLM\SOFTWARE\Policies\Cmdr`, the Group Policy convention). Un-gate the module.
- **MTP** (`mtp/` app half: hotplug watcher, `macos_workaround.rs`): (a), but the work is in `cmdr-mtp`. Windows binds
  MTP devices to the WPD driver, so `nusb` can't claim them without WinUSB, and the native route is the WPD COM API.
  Crates audit.
- **ADB** (`adb/`): (c) once un-gated: zero platform cfgs inside, `adb.exe` speaks the same TCP protocol.
- **File watcher** (`file_system/watcher.rs`): (c). The `not(macos)` arm is `notify`, which uses `ReadDirectoryChangesW`
  on Windows.
- **Notifications** (`notifications.rs`): (c). `NotificationPermission::Unknown` off macOS is fine; the plugin supports
  Windows toasts.
- **Spotlight recents** (`spotlight.rs`): (b). It already returns empty off macOS. The Windows Recent Items folder is an
  option.
- **Thread priority / QoS** (`priority/`, crates' `cmdr-fs`): (b)/(c) here. Windows' `SetThreadPriority` /
  `THREAD_MODE_BACKGROUND_BEGIN` sits in the crates.
- **System memory / diagnostics** (`system_memory.rs:64`, `diagnostics_snapshot.rs`): (a), small. `sysinfo` is already a
  dependency and covers Windows memory. `thermal_state` is (b).
- **Platform identity** (`platform.rs:9`): (a), tiny. `RtlGetVersion` or the registry `ProductName`/`DisplayVersion`.
  The fallback today is `"windows"`.
- **Mouse back/forward** (`mouse_nav.rs`): (a), small. `WM_XBUTTONDOWN`, or check whether WebView2 already delivers
  `mouse` button 3/4 to the frontend.
- **Disk images** (`volumes/disk_image.rs`, `hdiutil`): (b). `Mount-DiskImage` for ISO/VHD is a nice-to-have.
- **Window chrome** (`tauri.conf.json`): `titleBarStyle: Overlay`, `trafficLightPosition`, `hiddenTitle`, and
  `acceptFirstMouse` are macOS keys. Linux has to revert Overlay at runtime (`lib.rs:733-740`); verify on Windows
  whether Overlay is ignored or hides the controls, and generalize the revert to `not(macos)` if needed. The
  `macos-private-api` feature (`Cargo.toml:142`) is harmless off macOS.
- **Capabilities JSON** (`capabilities/*.json`): (c). No platform-specific permissions spotted.
- **Global shortcut** (`downloads/global_shortcut.rs`): (c). The plugin supports Windows. The default `⌃⌥⌘J` needs a
  Windows accelerator; the frontend audit covers display.

## macOS-only by nature (hide, don't port)

- Dock pinning, Services menu, Liquid Glass slider, FDA/TCC onboarding, Quick Look, Finder tags, the Finder extension
  and `NSFileViewer` reveal handler, File Provider actions, the DiskArbitration unmount approver, the `ptpcamerad`
  workaround, `.ips` crash reports, Objective-C exceptions, `hdiutil` disk images, NetFS wedge detection, the Spotlight
  recents query, and the custom updater's TCC-preserving install.

## Windows-only features users will expect

- **Platform features**: Recycle Bin with restore (the crates audit; the `trash` crate 5.2.9 is alive), drive letters as
  top-level volumes, UNC paths (`\\server\share`) as first-class locations, taskbar progress for long operations, jump
  lists (recent folders), and Explorer integration ("Open in Cmdr" verb, optionally "set as default folder handler",
  which is risky and opt-in).
- **Filesystem behaviors**: hidden/system attributes, so `FILE_ATTRIBUTE_HIDDEN` decides hidden files rather than the
  dot-prefix; `.lnk` shortcuts; junctions; Known Folders (`SHGetKnownFolderPath`) for favorites and default locations;
  long paths (`\\?\`); reserved names (`CON`, `NUL`, ...) and trailing-dot/space rules in rename and Multi-Rename
  validation; case-insensitive name collisions; Alt-key menu mnemonics.

## Proposed convention

- **One seam per feature, shaped the same everywhere**: `<feature>/platform/{mod.rs, macos.rs, linux.rs, windows.rs}`.
  - `mod.rs` picks the arm with `#[cfg_attr(target_os = "...", path = "...")] mod imp;` and pins every exported fn's
    signature: `const _: fn(&Path) -> Option<String> = imp::fetch_icon_for_path;`. Every arm must then match, checked on
    the Mac too.
  - A stateful backend (secrets, managed policy, watcher) uses a trait, as `ManagedPrefsSource` already does.
- **Exhaustive arms, no silent "other"**:
  `#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))] compile_error!(...)` once at the
  crate root. `stubs/` goes away. Each "not available here" answer lives in that feature's `windows.rs` (or `linux.rs`),
  with the real types.
- **One IPC command per feature**: commands stay pass-throughs with one signature; the platform split sits under them.
  This deletes the 63 duplicated command definitions, the three-way `ipc.rs` groups, and the hand-synced stub types (the
  stubs' C.md's "align the corresponding stub type by hand" stops being a rule).
- **A capability answer instead of silent stubs**: a `platform_capabilities()` command (`has_quick_look`, `has_dock`,
  `has_tags`, `can_drag_out`, ...), so the frontend hides a feature on purpose. Today a stub returning success makes it
  "silently not appear", which is untestable.
- **Neutral names**: `show_in_finder` becomes `reveal_in_file_manager` and `get_macos_major_version` becomes
  `get_os_version`, per "name internals after the UI", since the UI says Explorer on Windows.
- **Enforcement**: a `desktop-rust-platform-cfg-placement` check that allows `target_os`/`unix`/`windows` cfgs only in
  `*/platform/*`, `lib.rs` mod declarations, `Cargo.toml`, and tests. It starts with a per-file allowlist at today's
  counts and ratchets down (house style). It generalizes `desktop-rust-cfg-gate`, which already guards macOS-only
  imports.
- **Compiler-backed**: a `cargo check --target x86_64-pc-windows-msvc` lane (CI `windows-latest`, or `cargo xwin check`
  locally) plus clippy. Without it every arm above rots the way `stubs/volumes.rs` did.

Is it a clear win? Mostly yes, even if Windows never ships:

- It removes 63 duplicate commands and about 1,000 lines of hand-mirrored stubs.
- It gives the Linux arms compile-time signature parity with macOS (today a drift shows up only in the Linux CI lane).
- It makes the layering honest, since remote backends no longer depend on OS-mount code.
- `lib.rs` setup shrinks from a 400-line `cfg` weave.

The tradeoff: churn across about 190 files and merge friction with parallel work, so land it seam by seam, and the
signature pins add a few lines per seam. The `compile_error!` and the Windows check lane are the only parts that pay off
only if Windows ships, and both are cheap.

## Refactor tasks

Sizes use the board's scale. "Win alone" means the task is worth landing even if Windows never ships.

1. **XS, win alone (partly)**: move the cross-platform crates out of the `any(macos, linux)` table
   (`Cargo.toml:304-337`: `url`, `semver`, `plist`, `pulldown-cmark`, `ammonia`, `unicode-normalization`, `urlencoding`,
   and later `smb2` and `mdns-sd`) into `[dependencies]`, and gate the five Unix-only tests (class 3). It makes the
   manifest tell the truth.
2. **S, prerequisite**: a Windows compile lane: `cargo check`/`clippy -p cmdr --target x86_64-pc-windows-msvc` on
   `windows-latest`, informational at first. It replaces this audit's static guesses with the real break list. It
   depends on the crates audit's fixes, so run it as a scoreboard.
3. **S, win alone**: write the convention doc (a new platform-seams guide in `docs/guides/`) and the
   `desktop-rust-platform-cfg-placement` check with a ratcheting allowlist. Pilot the shape on `platform.rs` (identity),
   `licensing/device_id.rs`, `system_memory.rs`, and `diagnostics_snapshot.rs`, then move `linux_distro.rs` under it.
4. **M, win alone**: one IPC command per feature. Collapse the 63 duplicated commands (file actions, permissions, accent
   color, clipboard, drag, Quick Look, get-info) to one definition each with a platform fn below; delete
   `stubs/{permissions,accent_color,text_size,display_accessibility,glass_tint}.rs`; rename the platform-named commands.
   It touches the frontend only for renames.
5. **S, win alone**: startup seam. Move the per-OS installers out of the `lib.rs` setup hook (accent, display
   accessibility, glass, text size, mouse nav, Services, volume watcher, unmount approver, `ptpcamerad`, the Linux title
   bar revert) into `platform/startup` with one `install(app)` per OS.
6. **M, win alone**: the volumes seam. Fold `volumes/` and `volumes_linux/` into one `volumes/` with
   `platform/{macos,linux}/` behind a pinned fn set (or a `MountTable` trait); kill the `as platform` aliases, the
   `SmbMountInfo` alias, and `volume_listing.rs`'s per-OS re-exports; route `listing_host/mounts.rs`,
   `space_poller/mod.rs:514`, `file_system/index_provider.rs`, and `usb_speed.rs` through it. Delete `stubs/volumes.rs`.
7. **L (4–6 agents), win alone**: split remote backends from OS mounts.
   - Un-gate `adb`, the SFTP/WebDAV/S3/NFS wiring, `saved_servers`, `server_move`, `server_volumes`,
     `commands/{servers,sftp,webdav,s3,nfs,volume_sign_in}`, and `managed_policy` from `any(macos, linux)`.
   - Put `network/`'s OS-mount half (`mount*.rs`, `smb_smbutil.rs`, `smb_smbclient.rs`, `netfs_wedge.rs`,
     `os_mount_notice.rs`, `keychain.rs`) behind `network/os_mount/platform/`.
   - Delete `stubs/network.rs` (578 lines) and `stubs/mtp.rs`.
   - This is the biggest layering win: the pure-Rust backends stop depending on OS-mount code.
8. **S, win alone**: small-module seams for icons (`icons/mod.rs`), the intl UI locale (`intl/mod.rs:175-205`), the
   viewer's positional read (`file_viewer/source.rs`), the size measurer's inode dedup
   (`listing_index_sizes/count/measure.rs`), and the crash reporter's native-crash hook.
9. **Windows implementations**, each after its seam and each XS–S unless noted:
   - Secrets via `windows-native-keyring-store` (XS, a blocker).
   - Drives and the device watcher (M).
   - Eject (S).
   - Icons (S).
   - Clipboard `CF_HDROP` (S).
   - Drag out with plain paths (S); virtual files (M).
   - Accent color, text size, display accessibility, and intl together (S).
   - Crash minidumps (S).
   - The instance-lock `MessageBoxW`, device id, OS version, and memory together (XS).
   - Open with, default app, and terminal (S).
   - Managed policy from the registry (XS).
   - `WNetAddConnection2W` mounts plus Credential Manager SMB creds (S).
   - Explorer verb, taskbar progress, and jump lists (S, partly packaging).
   - Multi-root path model (`"root"` volume id, `/` assumptions at `file_system/mod.rs:115`, `priority/roots.rs:64`,
     `tools/resources/volumes.rs:465`): M, and it overlaps the crates audit's `Volume`/path work. Coordinate there.
   - MTP over WPD: L, in `cmdr-mtp`, crates audit.

Suggested order: 1 → 2 → 3 → (4, 5, 6 in parallel) → 7 → 8 → 9. Steps 1–8 are all Mac-landable and pay off on their own.
Step 9 needs the Windows lane green.
