# Volume abstraction

This module defines the `Volume` trait — the core abstraction for all storage backends in Cmdr — and the `VolumeManager` registry.

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3, FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

| File | Role |
|---|---|
| `mod.rs` | `Volume` trait (async — most methods return `Pin<Box<dyn Future>>`; sync: `name`, `root`, `supports_*`, `local_path`, `space_poll_interval`), `VolumeScanner`, `VolumeWatcher`, `VolumeReadStream` traits, `MutationEvent` enum, shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`, `SourceItemInfo`) |
| `friendly_error.rs` | User-facing error messages: `FriendlyError`, `ErrorCategory`, errno mapping. See [Friendly error system](#friendly-error-system) below. |
| `provider.rs` | Provider detection and enrichment: `Provider` enum (19 variants), `detect_provider()`, `provider_suggestion()`, `enrich_with_provider()`. Re-exported via `friendly_error.rs`. |
| `manager.rs` | `VolumeManager` — thread-safe `RwLock<HashMap>` registry; supports a default volume |
| `local_posix.rs` | `LocalPosixVolume` — real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info. |
| `mtp.rs` | `MtpVolume` — MTP device storage; async `Volume` trait with direct async MTP calls. Uses `MtpReadStream` for streaming (calls `FileDownload::next_chunk().await` directly). Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb.rs` | `SmbVolume` — SMB share storage; async `Volume` trait with direct async smb2 calls. Uses `tokio::sync::Mutex<Option<(SmbClient, Tree)>>` + `AtomicU8` connection state. Also contains `connect_smb_volume()`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb_watcher.rs` | Background SMB change watcher (`run_smb_watcher`). Owns a dedicated smb2 connection for `CHANGE_NOTIFY`, debounces events, feeds `notify_directory_changed`. Spawned by `connect_smb_volume()`. |
| `in_memory.rs` | `InMemoryVolume` — `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`) |

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>  (async trait — most methods return Pin<Box<dyn Future>>)
        ├─ LocalPosixVolume   → real FS (spawn_blocking for I/O), FSEvents watcher, jwalk scanner
        ├─ MtpVolume          → direct async MTP ops
        ├─ SmbVolume          → direct async smb2 ops (direct protocol, not OS mount)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

`VolumeScanner` and `VolumeWatcher` are separate sub-traits returned by `Volume::scanner()` and `Volume::watcher()`. Only `LocalPosixVolume` implements both today.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `supports_watching()` — enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the `VolumeWatcher` trait used for drive indexing). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()` — enables copy/move UI. Local, MTP, and SMB return `true`.
- `supports_streaming()` — enables cross-volume transfers via `open_read_stream` / `write_from_stream`. `MtpVolume` and `SmbVolume` return `true`. The streaming path is the universal fallback for any non-local volume pair — future volume types (FTP, S3) just implement these two methods to get cross-volume copy for free.
- `local_path()` — returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations. `SmbVolume` returns `None` so copies go through smb2 instead of the slow OS mount.
- `supports_local_fs_access()` — whether `std::fs` operations (stat, read_dir) work on this volume's paths. Default `true`. `MtpVolume` and `SmbVolume` return `false`. Used to skip the legacy synthetic entry diff path (now superseded by `notify_mutation`).
- `notify_mutation(volume_id, parent_path, mutation)` — called after a successful mutation (create, delete, rename) to update the listing cache immediately. Default impl uses `std::fs` (works for `LocalPosixVolume`). `SmbVolume` and `MtpVolume` override to use their own protocol's `get_metadata`. Fire-and-forget, no error propagation.
- `smb_connection_state()` — returns `Some(SmbConnectionState)` for SMB volumes (green/yellow indicator in volume picker). Default `None`. Only `SmbVolume` implements it.
- `on_unmount()` — lifecycle hook called before unregistration. `SmbVolume` uses it to disconnect its smb2 session. Default is no-op.
- `scanner()` / `watcher()` — drive indexing hooks; `None` by default.
- `export_to_local(source, dest, on_progress)` / `import_from_local(source, dest, on_progress)` — always take an `on_progress` callback `&(dyn Fn(u64, u64) -> ControlFlow<()> + Sync)`. Callers that don't need progress pass a no-op (`&|_, _| ControlFlow::Continue(())`). `SmbVolume` reports per-chunk progress via smb2's `read_file_with_progress`/`write_file_with_progress`. `MtpVolume` reports export progress via `download_file_with_progress` (import doesn't report yet). `LocalPosixVolume` and `InMemoryVolume` ignore the callback (OS-native copy APIs).
- `space_poll_interval()` — recommended interval for the live disk-space poller (`space_poller.rs`). Default 2 s (local volumes). `SmbVolume` and `MtpVolume` override to 5 s. `InMemoryVolume` returns `None` (no polling). The poller uses this to tick each volume at its own cadence.

## Path handling gotchas

- **`LocalPosixVolume::resolve`**: accepts empty, `.`, relative, or absolute paths. Three-way branch for absolute paths: (1) already starts with volume root — used as-is, (2) volume root is `/` — absolute path passed through unchanged, (3) otherwise — leading `/` stripped and joined to root. This handles frontend sending full absolute paths.
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the bare relative path the MTP library expects.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## SMB auto-upgrade lifecycle

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in two scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts`): Scans registered volumes for `smbfs` type. Waits for mDNS
   discovery to reach `Active` state (polls every 500ms, up to 15s) because Keychain credentials are keyed by hostname
   (from mDNS), not IP (from `statfs`). Uses `tauri::async_runtime::spawn` (not `tokio::spawn` — runs during `setup()`
   before Tokio is fully available). Emits `volumes-changed` after upgrades so the frontend refreshes indicators.

2. **Mount detection** (`volumes/watcher.rs::try_upgrade_smb_mount`): When FSEvents detects a new volume in `/Volumes/`
   and it's `smbfs`, spawns a background upgrade attempt. By this point mDNS is already active.

Both paths check the `network.directSmbConnection` setting (global `AtomicBool`). Both are best-effort — failures log a
warning and the volume stays as `LocalPosixVolume`. The "Connect directly" UI action (`upgrade_to_smb_volume` command)
provides a manual upgrade path.

## Friendly error system

`friendly_error.rs` turns raw OS errors into warm, actionable messages so the user feels supported when something goes
wrong. This is one of Cmdr's UX differentiators: where other file managers show "I/O error: Operation timed out (os
error 60)", we show a friendly title, a plain-language explanation, and provider-specific advice ("This folder is managed
by **MacDroid**. Here's what to try: ...").

### Philosophy

**The user should never feel alone with a broken state.** Every error message should feel like the app is putting its
hand on the user's shoulder and saying "Here's what happened, and here's what you can do." We go above and beyond: we
detect which cloud provider or mount tool manages the path, and tailor the suggestion to that specific app. A timeout on
a Dropbox folder gets different advice than a timeout on an SSHFS mount.

Power users also need the raw details (errno name, code) for debugging or bug reports. These are available in a
collapsible "Technical details" section, never hidden but never in your face either.

### Architecture

Two-layer mapping across two files:

1. **`friendly_error_from_volume_error(err, path)`** (`friendly_error.rs`) — maps `VolumeError` variants and macOS errno
   codes (37 codes) to a `FriendlyError` with category (Transient/NeedsAction/Serious), title, explanation, suggestion,
   and raw detail.
2. **`enrich_with_provider(error, path)`** (`provider.rs`, re-exported from `friendly_error.rs`) — detects 19
   cloud/mount providers from path patterns and `statfs` filesystem type, then overwrites the suggestion with
   provider-specific advice.

The frontend receives the fully-baked `FriendlyError` struct via the `listing-error` Tauri event and renders it with
category-based visual styling. The frontend never sees errno codes or does OS-specific logic.

### Adding a new error message

When you need to handle a new errno or `VolumeError` variant:

1. Add the match arm in `friendly_error_from_volume_error`
2. Pick the right `ErrorCategory`: **Transient** (retry might work), **NeedsAction** (user must do something),
   **Serious** (something is genuinely broken)
3. Write the message following the rules below
4. Add a unit test asserting the category and that the text follows the style rules
5. Run the existing `error_messages_never_contain_error_or_failed` test to catch violations

### Adding a new provider

When a new cloud storage or mount tool becomes popular enough to detect:

1. Add a variant to the `Provider` enum with `display_name()` and `app_name()`
2. Add path detection in `detect_provider` (CloudStorage prefix, specific path, or `statfs` type)
3. Write provider-specific suggestions in `provider_suggestion` for each `ErrorCategory`
4. Add a unit test for path detection and suggestion content
5. Update `volumes/CLAUDE.md` provider table to keep the two lists in sync

### Writing rules for error messages

These are non-negotiable. The existing test suite enforces some of them automatically.

- **NEVER use "error" or "failed"** in titles, explanations, or suggestions. Say "Couldn't read" not "Read error". The
  automated test `error_messages_never_contain_error_or_failed` catches this.
- **Active voice, contractions**: "Cmdr couldn't..." not "The operation was unable to..."
- **No trivializing**: no "just", "simply", "easy", "all you have to do"
- **No permissive language**: "Check your connection" not "You might want to check..."
- **Direct and warm**: "Here's what to try:" not "Please attempt the following remediation steps:"
- **No em dashes**: use parentheses, commas, or new sentences
- **Sentence case in titles**: "Connection timed out" not "Connection Timed Out"
- **Bold key terms** with `**` only when it helps scanning (for example, provider names)
- **Platform-native terms**: "System Settings" on macOS, "Finder", "Trash"
- **Keep it short**: max two sentences for explanation, bullets for suggestions

Good example:
```
title: "Connection timed out"
explanation: "Cmdr tried to read this folder but the connection didn't respond in time."
suggestion: "Here's what to try:\n- Check that the device or server is reachable\n- ..."
```

Bad example (every rule violated):
```
title: "I/O Error: Operation Timed Out"   // "Error", Title Case
explanation: "An error occurred while the system attempted to access the directory."  // passive, "error"
suggestion: "You may want to try simply reconnecting the device."  // permissive, trivializing
```

### Provider detection strategies

| Strategy | Providers covered |
|---|---|
| `~/Library/CloudStorage/<Prefix>*` | Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic fallback for unrecognized providers |
| `~/Library/Mobile Documents/` | iCloud Drive |
| `/Volumes/pCloudDrive` | pCloud (FUSE virtual drive) |
| `/Volumes/veracrypt*` | VeraCrypt |
| `~/.CMVolumes/` | CloudMounter |
| `statfs` `f_fstypename` (macOS) | macFUSE/SSHFS/Cryptomator/rclone (`macfuse`, `osxfuse`), pCloud (`pcloudfs`) |

The `statfs` check runs only at error time (not on every listing), so the syscall cost is negligible.

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Key decisions

**Decision**: Trait with optional methods defaulting to `NotSupported`/`false`
**Why**: New volume types (SMB, S3, FTP) will have vastly different capability sets. Forcing every implementor to stub out every method would be noisy and error-prone. Defaults let new backends start with just `list_directory` + `get_metadata` and opt in to capabilities incrementally. The alternative — a capabilities bitfield — would require runtime checks everywhere and couldn't express return-type differences.

**Decision**: `VolumeScanner` and `VolumeWatcher` are separate sub-traits, not part of `Volume`
**Why**: Scanning and watching have their own lifetimes, threading models, and state (handles, channels). Folding them into `Volume` would force every volume to carry scanner/watcher state even if it never indexes. Returning `Option<Box<dyn VolumeScanner>>` keeps the core trait lightweight.

**Decision**: `VolumeManager` uses `RwLock<HashMap>` (not `DashMap` or `Mutex`)
**Why**: Volume registration/unregistration is rare (mount/unmount events); reads are frequent (every file operation resolves a volume). `RwLock` gives concurrent read access without pulling in an extra dependency. `DashMap` would work but is heavier than needed for a registry that rarely exceeds ~10 entries.

**Decision**: `VolumeManager::register_if_absent` for watcher registrations
**Why**: When the mount flow pre-registers an `SmbVolume`, the FSEvents watcher would overwrite it with a `LocalPosixVolume` via `register`. `register_if_absent` is a no-op if a volume is already registered, preserving the `SmbVolume`. The existing `register` (overwrite) is kept for explicit replacement (like SmbVolume replacing itself on reconnect).

**Decision**: `Volume` trait is async (methods return `Pin<Box<dyn Future>>`)
**Why**: MTP and SMB operations are inherently async (USB bulk transfers, network I/O). The previous sync trait required `block_on` bridges that risked nested-runtime panics in cross-volume streaming. The async trait lets MTP and SMB call their async backends directly. `LocalPosixVolume` wraps its blocking I/O in `spawn_blocking`. Sync-only methods (`name()`, `root()`, `supports_*()`, capability flags) remain non-async.

**Decision**: `VolumeError` stores `String` messages, not the original `std::io::Error`
**Why**: `std::io::Error` is not `Clone`, but `VolumeError` needs to be `Clone` for ergonomic error propagation across thread boundaries and for serialization to the frontend. Storing the formatted message loses the original error type but keeps the information that matters for user-facing error messages. The `IoError` variant also carries `raw_os_error: Option<i32>` so the friendly error mapper can match on platform-specific errno codes.

**Decision**: Friendly error mapping is two layers: errno mapping, then provider enrichment
**Why**: Not every provider+errno combination needs custom copy. The base errno message is always useful. Provider enrichment is additive, making the suggestion more specific when we recognize who manages the mount. Keeping them separate avoids a combinatorial explosion of messages.

**Decision**: Friendly error mapping lives in Rust, not the frontend
**Why**: The mapping needs access to the full path (for provider detection) and platform-specific errno codes. Doing it in Rust keeps the frontend thin (principle: smart backend, thin frontend) and avoids duplicating errno knowledge in TypeScript. The frontend receives a ready-to-render `FriendlyError` struct with markdown strings.

**Decision**: `LocalPosixVolume` uses `symlink_metadata` for `exists()` instead of `Path::exists()`
**Why**: `Path::exists()` follows symlinks — a dangling symlink returns `false`, which would make the volume claim a file doesn't exist when it visibly does in a directory listing. `symlink_metadata` detects the symlink itself, matching what the user sees.

## Gotchas

**Gotcha**: `LocalPosixVolume::resolve` has a three-way branch for absolute paths
**Why**: The frontend sometimes sends full absolute paths (like `/Users/alice/Documents`), not paths relative to the volume root. If the volume root is `/Users/alice/Dropbox`, the resolve logic must detect whether the absolute path is already inside the root (pass through), whether the root is `/` (pass through), or neither (strip leading `/` and join). Getting this wrong silently serves the wrong directory.

**Gotcha**: `MtpReadStream::Drop` spawns a detached cancel task
**Why**: When a download is cancelled mid-stream (user presses Cancel during MTP copy), the `MtpReadStream` is dropped
before the `FileDownload` is fully consumed. mtp-rs's `ReceiveStream` panics on drop if not consumed or cancelled
(to prevent USB session corruption). The `Drop` impl calls `download.cancel(DEFAULT_CANCEL_TIMEOUT).await` on a
spawned detached task. This is safe because the stream always lives in an async context (tokio worker thread), so
`Handle::try_current()` succeeds. The detached task runs independently — the drop returns immediately.

**Gotcha**: `MtpVolume::get_metadata` is expensive — it lists the entire parent directory
**Why**: MTP has no single-file stat call — `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

**Decision**: `notify_mutation` lives on the Volume trait, not in Tauri commands
**Why**: Every mutation method (`create_file`, `create_directory`, `delete`, `rename`) knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation. The alternative (notification calls in every Tauri command) is fragile — easy to miss a call site.

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `notify_directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`path_to_id(mount_path)` for SMB, `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`
**Why**: `SmbVolume` now handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. The old `std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) is redundant and goes through the slow OS mount. Returning `false` skips it.

**Decision**: `SmbVolume` uses `tokio::sync::Mutex<Option<(SmbClient, Tree)>>`, not `RwLock` or `std::sync::Mutex`
**Why**: Every `SmbClient` method takes `&mut self` — there is no read-only access path. An `RwLock` where you only ever take write locks is strictly worse than a `Mutex` (higher overhead). `tokio::sync::Mutex` is used instead of `std::sync::Mutex` because the lock is held across `.await` points in async Volume methods. The `Option` allows graceful cleanup on disconnect (set to `None`).

**Decision**: `SmbVolume::local_path()` returns `None`
**Why**: `local_path()` is checked in `volume_copy.rs` to decide whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount — exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: `on_unmount()` trait method instead of `Any` downcasting
**Why**: Avoids runtime type checking, extensible for future volume types (S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

**Decision**: SmbVolume background watcher uses a dedicated smb2 connection, not the main one
**Why**: `smb2::Watcher<'a>` borrows `&'a mut Connection` for its lifetime (long-poll blocks until server reports changes). Using the main client would block all file operations. The watcher task owns its own `SmbClient` + `Tree`, and stats new/modified files through the main client via `VolumeManager::get(volume_id)`.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is
**Why**: `Watcher<'a>` borrows `&'a mut Connection`. Storing both the client and watcher on the struct would require self-referential types. Instead, the `tokio::spawn`ed task owns the client, creates the watcher, and runs the loop. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory
**Why**: Prevents 1000 individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The 50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes
**Why**: SMB servers send paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD
**Why**: SMB servers return NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing display paths used for cache lookups.

**Decision**: Progress callbacks use `&dyn Fn(u64, u64) -> ControlFlow<()>`, not `FnMut`
**Why**: The Volume trait is object-safe (`dyn Volume`), so callbacks must be `Fn` (not `FnMut`). Callers use `AtomicU64` for byte counters and `Cell<Instant>` for timestamps to mutate state inside a `Fn` closure. This avoids needing `RefCell` or `Mutex` in the hot path.

**Gotcha**: On macOS, never use `statvfs` alone for disk space — use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks and ignores purgeable space (APFS snapshots, iCloud caches), which can be tens of GB. This causes inconsistent numbers between the status bar (NSURL API) and copy validation (`statvfs`), and prematurely blocks copies that would succeed. `get_space_info_for_path` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

## Testing

- **E2E error injection**: The `Volume` trait has an `inject_error(&self, errno: i32)` method behind the `playwright-e2e` feature flag. `LocalPosixVolume` and `InMemoryVolume` implement it — the next `list_directory` call returns the injected errno, then clears it (single-shot, so retry tests work). Default is no-op.
- `in_memory_test.rs` — unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `inmemory_test.rs` — integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `local_posix_test.rs` — real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `manager.rs` inline tests — concurrent registration/read/write-mix scenarios
- `mtp.rs` inline tests — path conversion and capability flags (no device needed)
- `smb.rs` inline tests — type mapping (DirectoryEntry→FileEntry, FsInfo→SpaceInfo, Error→VolumeError), connection state transitions, path conversion, capability flags (no server needed)
- **Docker SMB integration tests**: `smb.rs` contains `#[ignore]` tests that require Docker SMB containers (start with
  `apps/desktop/test/smb-servers/start.sh`). Connect via `smb2::testing::guest_port()` (10480, guest/no-auth),
  `auth_port()` (10481, `testuser`/`testpass`), `readonly_port()` (10488), `slow_port()` (10493, 200ms latency). Use
  these for testing real SMB protocol behavior (streaming, error paths, network edge cases). See
  `apps/desktop/test/smb-servers/README.md` for the full container list and env var overrides.
