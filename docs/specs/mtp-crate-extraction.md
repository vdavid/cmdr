# MTP becomes `crates/cmdr-mtp`: retrofitting the last pre-seam backend onto the host seams

**Problem.** MTP is the one storage backend that still reaches sideways into the app. Its session layer holds a
`tauri::AppHandle`, emits seven frontend events itself, writes the listing cache directly at four sites, feeds the index
handle directly, spawns on the ambient runtime, and gates real behavior behind nine inline `#[cfg(test)]`s. Nothing but
habit stops the next reach, and the backend that talks to the flakiest hardware is the one with no scoped verification
loop: every MTP change compiles the whole app. Archive's extraction surfaced seven `unwrap`s that were only legal under
`cfg(test)`; MTP is where the same class of latent defect most likely hides.

**Why now, before launch.** Data safety and honest progress are the two top product values, and MTP is the backend where
both are hardest (a dropped future wedges the phone; a lost mutation event leaves a stale pane). After the boundary, a
sideways reach is a compile error and `cargo check -p cmdr-mtp --all-targets` verifies the backend with no app in it.
That guarantee is worth more before launch than after.

**Shape.** The same layering SMB and ADB have: your `mtp-rs` under `crates/cmdr-mtp`, the app keeping only what needs
the app. The insight that sizes this correctly: a backend has two faces
(`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Architecture"). MTP's **file-ops face** (`impl Volume`,
1,333 lines in `backends/mtp/`) already is the trait and moves untouched. All the work is on its **lifecycle face**
(`src/mtp/`, 7,481 lines): replacing each direct reach with a seam, each tauri event with a typed value the app maps,
and each `cfg(test)` with the `testing` feature. Do that retrofit **in place first**, with the whole suite watching, and
the move itself becomes a `git mv`.

**Worked examples to copy.** `crates/cmdr-smb/` is the retrofit precedent (`crates/cmdr-smb/DETAILS.md` § "Which side a
test lives on"); `crates/cmdr-adb/` plus `src-tauri/src/adb/` is the device-backend layout to mirror (crate provides the
protocol and the volume, app runs the hotplug task, the provider, and the IPC commands). The seam catalog and the
ten-step "Writing a new backend" list: `crates/cmdr-fs/src/volume/host/DETAILS.md`. Read all three before M1.

## What moves, what stays

- **Crate `crates/cmdr-mtp/`** (no `tauri`, no `tauri_specta`, no `cmdr`, no English): `src/mtp/discovery.rs`,
  `src/mtp/types.rs`, all of `src/mtp/connection/` (the session layer), `src/mtp/virtual_device.rs` (behind a
  `virtual-device` feature that forwards to `mtp-rs/virtual-device`), and `backends/mtp/` as `src/volume/` (the
  `impl Volume` split by concern the way it already is: `volume_impl`, `streams`, `mapping`, `scan`, `cancel`).
- **App `src-tauri/src/mtp/`** keeps: `watcher.rs` (the hotplug task over `mtp_rs::watch_devices()`, `KNOWN_DEVICES`,
  auto-connect, the `MTP_ENABLED` gate, and the ptpcamerad calls; this is ADB's tracker twin), `macos_workaround.rs`,
  `volume_wiring.rs` (registrar + `DeviceVolumeProvider`), a new `events.rs` (the tauri event payload types and the
  adapter that maps the crate's typed events onto them), and `mod.rs` as a re-export of the crate plus the app's parked
  manager instance. `commands/mtp.rs`, `stubs/mtp.rs`, `ipc.rs`, and the frontend do not change shape.
- **No `backends/mtp.rs` shim.** Call sites import `cmdr_mtp::` directly, the way `cmdr_sftp`, `cmdr_webdav`, and
  `cmdr_adb` are used; `backends/smb.rs` and `backends/archive.rs` are retrofit-era compatibility re-exports, not the
  pattern to copy. App-side tests go beside the app subsystem they assert on (`write_operations/` for transfer cells,
  `network/`-style wiring tests next to `mtp/volume_wiring.rs`, `file_system/volume/` for the oracle cell), the way
  `sftp_transfer_integration_test.rs` and `sftp_volume_wiring_test.rs` sit today.

## Decisions

1. **The manager becomes a value; the app parks the one it built.**
   `MtpConnectionManager::new(host: VolumeHost, events: Arc<dyn MtpDeviceEvents>, registrar: MtpVolumeRegistrar)`
   replaces the crate-level `LazyLock` singleton and the registrar's `OnceLock`. The app keeps a
   `OnceLock<Arc<MtpConnectionManager>>` in `src/mtp/mod.rs` behind the existing `connection_manager()` name, filled at
   startup where `install_volume_registrar` runs today; `MtpVolume` holds an `Arc<MtpConnectionManager>`. Why: the
   host-is-a-value rule (`crates/cmdr-fs/src/volume/host/CLAUDE.md`), and today's global is why every virtual-device
   test serializes on `virtual_device_test_lock`; a test that builds its own manager with fakes needs no lock. Cost:
   about 50 non-test `connection_manager()` sites inside the crate-bound code (`session_reset.rs` 19, `volume_impl.rs`
   15, `event_loop.rs` 14) become `self` / `Arc<Self>` / a field, and about 60 test sites take the fixture's manager.
   Spawned tasks capture an `Arc<Self>` (or `Weak` where the task must not keep a retired manager alive).
2. **Device-lifecycle events are a crate-local trait, not a `cmdr-fs` seam.** `cmdr_mtp::MtpDeviceEvents` with one
   method taking a typed `MtpDeviceEvent` enum: `Connected { device_id, device_name, storages }`,
   `Disconnected { device_id, reason }`, `StorageRemoved { device_id, storage_id }`,
   `ExclusiveAccess { device_id, blocking_process }`, `PermissionDenied { device_id }`. The two ptpcamerad events are
   emitted by `watcher.rs`, which stays app-side, so they never cross. Why not a host seam: the host seams are the
   questions every backend asks; these are MTP-shaped, and ADB's equivalent lives in its app-side tracker. The five
   `tauri_specta::Event` payload structs move to `src/mtp/events.rs` with their derives, keeping exact names and
   `camelCase` fields so the wire contract is unchanged. `NoMtpDeviceEvents` is the detached default; a
   `RecordingMtpDeviceEvents` under `testing` lets a test assert the sequence a user would have seen.
3. **Listing reaches become `ListingHost` calls, one per changed directory.** `try_get_authoritative_listing` →
   `host.listings().authoritative_listing`. The event loop's blanket and targeted refreshes
   (`get_listings_by_volume_prefix`, `compute_diff`, `update_listing_entries`, `notify_directory_changed`) need two
   additions to the seam: a `DirectoryChange::Replaced(Vec<FileEntry>)` variant (the backend re-listed a directory; the
   host diffs and patches, which is where `compute_diff` goes) and a
   `ListingHost::volumes_with_open_listings(volume_id_prefix) -> Vec<String>` query, so the targeted refresh keeps
   resolving handles only against storages a pane is showing. ❌ Never one seam call per entry (the dispatch rule); the
   `host_seam_test.rs` this crate must carry asserts `change_count` stays put across a full listing walk.
4. **Index reaches become `IndexNotifier` methods.** `index_host::index().on_device_object_changed / _removed` become
   `IndexNotifier::device_object_changed(device_id, handle)` and `device_object_removed(device_id, handle)`, defaulting
   to no-ops; the app's `index_host::VolumeIndexNotifier` forwards to the handle. `cmdr-index` already carries the MTP
   transport (`crates/cmdr-index/src/indexing/transports/mtp/`); nothing there changes.
5. **Spawning goes through `host.runtime()`.** Nine `tokio::spawn` sites in `event_loop.rs`. The watcher OS thread and
   the synchronous attach path have no reactor, which is exactly the panic the runtime seam exists for.
6. **`UsbSpeed` moves to `cmdr-fs`.** It is the cross-platform mirror of `mtp_rs::UsbSpeed` that `LocationInfo` and
   `DeviceVolumeEntry` carry for both device backends; vocabulary belongs in the vocabulary crate. The app's
   `usb_speed.rs` becomes a re-export.
7. **`cfg(test)` gates on behavior become `any(test, feature = "testing")`, and the two test hooks widen under that
   gate.** Sites: `connection/mod.rs` (`storage_lookups` field and its init), `connection/file_ops.rs` (the two counter
   reads), `backends/mtp/streams.rs` (the `test_window` branch and module), `backends/mtp/mod.rs` (`test_hooks`),
   `volume_impl.rs:56` (the bump). `test_hooks` and `test_window` become
   `cmdr_mtp::volume::testing::{list_directory_call_count, reset_list_directory_call_count, set_read_window}`, `pub`
   under the gate: the app's `mtp_scan_oracle_tests.rs` asserts on the APP's fresh-listing oracle and stays app-side, so
   it needs the counter across the boundary. This is the same argued exception SMB granted `detach_session_for_test`
   (`crates/cmdr-fs/src/volume/host/DETAILS.md` § "Visibility that has no cross-crate equivalent").
   `virtual_device.rs`'s five gates become the `testing` feature too; the E2E-facing functions
   (`activate_from_env_if_requested`, `setup_virtual_mtp_device_at`, `rescan_virtual_device`, pause / resume) stay under
   `virtual-device`.
8. **Platform gating stays exactly where it is.** The app declares `cmdr-mtp` under its existing
   `[target.'cfg(any(target_os = "macos", target_os = "linux"))'.dependencies]` table, `mtp-rs` moves there from the
   app, and the `stubs::mtp` commands keep serving other targets. The crate itself compiles wherever `mtp-rs` does.
9. **Connect ordering is preserved and pinned.** `connect()` attaches storages through the registrar synchronously,
   BEFORE the event loop starts, and `mtp_test.rs`'s registrar cell keeps pinning that. Decision 1 adds an indirection
   here; verify against the virtual device in the app suite and a real phone at the end. Not settleable statically.
10. **The two "permanently app-resident" claims are revised, not deleted.** `backends/DETAILS.md` § "Per-backend
    decisions" and `crates/cmdr-fs/src/volume/host/DETAILS.md` § "Which backends move" record the new decision and why
    each of the three original reasons is answered (events: decision 2; `cfg(test)`: decision 7; visibility: decision 7;
    "veneer over `src/mtp/`": both move together). `local_posix` stays permanent; nothing here touches it.

## Milestones

Each milestone is one or more commits, green on the checks it names, in a worktree from
`~/.claude/scripts/new-worktree.sh mtp-crate`. Run `pnpm check --fast` while iterating and plain `pnpm check` per
milestone. M1 and M2 happen IN PLACE under the current paths: every step compiles against the app and the existing
suites catch a regression before any path churns. Do not start M3 until M2 is green.

### M0: record the decision

- Revise the two decision paragraphs (decision 10). Add this spec to `docs/specs/index.md` if it isn't there.
- Green: `pnpm check docs-dead-links docs-link-text docs-reachable oxfmt`.

### M1: de-tauri the session layer, in place

1. Add `IndexNotifier::device_object_changed / device_object_removed` (decision 4), `DirectoryChange::Replaced` and
   `ListingHost::volumes_with_open_listings` (decision 3) to `cmdr-fs`, with recording-fake support, and wire the app
   adapters in `index_host.rs` and `file_system/listing/listing_host.rs`. TDD: a `RecordingListings` cell first.
2. Add `MtpDeviceEvents` + `MtpDeviceEvent` in `src/mtp/connection/events.rs`; move the five tauri payload structs to
   `src/mtp/events.rs` with an adapter `TauriMtpDeviceEvents` that maps each variant to the existing event. Remove every
   `AppHandle` parameter from `connect`, `handle_storage_added`, `handle_storage_removed`, `start_event_loop`,
   `handle_device_disconnected`, `handle_device_session_reset`, `reopen_after_session_reset`.
3. Decision 1: the manager takes `VolumeHost`, events, and registrar at construction; the app parks it; the crate-side
   `OnceLock` registrar goes away. Replace the `connection_manager()` sites listed above.
4. Decisions 3, 4, 5: listing, index, and spawn reaches through the host.
5. Green: `pnpm check` plus the virtual-device suites (the `virtual-mtp` feature build; how to run them:
   `docs/tooling/virtual-mtp.md`). **`pnpm bindings:regen` must produce zero diff in `src/lib/ipc/bindings.ts`**: that
   is the proof the frontend contract didn't move.

### M2: test gates and vocabulary, in place

1. Decision 7: the `cfg(test)` conversions and the widened test hooks. Decision 6: `UsbSpeed` to `cmdr-fs`.
2. `crate::ignore_poison` → `cmdr_fs::ignore_poison`; `crate::file_system::listing::FileEntry` → `cmdr_fs::FileEntry`.
3. Replace every `use super::*` prelude in the 17 files that carry one with explicit imports (the SMB answer:
   `crates/cmdr-fs/src/volume/host/DETAILS.md` § "Test modules reached through `use super::*`"). This is what makes the
   M3 split sizeable; a glob hides what a test really reaches.
4. Check every ``[`Type::method`]`` rustdoc link in the moving files for an app-side target and make it prose.
5. Green: `pnpm check`, same suites as M1, bindings zero-diff again.

### M3: the move

1. `crates/cmdr-mtp/Cargo.toml` modeled on `crates/cmdr-adb/Cargo.toml`: `cmdr-fs` path dep, `mtp-rs = "0.32.0"` (the
   version the app pins today; ❌ don't bump as a side effect), `specta = "=2.0.0-rc.24"`, `tokio` with `rt`, `sync`,
   `time`, `macros` and NOT `rt-multi-thread`, `tokio-util` default-features off, `log`, `bytes` if the session layer
   uses it. Features: `testing = ["cmdr-fs/testing"]`, `virtual-device = ["mtp-rs/virtual-device"]`. Self dev-dependency
   with `testing`. `lints.workspace = true`, `#![warn(unused_crate_dependencies)]`, `#![deny(missing_docs)]` (every
   `pub` item gets a doc; SMB paid this too).
2. Workspace: add the member to the root `Cargo.toml` alphabetically. App:
   `cmdr-mtp = { path = "../../../crates/cmdr-mtp" }` under the macOS/Linux target table,
   `virtual-mtp = ["cmdr-mtp/virtual-device"]`, and the app's dev-dependency on `cmdr-mtp` with `testing` (the shape
   `cmdr-adb` uses). `cargo deny check` (nothing new enters the graph; assert it).
3. `git mv` the files (decision "What moves"), fix paths, `src/mtp/mod.rs` re-exports the crate under the original
   names; `backends/mod.rs` drops its `mtp` module and re-exports `MtpVolume` from the crate.
4. `index-crate-isolation`: add `cmdr-mtp` to `guardedIndexCrates`, and a `surfaceGuardedCrates` entry with ceilings set
   to the counts the audit lands on, justified in `crates/cmdr-mtp/DETAILS.md` § "The public surface is capped". The two
   MTP-specific checks (`desktop-rust-mtp-dropping-timeout`, `desktop-rust-mtp-no-transport-reset`) scan `src/mtp/`
   only: extend both to also walk `crates/cmdr-mtp/src/`, with a test each, so the guardrails move with the code they
   guard.
5. Green: `cargo check -p cmdr-mtp --all-targets` with no app in it, then `pnpm check`, then
   `pnpm check --include-slow`.

### M4: the test split

Split by what a cell ASSERTS, never by what it connects to (`crates/cmdr-smb/DETAILS.md` § "Which side a test lives
on"). Starting allocation, to be corrected cell by cell while reading:

- **Crate**: `mtp_conformance_test.rs` (as `volume/conformance_test.rs`), `mtp_delete_test.rs`,
  `mtp_read_range_test.rs`, `mtp_read_bench.rs`, `connection/path_cache_sync_test.rs`, and the `Volume`-contract cells
  of `mtp_test.rs`. Add `volume/host_seam_test.rs` (seed a virtual device, walk it every way the backend can, assert
  `change_count` and the recorded device-event sequence), copying `crates/cmdr-sftp/src/volume/host_seam_test.rs`.
- **App** (beside the subsystem each one asserts on, never under `backends/`): `mtp_archive_test.rs` (archive routing is
  the app's), `mtp_scan_oracle_tests.rs` (the app's oracle), the registrar-ordering and wiring cells of `mtp_test.rs`,
  `write_operations/.../rename_merge_mtp_tests.rs` and `delete/volume_cancel_tests.rs` (they drive the app's pipeline).
  App cells reach the fixture through `cmdr_mtp::testing` and a thin `mtp_test_support.rs` that passes the app's real
  `VolumeHost`, the way `smb_test_support.rs` does.
- **E2E** (`test/e2e-playwright/mtp-*.spec.ts`): unchanged. The `virtual-mtp` feature forwards, and the wire contract is
  pinned by the bindings zero-diff. Run the MTP shard once at the end of M4.

### M5: docs, and the real device

1. `crates/cmdr-mtp/CLAUDE.md` + `DETAILS.md`: the must-knows move from `src/mtp/CLAUDE.md` and
   `src/mtp/connection/CLAUDE.md` + `DETAILS.md` with the code; what stays app-side stays documented app-side.
   `backends/CLAUDE.md` (module map, "MTP is a crate now" beside the SMB paragraph), `backends/DETAILS.md`,
   `file_system/volume/DETAILS.md` § "Architecture" (move `Mtp` from the app row to the crate row),
   `docs/architecture.md` (backend map and the workspace-crates list), `docs/tooling/virtual-mtp.md`,
   `scripts/check/checks/DETAILS.md` (the two checks' new scope), `crates/cmdr-fs/src/volume/host/DETAILS.md` (§ "Which
   backends move", § "Visibility": MTP no longer wears it).
2. Allowlists: the two `file-length` entries (`connection/directory_ops.rs` 983, `connection/mod.rs` 1204) carry over
   under their new paths at their current numbers; that's a rename, not growth, and this spec is the consent. Any OTHER
   new entry or raise is a finding to surface, not silence. New `CLAUDE.md`s stay under 600 words so `claude-md-length`
   needs no entry.
3. Real-device QA (David): connect, list, copy both ways, delete a folder with children (must refuse), unplug mid-copy,
   session reset (a phone screen lock does it), replug to a different port (index re-matches by serial), toggle the
   setting off and on. Watch the log for the registration-before-event-loop order.
4. FF-merge to local `main`, delete the worktree and branch.

## Verification gates, in one place

- `pnpm bindings:regen` → zero diff, after M1, M2, and M3.
- `cargo check -p cmdr-mtp --all-targets` compiles with no app crate in the graph (M3 onward).
- `pnpm check` per milestone; `pnpm check --include-slow` after M3 and M4.
- The E2E MTP shard after M4.
- Real device at M5.

## Risks and the answer to each

- **The manager refactor (decision 1) is the largest single change.** Do it as its own commit inside M1, after the
  event-trait commit, so a regression bisects to it. If a spawned task needs the manager after retirement, that's a
  `Weak` and a log line, never a leaked `Arc`.
- **`use super::*` hides the split size.** M2 step 3 removes the unknown before M3 needs the number.
- **`missing_docs` over roughly 9,000 lines.** Mechanical but real; budget it. A doc that only restates the signature is
  worse than none: say what the caller must know.
- **A latent `cfg(test)`-only `unwrap` or dead import surfaces as a clippy finding at M3.** That is the point. Fix it,
  don't allow it.
- **Windows and the stubs.** The crate is never compiled into a non-macOS/Linux app build; `stubs::mtp` keeps that
  target green. Assert with a `cargo check --target x86_64-pc-windows-msvc` if the toolchain is installed; otherwise
  CI's matrix is the gate.

## Cost to finish

Roughly two to three days of agent work: M1 is half of it (the manager and the event trait), M3 and M4 a day together,
M0, M2, and M5 the rest. Real-device QA at the end is David's half hour.
