# Windows portability audit: `crates/`

Scope: every crate under `crates/` (not `vendor/`), static reading at `7a199a9705` (2026-11-04). Classification per
concern:

- **(a)** unix-generic, needs a Windows arm.
- **(b)** macOS-only today, with a Linux arm already (Windows needs a third arm, or falls into the Linux one).
- **(c)** macOS-only, no Windows equivalent.
- **(d)** can be dropped or feature-gated off on Windows.

## Method and limits

- `rustup target list --installed` (2026-11-04): `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `wasm32-unknown-unknown`. **No `x86_64-pc-windows-msvc`**, so no `cargo check` ran; every "won't compile" below is
  from reading code and dependency sources, not from a compiler. Error counts per crate are unknown until a Windows
  check lane exists (task T1).
- `libc` on Windows (checked in `libc-0.2.191/src/windows/mod.rs`): has the MSVC CRT errno set (`EPERM`…`EILSEQ` plus
  the POSIX supplement `EADDRINUSE`…`EWOULDBLOCK`, `ENOTSUP`, `ETIMEDOUT`, `ENOTCONN`, `EHOSTUNREACH`, `ENETDOWN`, …)
  and a handful of CRT functions (`stat`, `access`). It has **no** `ESTALE`, `EHOSTDOWN`, `EDQUOT`, `EREMOTE`, no
  `statvfs`/`statfs`, `geteuid`/`getuid`/`getgid`, `gethostname`, `mmap`, `sigaction`, `renameat2`, `mkfifo`. The MSVC
  numbers also differ from macOS and Linux (`ETIMEDOUT` is 60 / 110 / 138), and `io::Error::raw_os_error()` on Windows
  returns a Win32 `GetLastError` code, not an errno at all.
- Crate liveness (crates.io API, 2026-11-04): `windows-sys` 0.61.2 and `windows` 0.62.2 (Microsoft), `notify` 8.2.1
  (2026-10-27), `trash` 5.2.9 (2026-09-13), `junction` 2.1.0 (2026-09-24), `normpath` 1.5.2, `dunce` 1.0.5, `file-id`
  0.2.3 (notify-rs, 2025-08), `same-file` 1.0.6 (BurntSushi, feature-complete, last release 2020), `proxy_cfg` 0.4.2
  (Devolutions, 2025-12, small), `ort` (2.0 still pre-release only, active 2026-10). ❌ `windows-acl` 0.3.0 is stale
  (2021): use `windows-sys` directly.

## Platform-site counts (all `.rs`, tests included)

Pattern: `target_os = "macos"|"linux"`, `cfg(unix)`, `not(unix)`, `std::os::unix`, `target_family`.

- cmdr-operations 308, cmdr-index 304, cmdr-fs 219, cmdr-local 92, cmdr-archive 18, cmdr-adb 7, cmdr-http 7, cmdr-restic
  7, cmdr-update-trust 7, cmdr-mtp 6, cmdr-git 4, cmdr-menu 3, cmdr-smb 3, the rest 0–2.
- Hot files: `cmdr-index/src/indexing/watch/watcher.rs` (53 incl. imports),
  `cmdr-index/src/indexing/resources/memory_watchdog.rs` (29), `cmdr-operations/src/transfer/copy_strategy.rs` (26),
  `cmdr-fs/src/process_memory/mod.rs` (19), `cmdr-local/src/reading.rs` (18), `cmdr-fs/src/metadata/staged.rs` (16).
- A large share is test code: cmdr-operations tests call `std::os::unix::fs::symlink` 51 times and `libc::geteuid` as a
  root check in five files.

## Headline findings

1. **The workspace won't compile for Windows for dependency reasons before any of our code is looked at.** `xattr` 1.7
   imports `std::os::unix::io` unconditionally (`xattr-1.7.0/src/lib.rs:34`; its "unsupported" module is for other
   unixes), `uzers` and `exacl` are unix-only, and all three are unconditional dependencies: `xattr` in cmdr-fs
   (`not(wasm)` section) and cmdr-local `[dependencies]`; `uzers` and `exacl` in cmdr-fs's `not(wasm)` section. `libc`
   is unconditional in cmdr-local and cmdr-nfs and `not(wasm)` in cmdr-fs.
2. **Gates encode "two platforms" as a binary.** `not(target_os = "macos")` means "Linux" in 58 non-test sites
   (cmdr-index 19, cmdr-fs 18, cmdr-local 8, cmdr-operations 6, …), `not(target_family = "wasm")` means "a unix host" in
   34 (cmdr-fs 29), and `any(macos, linux)` / `not(any(macos, linux))` appear 42 / 25 times as a stand-in for "a desktop
   host". Windows silently lands in the Linux arm, or in a stub arm nobody runs. A third platform should be a compile
   error where an arm is missing, not a quiet inheritance.
3. **`not(unix)` stub arms in cmdr-operations are data-safety hazards, not just gaps.** They compile on Windows and
   answer the unsafe way: `validation.rs:376` `is_same_file` → `false` (copy-onto-itself guard off), `validation.rs:298`
   `validate_destination_writable` → `Ok`, `rename/validity.rs:301` `same_local_file` → `false`,
   `leaf_validation.rs:298` `file_id` → `None`, `validation.rs:468` `is_same_filesystem`, `scan_walker.rs:491` (no
   hardlink dedup), `ledger.rs:180`, `source_binding.rs:127`. Each must get a real Windows answer before any Windows
   write path ships; the cleanest fix is one identity seam (S1) that has no "don't know, say no" default.
4. **Errno is the error vocabulary.** `VolumeError::{IoError, PermissionDenied}.raw_os_error: Option<i32>`
   (`cmdr-fs/src/volume/error.rs:67`, `:234`) is read as an errno by `friendly_error/errno.rs:19`/`:123`,
   `volume/transport_fault.rs:55`, `cmdr-operations/src/error_classification.rs:27`, `durable.rs:18`,
   `metadata/staged.rs:210`, `cmdr-local/src/local_posix/links.rs:15`, and network backends synthesize errnos with
   `libc::E*` (cmdr-nfs, cmdr-smb, cmdr-s3, cmdr-webdav, cmdr-sftp, cmdr-adb: 30+ sites). On Windows a local error
   arrives as a Win32 code, so every errno table misclassifies it, and `libc::EDQUOT`/`ESTALE`/`EHOSTDOWN`/`EREMOTE`
   don't even exist (cmdr-operations `error_classification.rs`, cmdr-index transports, cmdr-nfs).
5. **Good seams already exist and should be the template**: the `Volume` trait with `LocalPosixVolume` as one
   implementation (cmdr-local), `VisionBackend` with a fake (cmdr-index media), `SystemProxies` (cmdr-http),
   `LocalCopyStrategy` enum (cmdr-operations), `filesystem_kind::probe_mount_for_path`'s three-arm façade
   (`cmdr-local/src/filesystem_kind.rs:38`/`:52`/`:66`), `mapped_file_faults`' `use linux as platform` /
   `use macos as platform` (`cmdr-fs/src/mapped_file_faults/mod.rs:41`–`:49`), `metadata`'s
   `ClassOutcome::NotApplicable` model, and the `cmdr_mimalloc` build-script cfg (`cmdr-fs/build.rs`). The wasm build
   already proved the "host OS deps behind one gate" discipline works.
6. **The path model is POSIX strings.** `FileEntry.path: String`, mount prefixes `"/Volumes/"`, `"/mnt/"`, `"/media/"`
   (`cmdr-index/src/importance/classify.rs:378`, `importance/signals.rs:244`), and ~400 `'/'` idioms in non-test code
   (cmdr-index 263, cmdr-fs 98, cmdr-archive 16, cmdr-operations 13). Most are volume- or index-relative and fine. The
   load-bearing decision is app-level: present each Windows drive (and UNC share) as a volume rooted at `/`, with the
   `C:\…` translation inside the Windows local backend only. Remote backends already work this way, so the index and
   operations stay untouched. Out of scope for this audit beyond flagging it.
7. **Possible bug (Linux, today)**: `cmdr-index/src/media_index/scheduler/lifecycle.rs:231` wires `FakeVisionBackend` as
   the production backend off macOS. The fake derives OCR text, a tag, and an 8-float embedding from the file stem
   (`backend/fake.rs`). Media indexing is off by default, but a Linux user who turns it on gets invented OCR/tags
   written to their real `media.db`. Worth verifying; the fix (an "unavailable" backend that reports the feature off) is
   the same thing Windows needs (T13). Fixed in #943: off macOS the scheduler doesn't start.

## Per-crate inventory

### cmdr-fs (vocabulary + host primitives)

- **Dependencies**: `libc`, `xattr`, `uzers`, `exacl` under `not(wasm)` (`Cargo.toml:113`–`:135`): all four need
  `cfg(unix)` (finding 1). `rusqlite` bundled, `tokio`, `mimalloc` are Windows-fine.
- **Owner / group names** (`entry.rs:48`–`:55`, `uzers`): (a). Windows: owner SID via `GetSecurityInfo` +
  `LookupAccountSidW`; group has no meaningful analog, show empty or the primary group. `FileEntry.permissions: u32` is
  a POSIX mode by contract (`entry.rs:201`); Windows synthesizes one (readonly attribute → `0o444`/`0o644`, dir bit).
- **Node identity** (`volume/node_identity.rs:26`, `cfg(unix)` inode + birth time): (a). Windows:
  `GetFileInformationByHandleEx(FileIdInfo)` gives a 64-bit volume serial + 128-bit file ID (ReFS IDs don't fit `u64`);
  `created()` already works. ❗ `inode: u64` is too narrow for ReFS; S1 should widen the type.
- **Metadata carry** (`metadata/`): mode (`mod.rs:285`, `mode.rs:67`, `staged.rs:109`, `carried.rs:193`), ACL
  (`mod.rs:312`/`:324`/`:345`, `blockers.rs:70`/`:149`, `exacl`), BSD flags (`flags.rs:61`–`:130`, `chflags`,
  `UF_HIDDEN`/`UF_IMMUTABLE`/`SF_*`), birth date via `setattrlist` (`staged.rs:239`), xattrs (`carried.rs:212`–`:260`,
  `staged.rs:87`). Mode/ACL/xattr (a), flags and birth date (b).
  - Windows: hidden → `FILE_ATTRIBUTE_HIDDEN`, Locked → `FILE_ATTRIBUTE_READONLY` (❗ Windows refuses to delete a
    read-only file, so `clear_blockers` / `remove_*_clearing_blockers` gain a real Windows job), append-only and nodump
    (d). Birth time: `SetFileTime` / `std::fs::FileTimes::set_created` (`std::os::windows::fs::FileTimesExt`). ACL: DACL
    via `GetNamedSecurityInfoW`/`SetNamedSecurityInfoW`; recommend (d) at first and let the destination inherit, as
    Explorer does. Xattrs → NTFS alternate data streams (`FindFirstStreamW`, `path:stream`); `CopyFileExW` carries them
    itself.
- **Durability** (`durable.rs:15`): portable already (`sync_data` = `FlushFileBuffers`); the `ENOTSUP` fallback is
  macOS-only and fine.
- **Name folding / limits**: `name_fold.rs:67`/`:78` (NFD on macOS), `validation.rs:14`–`:18` (`MAX_PATH_BYTES` 4096 on
  Linux, 1024 "else": Windows gets macOS's number). (b). Windows: no normalization, case-insensitive via the volume's
  upcase table, names ≤ 255 UTF-16 units, paths 260 legacy or 32,767 with `\\?\`/long-path manifest, reserved names
  (`CON`, `NUL`, `COM1`…), forbidden `<>:"/\|?*`, no trailing dot or space. See S9: these rules belong to the
  destination filesystem, not the host OS.
- **Thread QoS** (`thread_qos.rs:67`–`:93`, `pthread_set_qos_class_self_np`): (b, no-op on Linux). Windows:
  `SetThreadInformation(ThreadPowerThrottling)` (EcoQoS) plus `THREAD_PRIORITY_BELOW_NORMAL`. ❌ Not
  `THREAD_MODE_BACKGROUND_BEGIN`: it also throttles I/O, the same trap the module's "why Utility, not Background" note
  avoids on macOS.
- **Thread / process CPU and memory** (`thread_cpu.rs:35`, `process_cpu/macos.rs`, `process_memory/mod.rs:66`–`:415`,
  `vm_regions.rs`, `rust_heap.rs`): Mach `task_info`, `proc_pidinfo`, malloc zones. Thread/process CPU (b-ish, Linux
  answers `None`) → `GetThreadTimes` / `GetProcessTimes`. Footprint → `K32GetProcessMemoryInfo` (`PrivateUsage`).
  Malloc-zone and VM-region census (c) → (d).
- **Mapped-file fault recovery** (`mapped_file_faults/`, `SIGBUS` + `mmap(MAP_FIXED)` remap): (b). Windows analog is
  `EXCEPTION_IN_PAGE_ERROR` via `AddVectoredExceptionHandler` plus `MapViewOfFileEx` at a fixed address: possible but
  delicate. Recommend (d): on Windows the consumers read through positioned reads instead of mapping removable or
  network files.
- **macOS-only modules**: `attr_buf.rs` (`getattrlistbulk` decoder), `cloud_provider.rs`, `file_provider.rs` (File
  Provider domain xattr), `tcc_paths.rs:183`/`:210` (TCC + `statfs` network check), `firmlinks.rs:64`/`:82`,
  `apple_double.rs:57`/`:70`, `icons/special_folders.rs:39`/`:45`. TCC, firmlinks, AppleDouble: (c) → (d). Cloud
  detection has a real Windows analog: Cloud Files placeholders show as `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` /
  `RECALL_ON_OPEN` / `OFFLINE` (+ `CfGetPlaceholderInfo`), which is cheaper than the macOS path. Special folders →
  `SHGetKnownFolderPath(FOLDERID_*)`, Recycle Bin.
- **Friendly errors** (`volume/friendly_error/errno.rs:19`/`:123`, `provider.rs:119`/`:146`): (b), errno-keyed; S2.
- **Structure**: platform code is mostly per-function `cfg` twins inside feature modules, with two exceptions that
  already use a platform submodule (`mapped_file_faults`, `process_cpu`). `testing/disk_images` is macOS-only by design.

### cmdr-local (the local disk)

- `LocalPosixVolume` is honest about being POSIX: the whole crate is unix (`reading.rs:17`
  `use std::os::unix::fs::{MetadataExt, PermissionsExt}` unconditional, `local_posix.rs:83`, `:904`–`:932`).
  Recommendation: keep it Unix-only and add a sibling `LocalWindowsVolume` behind the same `Volume` trait (T11), sharing
  the backend-agnostic parts (`mutation.rs`, the scan oracle, the `VolumeHost` wiring) through cmdr-fs's free bodies.
  Forcing one struct with `cfg` arms everywhere would recreate the scattered-gate problem.
- **No-replace rename** (`local_posix.rs:91`–`:131`, `renamex_np` / `renameat2`): (b). Windows: `MoveFileExW` _without_
  `MOVEFILE_REPLACE_EXISTING` is natively no-replace (`ERROR_ALREADY_EXISTS`). ❗ `std::fs::rename` REPLACES on Windows.
  Atomic replace:
  `SetFileInformationByHandle(FileRenameInfoEx, FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS)`,
  falling back to `ReplaceFileW`.
- **Reader** (`reading.rs`, `reading/bulk.rs`): `lstat` + `read_dir` (a), `getattrlistbulk` (c, macOS fast path).
  Windows: `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)` returns size, times, and attributes per entry
  in one call, a bulk lister comparable to `getattrlistbulk`; `std::fs::read_dir` already uses it, so
  `DirEntry::metadata()` is free on Windows. File ID and link count need `GetFileInformationByHandle` per file (only for
  hardlink dedup). `UF_HIDDEN` (`reading.rs:31`) → `FILE_ATTRIBUTE_HIDDEN`; `added_at` (macOS) → (d).
- **Space** (`space.rs`, `live_space.rs`, `statvfs`, NSURL): (b). Windows: `GetDiskFreeSpaceExW`.
- **Mounts** (`mount_table.rs` macOS `getfsstat`, `linux_mounts.rs`, `statfs.rs`, `filesystem_kind.rs`): (b). Windows:
  `GetLogicalDriveStringsW`, `GetDriveTypeW`, `GetVolumeInformationW` (fs name: NTFS / ReFS / FAT32 / exFAT),
  `FindFirstVolumeW` + `GetVolumePathNamesForVolumeNameW` for folder-mounted volumes. `filesystem_kind` already has the
  right façade shape; `mount_table` vs `linux_mounts` do not (S5).
- **Write-access probe** (`local_posix.rs:904`–`:932`, `statvfs ST_RDONLY` + `access(W_OK)`): (a). Windows:
  `GetVolumeInformationW` `FILE_READ_ONLY_VOLUME`, plus `AccessCheck` or a try-create.
- **Symlinks** (`local_posix/links.rs`): (a). Windows: `std::os::windows::fs::{symlink_file, symlink_dir}` (the kind
  must be known up front), creation needs Developer Mode or `SeCreateSymbolicLinkPrivilege`
  (`SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE`); junctions via `junction`. Reparse points also cover OneDrive
  placeholders, so "is a link" must check the reparse tag, not just the attribute.
- **Xattrs / Finder tags** (`local_posix/xattrs.rs`, `xattr`): (a) for the mechanism (ADS), (d) for Finder tags.
- **Metadata caps** (`local_posix.rs:299`/`:308`): a third `METADATA_CAPS` for Windows.

### cmdr-operations (the write engine)

- **Local copy** (`transfer/copy_strategy.rs:30`–`:290`, `macos_copy.rs` `copyfile`/clone, `linux_copy.rs`
  `copy_file_range`, `chunked_copy.rs`): (b). The `LocalCopyStrategy` enum is a good seam and already has a `StdCopy`
  third arm. Windows: `CopyFileExW` (progress callback, cancel flag, `COPY_FILE_NO_BUFFERING` for large files, carries
  ADS and attributes); block clone on ReFS / Dev Drive via `FSCTL_DUPLICATE_EXTENTS_TO_FILE` (Windows 11 24H2's
  `CopyFile` clones on its own there). ❗ `chunked_copy.rs:10` imports `OpenOptionsExt` unconditionally while the module
  is ungated (`transfer/mod.rs:13`), and `copy/folder_metadata.rs:14` likewise: compile blockers.
- **Trash** (`delete/trash.rs:45`/`:147`/`:167`, NSFileManager on macOS, `trash` on Linux, stub else): (b). Windows: the
  `trash` crate (already in the graph for Linux) has a Windows backend over `IFileOperation`. `trash_dir_for_path`
  (`:276`/`:304`) and the "trashed item is still the one trashed" identity rule need a Recycle Bin answer
  (`$Recycle.Bin\<SID>\$R…` names; `trash::os_limited` lists and restores on Windows).
- **Cloud trash** (`delete/cloud_trash.rs:57`–`:196`, `SF_DATALESS`, provider roots): (b with a stub). Windows: Cloud
  Files attributes, as in cmdr-fs.
- **Quarantine propagation** (`transfer/volume/extract_quarantine.rs:177`–`:197`, `state.rs:294`): (b with a no-op Linux
  arm). Windows has a direct analog worth doing: Mark of the Web, the `Zone.Identifier` ADS, which extraction should
  carry from the archive onto its contents, as Explorer does.
- **Identity and devices** (`ledger.rs:172`, `leaf_validation.rs:292`, `source_binding.rs:118`,
  `validation.rs:360`/`:458`, `rename/validity.rs:293`, `scan_walker.rs:243`/`:483`, `scan_dry_run.rs:213`,
  `durability.rs:189`, `mount_boundary.rs:35`/`:143`, `move_op/source_sweep.rs:508`, `copy_strategy.rs:66`–`:84`): (a),
  14 sites, some unconditional (compile blockers), the gated ones with unsafe `not(unix)` stubs (finding 3). S1.
- **Write-access and flags** (`validation.rs:227`–`:298`, `rename.rs:674`–`:713`, `error_classification.rs:116`/`:124`
  `is_locked_item`, `locked_sources.rs`): (a)/(b). Windows: read-only attribute is "locked"; directory write access via
  `AccessCheck` or a probe create.
- **Free space** (`free_space.rs:78`–`:145`, `statvfs` + NSURL important-usage): (a)/(b). Windows: `GetDiskFreeSpaceExW`
  (`lpFreeBytesAvailableToCaller` already honors quotas).
- **Mount table** (`transfer_sides.rs:259`–`:270`, `mount_boundary.rs:79`–`:90`): (b), callers pick
  `cmdr_local::mount_table` vs `linux_mounts` by `cfg`. S5.
- **Error classification** (`error_classification.rs:20`–`:110`): (a), errno table incl. `EDQUOT` (no such const on
  Windows). Windows-native classes with no POSIX twin: `ERROR_SHARING_VIOLATION` (32) / `ERROR_LOCK_VIOLATION` (33),
  "file in use by another app", which is the most common Windows copy/delete refusal and needs its own typed variant and
  copy. S2.
- **macOS-only bits**: `create_numbered` (`lib.rs:96`), `status_cache.rs:260` external-volume ops: check what they gate
  when T12 runs.
- **Tests**: 51 `symlink` calls, `geteuid` root checks (`validation.rs:504`, `delete_integration_test.rs:69`,
  `move_integration_test.rs:127`, `validation_integration_test.rs:334`), `UnixListener` + `mkfifo`
  (`validation_integration_test.rs:684`/`:717`). S10.

### cmdr-index (file, media, importance indexes)

- **Dependencies**: `notify` is `cfg(linux)` only (`Cargo.toml:88`), `libc` `cfg(unix)`, the objc2 / Vision / Core ML /
  `cmdr-fsevent-stream` set `cfg(macos)`. Already well-gated; Windows needs `notify` widened.
- **Drive watcher** (`indexing/watch/watcher.rs:84` macOS FSEvents struct, `:260` Linux notify struct, `:548` stub;
  `supports_event_replay()` `:77`; `watcher/journal.rs:4`–`:80`, `:58` `dev()`): (b). Three duck-typed structs with the
  same method set. Windows: `notify`'s `ReadDirectoryChangesW` backend (recursive is native, so a root watch works like
  macOS, unlike Linux's per-branch rule). Overflow surfaces as `ERROR_NOTIFY_ENUM_DIR` → rescan. Journal replay: the
  NTFS USN journal is the only analog and reading it needs an admin volume handle → (d), rescan at start as Linux does.
  S4.
- **Scanner** (`scanner/walker/mod.rs:84`/`:353` `bulk_read` macOS, `scanner/mod.rs:257` `device_of`,
  `scanner/exclusions.rs` macOS system paths, `indexing/metadata.rs:40`–`:152` ino/nlink with `not(unix)` arms):
  (a)/(b). Windows: `FindFirstFileExW` large fetch as the bulk lister; exclusions need a Windows list (`C:\Windows`,
  `$Recycle.Bin`, `System Volume Information`, `pagefile.sys`, …).
- **Transports gated by OS** (`lifecycle/state/startup.rs:605`/`:623`/`:641`/`:667`,
  `handle/mod.rs:290`/`:313`/`:781`/`:792`/`:811`, `transports/smb/watch.rs:52`–`:420`): `any(macos, linux)` gates on
  SMB/MTP/ADB indexing. These transports are pure Rust, so the gate means "a supported desktop", not anything
  SMB-specific. On Windows they should simply be on; the alias in S3 removes the ambiguity.
- **Collation** (`indexing/store/collation.rs:24`/`:32`): (b). Windows: `CompareStringOrdinal(…, bIgnoreCase)` matches
  NTFS's case-insensitivity.
- **Paths** (`importance/classify.rs:378`–`:385`, `importance/signals.rs:244`–`:253`, `paths/routing.rs:286`): (b),
  mount prefixes as string constants. Belongs behind the mount façade (S5).
- **Memory watchdog** (`indexing/resources/memory_watchdog.rs:35`–`:276`): macOS-only; ❗ Linux has no watchdog at all
  today. The thresholds and state machine are portable; only the footprint read is per-OS. S6 (a clear Linux win).
- **Media ML**: Vision OCR/tags (`media_index/backend/vision/`), CLIP on Core ML (`media_index/clip/macos.rs`,
  `clip/install.rs`), thermal (`media_index/thermal.rs:55`), autorelease pools (`writer/mod.rs:1319`,
  `network_scanner/mod.rs:380`): (c) → (d). Windows has partial analogs: `Windows.Media.Ocr` (WinRT through the
  `windows` crate) for OCR, ONNX Runtime (`ort`, pre-release) + DirectML for CLIP, no public thermal API. Recommend
  dropping on Windows first behind an explicit "unavailable" backend (T13), and revisiting OCR later as its own effort.
- **Importance last-used** (`importance/last_used.rs:46`–`:171`, Spotlight `kMDItemLastUsedDate`): (c) → (d). NTFS
  last-access updates are system-managed and often off, so there's no reliable signal.

### cmdr-archive

- `xattr` is `cfg(macos)` (`Cargo.toml:85`), AppleDouble writing in `mutation/mutator.rs:358`/`:374` has a `not(macos)`
  arm, `read/source.rs` and the `not(wasm)` gates are host-I/O only. `watch/mod.rs:33` uses `notify` (portable). Little
  to do: confirm the `not(wasm)` sections compile on Windows (T1) and add Mark-of-the-Web on extract (T12).

### cmdr-http (proxy routing)

- `system.rs:14`–`:27` `MacSystem` with a `Direct` non-macOS arm, `pac.rs` and `entries.rs:131` CFNetwork: (b/c). The
  `SystemProxies` trait is the seam. ❗ Because `Proxy::custom` turns reqwest's own system reader off (`CLAUDE.md`),
  Windows gets NO system proxy today, even though reqwest alone would read the registry. Windows arm:
  `WinHttpGetIEProxyConfigForCurrentUser` + `WinHttpGetProxyForUrl` (evaluates PAC/WPAD itself, so no `pac.rs` run-loop
  thread). Rename `MacSystem` → `OsSystem` with per-OS submodules (T10).

### cmdr-update-trust

- `archive.rs`, `code_signature.rs` (Security framework `SecStaticCode`), `main.rs:18`/`:54`: (c). Windows needs its own
  trust check: Authenticode via `WinVerifyTrust` plus a pinned signer (subject or thumbprint) on the installer, and
  vetting of whatever artifact the Windows updater downloads (NSIS/MSI, not a tarball). Separate small crate module
  (T14); nothing to share with the macOS code beyond the refusal-enum shape.

### Network and device backends

- **cmdr-nfs**: `dial.rs:110` `gethostname`, `:122` `getuid`/`getgid` (no Windows `libc` symbols) for `AUTH_SYS`; errno
  synthesis incl. `EREMOTE` (missing on Windows). (a). Windows: `GetComputerNameExW` (or the `gethostname` crate), a
  configurable uid/gid (default 65534) since Windows has no POSIX identity.
- **cmdr-smb**: `libc` only in dev-deps (`getpeername`, `sockaddr_*` in `session_integration_test.rs`); production is
  pure Rust. Errno synthesis (4 sites) → S2.
- **cmdr-adb**: `server.rs:252` `is_executable` is `cfg(unix)` with no other arm: Windows needs `adb.exe` discovery
  (`PATHEXT`, `%LOCALAPPDATA%\Android\Sdk\platform-tools`). (a), XS.
- **cmdr-mtp**: 6 sites, mostly `any(macos, linux)`; MTP on Windows normally goes through WPD, but our `mtp-rs` talks
  USB directly, which on Windows needs a WinUSB/libusb driver binding for the device (Zadig-style). That's a product
  question, not a crate refactor: flag (d) for a first Windows release.
- **cmdr-git**: `repo.rs:244`–`:254` `is_on_the_startup_disk` compares `dev()` unconditionally (compile blocker), S1.
  `notify` use is portable.
- **cmdr-restic, cmdr-s3, cmdr-sftp, cmdr-webdav, cmdr-listing, cmdr-menu**: test-only or errno-synthesis sites. S2 and
  S10 cover them.
- **fsevent-stream**: macOS-only by nature; already only depended on under `cfg(macos)`. (c), nothing to do.
- **cmdr-brief-columns, cmdr-collation, cmdr-media-meta, cmdr-operation-log, cmdr-listing-wasm, index-query,
  operation-log-dump**: no platform code found.

## Seams worth building (each a win without Windows)

- **S1. One file-identity vocabulary in cmdr-fs** (`cmdr_fs::host_fs` or similar): `FileId { volume, file }`,
  `same_node(a, b)`, `device_of(path)`, `link_count`, built off `std::fs::Metadata` on Unix and a handle query on
  Windows. Replaces ~30 scattered `dev()`/`ino()`/`nlink()` sites across cmdr-operations, cmdr-local, cmdr-index,
  cmdr-git. Why it wins today: the "same thing?" question is asked three different ways now (with dev, without dev as
  `NodeIdentity` deliberately does, dev-only for "same filesystem"), and a typed vocabulary makes the difference a name
  instead of a re-derivation; it also deletes the unsafe `not(unix)` defaults (finding 3) instead of patching each.
- **S2. Typed OS-error classes at the boundary**: classify once in `VolumeError::from_io_at` into an enum (transport
  dropped, read-only, no space / quota, name too long, device gone, permission EACCES vs EPERM, in use, not supported,
  other), carrying the raw code only for logs. Consumers (`error_classification.rs`, `friendly_error/errno.rs`,
  `transport_fault.rs`, `links.rs`, `durable.rs`, `staged.rs`) match the class; network backends construct the class
  instead of fabricating `libc::EIO`. Why it wins today: the macOS and Linux errno tables in `friendly_error/errno.rs`
  are already two copies, the errno numbers differ per OS so any raw errno crossing IPC or a log is platform-relative,
  and it's the type-level version of the "never classify by string" rule.
- **S3. Named platform cfgs** in a build script (precedent: `cmdr_mimalloc`): for example `cmdr_apple`, `cmdr_unix_host`
  (`unix` and not wasm), `cmdr_host` (not wasm). Plus a check that refuses `not(target_os = "macos")` in non-test code
  (forcing an explicit list of arms) so a new target is a compile error where an arm is missing. Why it wins today: the
  58 `not(macos)` arms are "Linux" only by accident, and the `not(wasm)` gate already conflates "has a host OS" with "is
  unix".
- **S4. A watcher backend trait** in cmdr-index: `FsEventsBackend` and `NotifyBackend` behind one `DriveWatcher`, with
  `supports_event_replay` a backend property. `NotifyBackend` becomes `not(macos)` (Linux + Windows). Why it wins today:
  three duck-typed structs in one 600-line file kept in sync by hand, and the existing `fake_journal` test seam becomes
  a third backend instead of a `#[cfg(test)]` early return (`watcher.rs:116`).
- **S5. One mount-table façade in cmdr-local** (`mounts::is_mount_point`, `mounts::list`,
  `mounts::user_mount_prefixes`), shaped like `filesystem_kind.rs`. Callers in cmdr-operations (`transfer_sides.rs:259`,
  `mount_boundary.rs:79`) and cmdr-index (`classify.rs:378`, `signals.rs:244`) stop naming the OS. Why it wins today:
  four callers duplicate the two-arm dispatch and the prefix lists.
- **S6. Process stats as one module with per-OS readers**: footprint, CPU, thread CPU; the memory watchdog consumes the
  portable reading. Why it wins today: Linux gets the watchdog it lacks (`/proc/self/status` `VmRSS` or `smaps_rollup`).
- **S7. Metadata classes with per-OS bodies**: move the per-class `cfg` twins in `cmdr-fs/src/metadata/` into
  `metadata/platform/{macos,linux}.rs` behind the existing class API, plus the caps constant from cmdr-local. Moderate
  win: the ordering rules stay in `mod.rs`, readable without the OS noise.
- **S8. Honest manifests**: `xattr`, `uzers`, `exacl`, `libc` under `cfg(unix)`; `notify` under `not(macos)` in
  cmdr-index. Small win, needed for any Windows check.
- **S9. Name and path validity per destination filesystem**: `cmdr-fs/src/validation.rs` and
  `cmdr-operations/src/rename/validity.rs` key limits on the host OS today. An exFAT stick or an SMB share on a Mac
  already enforces the Windows rules (forbidden characters, reserved names, trailing dots), so keying on the
  destination's `filesystem_kind` fixes real macOS cases and makes Windows a data entry. Verify current behavior on
  exFAT before sizing.
- **S10. Portable test helpers**: `testing::symlink`, `testing::is_root`, special-file fixtures, so tests gate on a
  capability (`can_create_symlinks()`) instead of `cfg(unix)`. Win: dedup of ~60 call sites.

## Proposed tasks

Sizes per the board (XS < 200k tokens, S 200–400k, M one agent + ≤ 3 subagents, L 4–15 agents). "Win" = worth doing even
if Windows never ships.

1. **T1, XS, win (enabler)**: S8 manifests + wrap the unconditional `std::os::unix` uses in ungated modules
   (`cmdr-local/src/reading.rs:17`, `cmdr-operations/src/transfer/chunked_copy.rs:10`,
   `transfer/copy/folder_metadata.rs:14`, `mount_boundary.rs:35`, `durability.rs:189`, `scan_dry_run.rs:213`,
   `scan_walker.rs:243`, `move_op/source_sweep.rs:508`, `cmdr-git/src/repo.rs:245`), plus a CI lane on a
   `windows-latest` runner that runs `cargo check --target x86_64-pc-windows-msvc` per crate and ratchets an allowlist
   of crates that must compile (pure-protocol crates first). ❗ A Linux/macOS host can't check the MSVC target for
   crates with C build scripts (`rusqlite` bundled, `mimalloc`), so the lane needs a Windows runner. Without the lane,
   nothing below can be verified.
2. **T2, S, win**: S3 named cfgs + the `not(macos)`-in-production check, converting the 58 + 67 gates.
3. **T3, S, win**: S1 file identity (widen to 128-bit file IDs), replacing the ~30 sites and the `not(unix)` stubs.
4. **T4, M, win**: S2 typed OS-error classes across cmdr-fs, cmdr-operations, cmdr-local, and the network backends (one
   agent for the core, subagents for the backend sweep).
5. **T5, S, win**: S5 mount façade.
6. **T6, S, win**: S4 watcher backend trait; Windows then needs only the `notify` dep widened and a smoke test.
7. **T7, S, win (Linux gains)**: S6 process stats + portable memory watchdog.
8. **T8, S, win**: S9 destination-filesystem validity (after a quick exFAT/SMB behavior check).
9. **T9, XS, win**: S10 test helpers.
10. **T10, XS, partial win**: cmdr-http `MacSystem` → `OsSystem` with per-OS submodules (win: naming and structure); the
    WinHTTP arm is a follow-up XS once T1's lane exists.
11. **T11, L, Windows-only**: `LocalWindowsVolume` in cmdr-local: reader (`FindFirstFileExW`), space, no-replace and
    atomic-replace rename, streams (`sync_data`, no dir fsync), links (file vs dir symlinks, junctions, reparse tags),
    ADS-backed xattrs, write-access probe, metadata caps, conformance suite green. Blocked on the app-level path-model
    decision (finding 6) and on T3/T4.
12. **T12, M, Windows-only**: cmdr-operations Windows arms: `CopyFileExW` strategy (+ block clone), trash via `trash`
    with Recycle Bin identity, read-only-as-locked in `clear_blockers`, sharing-violation as a typed refusal, Cloud
    Files placeholders, Mark of the Web on extract (cmdr-archive too).
13. **T13, XS, win (fixes the Linux fake-backend issue)**: an explicit "unavailable" `VisionBackend` and CLIP backend
    off macOS so the scheduler reports media indexing unavailable instead of running the fake; gate thermal, last-used,
    TCC, firmlinks, AppleDouble as (d) on Windows.
14. **T14, S, Windows-only**: cmdr-update-trust Authenticode check (`WinVerifyTrust` + pinned signer) for the Windows
    updater artifact.
15. **T15, XS, Windows-only**: small backend arms: cmdr-nfs identity (`GetComputerNameExW`, configurable uid/gid),
    cmdr-adb `adb.exe` discovery, cmdr-fs special folders (`SHGetKnownFolderPath`), thread QoS (EcoQoS).

Suggested order: T1 → T2 → T3/T4 in parallel → T5/T6/T7/T9 in parallel → T8, T10, T13 → the Windows-only T11 → T12 →
T14/T15. Everything through T10 and T13 lands on its own merit with Windows still unshipped.

## Open questions for the app-level audit

- The Windows path model (finding 6): drive-as-volume-rooted-at-`/` vs native `C:\` strings in `FileEntry.path`. This
  decides how much of the ~400 `'/'` idioms matter.
- Whether MTP ships on Windows at all (driver binding for direct USB vs a WPD backend).
- Volume arrival and removal events (`WM_DEVICECHANGE` / `CM_Register_Notification`) live in the app, not these crates,
  but S5's façade is where their answers should land.
