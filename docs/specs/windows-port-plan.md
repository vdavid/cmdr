# Windows port plan

**Cmdr runs on macOS (and builds on Linux), and has zero Windows code.** This plan gets there gradually: most steps are
refactors that make the codebase better on their own (platform seams, decoupling, typed errors), each landable on a Mac,
so the Windows-only work at the end shrinks to filling in arms behind seams that already exist.

Status: planned 2026-11-04 at `7a199a9705`. Tracking issue: [#942](https://github.com/getcmdr/cmdr/issues/942). Evidence
(static reads, no Windows compile yet):

- `docs/notes/windows-port/crates-audit.md`: every crate under `crates/`, per concern, with the Windows API per site.
- `docs/notes/windows-port/app-audit.md`: the app crate, feature by feature, and the six dispatch patterns in use.
- `docs/notes/windows-port/frontend-build-audit.md`: frontend, Tauri config, CI, release, website, api-server, tests.
- Linux precedent: `docs/specs/linux-builds-plan.md` and `docs/notes/linux-gaps-2026-08-10.md`. Most wins below close
  Linux gaps too.
- An earlier rough spec (`linux-and-windows-versions.md`, 2026-01-18, deleted in `eac9e6187f`) predates almost all of
  today's code; read it with `git show eac9e6187f^:docs/specs/linux-and-windows-versions.md` for history only.

## Where we stand

- **The crate boundary isn't an OS boundary.** "Nothing below imports the app" (`index-crate-isolation`) says nothing
  about the OS. `cmdr-operations`, `cmdr-index`, `cmdr-fs`, and `cmdr-local` hold about 900 unix/macOS sites; the
  network backends, `cmdr-listing`, `cmdr-collation`, `cmdr-menu`, `cmdr-media-meta`, and `cmdr-operation-log` are
  nearly clean.
- **The app crate**: 1,156 `macos` cfgs in 188 files, 269 `linux`, zero `windows`. The "other OS" arm (`stubs/`) has
  never been compiled and doesn't (`stubs/volumes.rs:194` calls `libc` off unix).
- **Nothing compiles for Windows today, for manifest reasons first**: unconditional unix-only deps (`xattr`, `uzers`,
  `exacl`, `libc`) in the crates, and cross-platform deps (`url`, `semver`, `pulldown-cmark`, …) declared for
  `any(macos, linux)` only in the app.
- **Gates encode "two platforms"**: about 180 `not(target_os = "macos")` arms mean "Linux" by accident, and 194
  `any(macos, linux)` gates switch pure-Rust features (ADB, SFTP, WebDAV, S3, NFS, saved servers) off on Windows only
  because they sit next to OS-mount code.
- **Data-safety hazards**: `not(unix)` stub arms in `cmdr-operations` compile on Windows and answer the unsafe way
  (`is_same_file` → `false`, `validate_destination_writable` → `Ok`). Errors are classified by errno, but Windows hands
  back Win32 codes. Both must be fixed by seams, not patched per site.
- **The frontend knows only "macOS or not"**, by sniffing the user agent (74 sites), and the release pipeline, website,
  and api-server hardcode three macOS builds.

## The architectural wins

Each is worth landing even if Windows never ships. Together they're most of the port.

1. **Platform seams, one shape everywhere.** `<feature>/platform/{mod,macos,linux,windows}.rs`, with `mod.rs` pinning
   every exported signature (`const _: fn(&Path) -> Option<String> = imp::fetch_icon;`) so each arm must match, a
   `compile_error!` for unknown targets, and a check that keeps platform cfgs inside seams. Kills six ad hoc dispatch
   patterns, 63 IPC commands defined once per OS, and about 1,000 lines of hand-mirrored stubs; gives Linux compile-time
   parity with macOS.
2. **Remote backends stop depending on OS-mount code.** Un-gating ADB, SFTP, WebDAV, S3, NFS, and saved servers from
   `any(macos, linux)` and putting the OS-mount half of `network/` behind its own seam is the biggest layering fix.
3. **Typed OS-error classes at the volume boundary.** Classify once in `VolumeError::from_io_at`; consumers match a
   class, network backends construct one instead of faking `libc::EIO`. Raw errnos stop crossing layers (their numbers
   differ per OS), which is the type-level version of the no-string-match rule.
4. **One file-identity vocabulary** (`FileId`, `same_node`, `device_of`, `link_count`) replaces about 30 scattered
   `dev()`/`ino()` sites asked three different ways, and deletes the unsafe "don't know, say no" stubs.
5. **The backend tells the frontend what exists.** A synchronous platform value plus a typed capability set (Quick Look,
   Services, FDA, drag-out, OS file-manager name, …) replaces the UA sniff, so features hide on purpose and tests can
   run each platform.
6. **Name validity follows the destination filesystem, not the host OS.** An exFAT stick or an SMB share on a Mac
   already enforces Windows' rules (reserved names, forbidden characters, trailing dots), so this fixes real macOS cases
   and makes Windows a data entry.
7. **N-platform release and services.** One release asset manifest drives `latest.json`, checksums, SBOM, vetting, and
   the website; downloads and telemetry gain an explicit `os`. The Linux release falls out of the same work.
8. **Smaller seams with Linux payoffs**: a watcher backend trait (Windows then gets watching from `notify`), one mount
   façade, process stats behind one module (Linux gains the memory watchdog it lacks), platform-variant UI copy (Linux
   needs it too), a per-platform keymap (fixes today's `Ctrl+D` clash off macOS).

## Decisions

Settled:

- **Image ML indexing (Vision, Core ML CLIP) doesn't ship on Windows.** An explicit "unavailable" backend reports it
  off, and the setting hides. `Windows.Media.Ocr` can come back later as its own effort.
- **Native window decorations on Windows** (as on Linux): no custom title bar fighting Snap Layouts.
- **NSIS installer, per-user (`currentUser`), `embedBootstrapper` for WebView2.** MSI only if enterprise asks.
- **D1. The path model** (David, 2026-11-04): every local path on Windows is an absolute forward-slash path whose first
  component names its root: `C:/Users/ada` for a drive, `//server/share/x` for a UNC share. Each drive and UNC share is
  its own volume, rooted at `C:/` or `//server/share/`. The drive letter is always uppercase. Backslashes exist only
  inside the Windows local backend (`\\?\C:\…` for long paths) and in the one display formatter. Case sensitivity is a
  per-volume property (`caseSensitive`), never inferred from the OS.
  - Why: it's the rule `crates/cmdr-fs/src/volume/remote_paths.rs` already sets for remote volumes, that an app path
    names its own volume. So the grammar becomes "a scheme URL, or a `/`-separated absolute path whose first component
    is the root". Every existing `/` arithmetic in the frontend, the index, and `lib/path` stays correct; `lib/path`
    only learns the two new root shapes; Win32 APIs accept `/` everywhere except the `\\?\` form.
  - ❌ Rejected: a drive as a volume rooted at `/` with volume-relative paths (`/Users/ada` plus a volume id). Those are
    exactly the bare paths `remote_paths.rs` forbids: a restored tab, favorite, drag, or go-to-path that loses the
    volume hint resolves to the wrong disk. Also rejected: native `\` on the wire (touches ~110 frontend sites and every
    Rust↔TS fixture) and MSYS-style `/c/Users` (unfamiliar, and pasted paths need translating).
- **D2. MTP ships on Windows** (David, 2026-11-04), through `mtp-rs`'s WPD COM backend. `mtp-rs` picks it automatically
  behind its high-level `mtp::` API, with no driver install (verified on a Pixel 9 Pro XL, per its README), and
  `cmdr-mtp` uses only that API, never the USB-only `ptp::` layer. Windows' MTP driver can stall any call for 5–6
  minutes after files change on the phone; that's a Windows quirk every app and library meets (File Explorer too), so
  Cmdr shows it as a "the phone is busy" state with a timeout, never as a hang or an error. Task 56. The crates audit's
  MTP note (WinUSB driver binding) predates this and is superseded.
- **D4. Minimum Windows version** (David, 2026-11-04): Windows 10 22H2 and Windows 11, x86_64 first, ARM64 later. Drop
  Windows 10 when WebView2 does (Microsoft promises WebView2 updates on 10 22H2 until at least 2028-10).
  - Why: Windows 10 is 25.13% of Windows desktops (StatCounter, 2026-10; Windows 11 74.29%, the rest about 0.6%), and
    Total Commander's audience likely skews to it. Code cost is a few fallbacks where Windows 11 has extras (EcoQoS
    thread mode, Mica, 24H2 block-clone copies), plus a Windows 10 VM in each release's QA.
  - Hard floor: Windows 7/8/8.1 are out. WebView2 stopped updating there in 2023-01, and Rust's tier-1
    `x86_64-pc-windows-msvc` has required Windows 10 since 2024 (the Win7 target is tier 3, no prebuilt std). Total
    Commander reaches Windows 95 only because it's a native Win32 app with no browser engine.

- **D3. Code signing and distribution** (David, 2026-11-04): Store first, a paid certificate only once Windows pays for
  it.
  1. **The Microsoft Store, as an MSIX package, is the primary Windows channel.** Microsoft signs it for free (the
     company developer account is free too), so there's no SmartScreen warning and Smart App Control lets it run from
     day one. The Store delivers updates, so the Store build turns Cmdr's own updater off. Slower hotfixes (Store
     certification per release, hours to days) are accepted. MSIX also gives the package identity Windows 11's top-level
     context menu needs.
  2. **An unsigned "Run anyway" NSIS installer on the website**, built from the same release binaries, for people who
     avoid the Store. The download page explains SmartScreen's "More info → Run anyway" step. Its updates travel the
     Tauri updater, trusted by the minisign signature plus the Rust-side checks (task 33).
  3. **Later, once Windows sales cover it**: Azure Artifact Signing through Rymdskottkärra AB ($9.99/month) signs the
     direct download, which retires the "Run anyway" step.
  - Why: the Store gives the highest OS trust at zero cost, and a signed direct download buys less trust than that for
    $119.88/year (SmartScreen still warns until reputation builds). Total Commander's route, a Sectigo OV certificate
    for its company (valid 2025-09-26 to 2028-09-25, read from `tcmd115801x64.exe`), costs more for the same trust as
    Azure. Since 2024, EV no longer buys instant SmartScreen reputation either.

## Bugs the audits found on the way

- **Linux: image indexing writes invented data.** Off macOS the production media backend is `FakeVisionBackend`
  (`crates/cmdr-index/src/media_index/scheduler/lifecycle.rs:231`), and the setting isn't hidden
  (`apps/desktop/src/lib/settings/definitions/indexing.ts:91`), so turning it on fills `media.db` with OCR text and tags
  derived from file names. Fixed in [#943](https://github.com/getcmdr/cmdr/issues/943) (task 1): off macOS the scheduler
  doesn't start, and the Image indexing settings page is gone.
- **Linux: dialog backdrops left a 29px strip live** at the top (`--titlebar-height` was unconditional, the title bar
  renders only on macOS). Fixed in task 24: `--titlebar-height` is 0 off macOS, the viewer's 35px override and the queue
  window's in-webview title line are macOS-only too.
- **Off macOS, `Ctrl+D` is bound twice**, so favorites never fires in the file list. Task 26.
- **api-server titles Linux reports "macOS Ubuntu …"** (`apps/api-server/src/report-cards.ts:140`). Task 2.
- **Off macOS the updater skips the advisory list and trust checks** (`lib/updates/updater.svelte.ts:390-430`). Fine for
  an unadvertised Linux alpha, a blocker for any Windows release. Task 33.
- **Linux licenses carried no device id.** Fixed in [#980](https://github.com/getcmdr/cmdr/issues/980): Linux now hashes
  `/etc/machine-id`, the precedent task 38's `MachineGuid` arm follows.

## Task list

Sizes per `docs/tooling/backlog-board.md` (XS: one agent under 200k tokens; S: one agent 200–400k; M: one agent with up
to three subagents; L: 4–13 agents). "Win" means worth it without Windows; "Windows" means Windows-only. Each task names
its prerequisites; tasks without any can start any time. Order within a phase is a suggestion.

### Phase 0: quick fixes

1. [XS] Win. **Done** ([#943](https://github.com/getcmdr/cmdr/issues/943)). Media ML off macOS is unavailable: no
   backend (`platform_backends` answers `None`), so the scheduler doesn't start, an older build's invented `media.db`
   rows go at launch ([#1001](https://github.com/getcmdr/cmdr/issues/1001)), and the image-indexing settings carry
   `needsImageIndexing`, which drops their page from the sidebar. The Linux E2E build keeps a fake engine
   (`cmdr-index/playwright-e2e`) so its specs still run. Task 25's capability set can take over the frontend half
   (`imageIndexingAvailable()`) later.
2. [XS] Win. Small fixes: `macOsLabel` prefixing non-mac OS names, the MCP instructions' "for macOS", `'Cascadia Mono'`
   before `Consolas` in `--font-mono`.

### Phase 1: make platform code honest and checkable

3. [XS] Win. **Done**. Honest crate manifests: `xattr`, `uzers`, `exacl`, `libc` under `cfg(unix)` in `cmdr-fs` and
   `cmdr-local`, the `xattr` dev-dependencies too; `notify` stays `linux` in `cmdr-index` until task 12 gives Windows a
   watcher arm. Of the nine ungated `std::os::unix` imports, two remained (`cmdr-local/src/reading.rs`,
   `cmdr-git/src/repo.rs`), now gated.
4. [XS] Win. **Done**. Honest app manifest: `url`, `semver`, `plist`, `pulldown-cmark`, `unicode-normalization`, and
   `urlencoding` sit in `[dependencies]` (`ammonia` and the viewer's positional read had already moved to
   `cmdr-viewer`), and the six unix-only tests carry `cfg(unix)`. `open_with_copies.rs` and `measure.rs` keep their
   `MetadataExt` inode reads for task 9's vocabulary: a `cfg(unix)` there would add cfgs outside a seam.
5. [S] Windows (cheap insurance). Non-blocking Windows CI lane: `windows-lane.yml` mirroring `macos-lane.yml`, native
   `cargo check` + clippy on `windows-latest`, a ratcheting allowlist of members that must compile (`platforms` metadata
   gains `"windows"`), promoted to blocking once green. Turns the audits' static guesses into a real break list. Needs 3
   and 4 to get past dependency resolution.
6. [XS] Win. **Done**. Tauri config split: macOS keys into `tauri.macos.conf.json`, Linux's into
   `tauri.linux.conf.json`, explicit per-OS `bundle.targets`, a `tauri.windows.conf.json` (NSIS, `currentUser`,
   `embedBootstrapper`), and `windows/app.manifest` (`longPathAware`, per-monitor-v2 DPI) through `build.rs`.
   `macos-private-api` stays on for every target: tauri-build refuses a per-target feature, and it's a no-op since Tauri
   2.12.1 (`apps/desktop/src-tauri/DETAILS.md` § "Tauri config per platform").
7. [S] Win. **Done**. `docs/guides/platform-seams.md`; one named cfg, `cmdr_host`, from the `cmdr-platform-cfg` build
   dependency (`cmdr_apple` and `cmdr_unix_host` would only respell `target_os = "macos"` and `unix`); the app root's
   unknown-target `compile_error!` (Windows is known); `platform-cfg-placement` counting negated OS cfgs and unseamed
   ones per file (seeded at 351 and 1,569 over 261 files); `platform/`, `system_memory/`, and `diagnostics_snapshot/`
   piloted, `linux_distro.rs` folded into `platform/linux/`.
8. [S] Win. **Done**. The crates' negated OS gates went from 138 to 18: each arm names its OSes, the browser build
   (`target_family = "wasm"`) where the old arm served it too, and Windows only where the body is right there, so a
   Windows build fails at each missing arm instead of inheriting Linux's. `cmdr-fs` takes `cmdr_host` (build script,
   crate-root gate, `rt`, the fallback runtime) and asks `cfg(unix)` where `not(wasm)` meant a POSIX host. Left for
   their own tasks: the identity stubs (9) and `cmdr-http` (16). Positive `any(macos, linux)` lists stay: honest, if not
   yet in seams.

### Phase 2: crate seams

9. [S] Win. File-identity vocabulary in `cmdr-fs` (128-bit file ids for ReFS), replacing the ~30 `dev()`/`ino()`/
   `nlink()` sites and every `not(unix)` "say no" stub in `cmdr-operations`, `cmdr-local`, `cmdr-index`, `cmdr-git`.
10. [M] Win. Typed OS-error classes in `cmdr-fs` (`VolumeError::from_io_at` classifies; "in use by another app" gets its
    own class), consumers in `cmdr-operations`, `friendly_error`, `transport_fault`, and a subagent sweep over the
    network backends' synthesized errnos.
11. [S] Win. Mount façade in `cmdr-local` (`mounts::list`, `is_mount_point`, `user_mount_prefixes`); callers in
    `cmdr-operations` and `cmdr-index` stop naming the OS.
12. [S] Win. Watcher backend trait in `cmdr-index`: FSEvents and `notify` behind one `DriveWatcher`, replay as a backend
    property, the test journal as a third backend. Windows then needs only the dep widened and a smoke test.
13. [S] Win (Linux gains a watchdog). Process stats behind one module with per-OS readers; the memory watchdog consumes
    the portable reading.
14. [S] Win. Name and path validity keyed on the destination's `filesystem_kind`, not the host OS (`cmdr-fs` validation,
    `cmdr-operations` rename validity, Multi-Rename). Start with a quick check of today's exFAT and SMB behavior on a
    Mac.
15. [XS] Win. Portable test helpers (`testing::symlink`, `is_root`, special-file fixtures, `can_create_symlinks()`),
    replacing ~60 `cfg(unix)` test sites.
16. [XS] Win. `cmdr-http`: `MacSystem` → `OsSystem` with per-OS submodules.
17. [S] Win (moderate). `cmdr-fs` metadata classes: per-OS bodies into `metadata/platform/`, ordering rules stay in
    `mod.rs`; `cmdr-local`'s caps constant moves beside them. Needs 7.

### Phase 3: app seams

18. [M] Win. One IPC command per feature: collapse the 63 duplicated commands (file actions, permissions, accent color,
    clipboard, drag, Quick Look, Get Info) onto platform fns, delete the matching `stubs/`, rename platform-named
    commands (`show_in_finder` → `reveal_in_file_manager`, `get_macos_major_version` → `get_os_version`; the frontend
    changes for renames only). Needs 7.
19. [S] Win. **Done**. Startup seam: `platform/startup/` with one pinned `install(app)` per OS, called once in the setup
    hook after the volume broadcast and before the MTP watcher, the first volume list, and the menu bar. It holds the
    order-free installers (macOS observers, mouse navigation, Services, volume watcher, unmount approver, `ptpcamerad`,
    File Provider warm-up, drag-out handle, updater state; Linux watcher, accent, title-bar revert); order-bound steps
    (the E2E activation policy, the FDA probe, `after_update`) stay on the spine.
20. [M] Win. Volumes seam: fold `volumes/` and `volumes_linux/` into one `volumes/` with `platform/` arms behind a
    pinned fn set, remove the `as platform` aliases and `stubs/volumes.rs`, route `listing_host/mounts.rs`,
    `space_poller`, `index_provider`, `usb_speed` through it. Needs 7, and 11 for the shared mount façade.
21. [M] Win. Un-gate the remote backends: ADB, SFTP/WebDAV/S3/NFS wiring, `saved_servers`, `server_move`,
    `server_volumes`, their commands, and `managed_policy` leave `any(macos, linux)`. Needs 4.
22. [M] Win. The OS-mount half of `network/` (`mount*.rs`, `smb_smbutil.rs`, `smb_smbclient.rs`, `netfs_wedge.rs`,
    `os_mount_notice.rs`, `keychain.rs`) moves behind `network/os_mount/platform/`; delete `stubs/network.rs` and
    `stubs/mtp.rs`. Needs 21.
23. [S] Win. Small-module seams: icons, intl UI locale, the viewer's positional read, the size measurer's inode dedup
    (onto task 9's vocabulary), the crash reporter's native-crash hook. Needs 7.

### Phase 4: frontend seams

24. [S] Win. **Done**. `#lib/platform` (`apps/desktop/src/lib/platform/CLAUDE.md`): a plugin init script in
    `tauri_builder.rs` defines `window.__CMDR_PLATFORM__` in every webview, `isMacOS()` lives there and reads it (no UA
    sniff; `'linux'` without a backend, the demo answers for the visitor), `data-platform` on `<html>`,
    `--titlebar-height: 0` off macOS, and `withPlatform()` drives every test that used to mock `isMacOS` or stub the UA.
25. [S] Win. Capabilities instead of OS checks: a typed `platform_capabilities()` from the backend; migrate the 74
    `isMacOS()` sites in 28 files. Needs 24 (and is easier after 18).
26. [S] Win. Per-platform default keymap: explicit per-platform overrides where mechanical mapping is wrong, retire the
    `⌃`→Shift hack, fix the `Ctrl+D` clash, conflict tests for all three platforms plus OS-reserved combos (Windows,
    GNOME). Needs 24.
27. [M] Win (Linux needs it). Platform-variant copy: intl injects a `platform` argument, the ~230 macOS-flavored English
    values and the `.mac`/`.other` key pairs become ICU `select` or neutral wording, i18n checks learn it. David reviews
    the English. Needs 24.
28. [L] Win. Translate task 27's changed strings into the 13 languages (translator agents, per
    `docs/guides/i18n-translation.md`). Needs 27.
29. [S] Windows, lint half is a win. Path wire format: `lib/path` learns `C:/` and `//server/share` roots,
    `caseSensitive` per volume, `\` in the display formatter, an ESLint rule steering raw local `split('/')` /
    `lastIndexOf('/')` to `lib/path`. Needs D1.

### Phase 5: release and services for N platforms

30. [M] Win (Linux release falls out). Release asset manifest: one manifest of (os, arch, kind, file, updater key)
    drives `publish`, `latest.json`, `release-finish`, `vet-update-archives`, checksums, SBOM, and website sizes;
    `dmgSizes` stays one more release for old website builds.
31. [M] Win. Downloads and analytics for N platforms: `/download/:version/:os/:arch` (old route stays as the macOS
    alias), artifact-aware UA classification in the api-server and dashboard funnel, an explicit `os` field through
    heartbeat, feedback, and reports (a Cloudflare D1 migration), the dashboard split by OS. Needs 30.
32. [S] Win. Website download page for three OSes (the Linux plan's Part 3, built once). David reviews copy and design.
    Needs 30.
33. [S] Win (Linux too). Updater trust parity off macOS: the Rust-side check, advisory list, and managed-install logic
    run everywhere; the plugin only hands off the installer; the UI handles "the app quits to install".

### Phase 6: Windows implementation

Each lands behind a seam from Phases 1–5, verified on the Windows lane (task 5).

34. [M] Windows. Multi-root path model in the app: drives and UNC shares as volumes, the `"root"` volume id and `/`
    assumptions (`file_system/mod.rs:115`, `priority/roots.rs:64`, `tools/resources/volumes.rs:465`). Needs D1, 20.
35. [L] Windows. `LocalWindowsVolume` in `cmdr-local`, a sibling of `LocalPosixVolume`: `FindFirstFileExW` reader,
    space, no-replace and atomic-replace rename (❗ `std::fs::rename` replaces on Windows), streams, file vs dir
    symlinks, junctions, reparse tags, ADS-backed xattrs, write-access probe, metadata caps, the conformance suite
    green. Needs 9, 10, 34.
36. [M] Windows. `cmdr-operations` arms: `CopyFileExW` strategy (with ReFS/Dev Drive block clone), Recycle Bin via
    `trash` with identity, read-only as "locked" in `clear_blockers`, sharing violations as a typed refusal. Needs 35.
37. [S] Windows. Cloud Files placeholders (OneDrive and friends: sync badges, cloud trash) and Mark of the Web on
    extract (`cmdr-operations` + `cmdr-archive`). Needs 36.
38. [S] Windows. Identity and system bundle: secrets via `windows-native-keyring-store` (❗ a blocker: off macOS and
    Linux secrets fall back to a plaintext file today), device id (`MachineGuid`, hashed like the Linux
    `/etc/machine-id` arm in `licensing/device_id.rs`), OS version, memory, the instance-lock `MessageBoxW`, managed
    policy from the registry. Needs 18.
39. [M] Windows. Drives: drive letters and labels, the device-arrival watcher (`CM_Register_Notification`), mapped and
    UNC drives, eject. Needs 20, 34.
40. [S] Windows. System look: accent color, text scale, high contrast and transparency, UI language and formats, all
    live-updating (`UISettings`, `WM_SETTINGCHANGE`). Light/dark already arrives through WebView2. Needs 19, 23.
41. [S] Windows. File icons via `SHGetFileInfoW` / `IShellItemImageFactory`. Needs 23.
42. [S] Windows. Clipboard (`CF_HDROP`, `Preferred DropEffect` for cut) and drag-out of plain paths (OLE). Needs 18.
43. [M] Windows. Drag-out of virtual files (MTP, SMB, archives) via `CFSTR_FILEDESCRIPTORW` / `FILECONTENTS`. Needs 42.
44. [S] Windows. Open with, default app, text editors, terminal (`SHAssocEnumHandlers`, `wt -d`). Needs 18.
45. [S] Windows. SMB OS mounts via `WNetAddConnection2W` and Credential Manager SMB credentials. Needs 22.
46. [S] Windows. Small backend arms: `cmdr-nfs` identity (`GetComputerNameExW`, configurable uid/gid), `cmdr-adb`
    `adb.exe` discovery, special folders (`SHGetKnownFolderPath`), thread QoS (EcoQoS, not background mode), WinHTTP
    system proxy. Needs 8, 16.
47. [S] Windows. Crash minidumps (`crash-handler` + `minidumper`) and Windows-shaped crash cards in the api-server.
    Needs 23.
48. [XS] Windows. WebView2 specifics: `cmdr-media` URLs via `convertFileSrc` plus the `.localhost` CSP origin, and
    `instance-id.ts` Windows data (Roaming) and cache (Local) dirs in lockstep with the Rust mirror.
49. [S] Windows (taskbar progress is a win: Dock progress too). Shell integration: an "Open in Cmdr" Explorer verb,
    taskbar progress for long operations (`Window::set_progress_bar`), jump lists of recent folders.
50. [S] Windows, later (D3 step 3). `cmdr-update-trust` Authenticode check (`WinVerifyTrust` plus a pinned signer) for
    the signed direct download's updater artifact. Needs 33, 51.
51. [XS] Windows, David, later (D3 step 3). Azure Artifact Signing account and org validation for Rymdskottkärra AB,
    once Windows sales cover it; an agent writes the click-path. Then `signCommand` with OIDC `azure/login` in task 52's
    job and an Authenticode verify in `vet-update-archives`.
52. [M] Windows. Windows release build: a `windows-latest` job that builds once and packages twice, the MSIX (via
    Microsoft's `winapp` CLI) and the unsigned NSIS installer, plus the minisign `.sig` for the NSIS updater artifact in
    both signing modes and the `windows-x86_64` manifest row. Needs 6, 30.
53. [S] Windows. Release vetting and distribution: `deny.toml` triple, third-party notices and SBOM, checksums for both
    packages, and a winget manifest for the NSIS installer. Needs 52.
54. [S] Windows. E2E smoke on `windows-latest`: `tauri-plugin-playwright` over TCP, three-way helpers, a Windows fixture
    root, a small spec subset. Needs the app to run on Windows (34–36).
55. [M] Windows. Windows QA pass on a real machine: an agent drives the checklist and fixes what it finds; David QAs
    look, feel, and timing-sensitive UI.

56. [S] Windows. MTP via `mtp-rs`'s WPD backend (D2): Windows device arrival and removal for phones, the 5–6-minute
    driver stall shown as a "the phone is busy" state with a timeout (a dropped future is safe, later calls queue behind
    it, and a delete can still finish after we gave up), and a QA pass on a real phone. Needs 5.
57. [S] Windows, partly David. Microsoft Store channel (D3 step 1): the company developer account and listing (David
    opens the account and reviews the listing copy), Store submission automated from the release pipeline, the Store
    build's own updater off, and a check of where MSIX redirects Cmdr's data and cache folders so the data dir, the
    drive index, and "Delete all Cmdr data" all find them. Needs 52.
58. [XS] Windows. The website's "Run anyway" download (D3 step 2): the unsigned NSIS installer on the download page with
    a short explainer of SmartScreen's "More info → Run anyway" step, next to the Store link. David reviews the copy.
    Needs 32, 52.

### Later, not in the first Windows release

- OCR via `Windows.Media.Ocr`, CLIP on ONNX Runtime + DirectML.
- Explorer shell-extension verbs in Cmdr's own context menu (`IContextMenu`), preview handlers, Mica, ARM64 builds.
