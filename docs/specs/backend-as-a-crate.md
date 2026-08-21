# A filesystem backend should be a crate, not thirty reach-throughs

**Problem**: S3, FTP(S), SFTP, WebDAV, and NFS are the top "planned" feature in `feature-status.json`, and there is no
boundary to write them behind. Verifying a change to one backend means compiling 332k lines. Meanwhile the seams that
would fix this already exist and already have a working client.

**Where it stands**: `SmbVolume` no longer reaches into the app at all (step 3, done) — every question it asks goes
through its `VolumeHost`. What's left is moving the code down, and that is one milestone with the test re-homing, not
three (step 4 below has the measurement).

**Size**: several days for what remains, essentially all of it splitting tests that grew to cover both sides of the
boundary. FTP afterwards is its own effort and is blocked on one product decision.

**Read first**: `crates/cmdr-fs/src/volume/host/DETAILS.md`, which carries the seam set, the nine-step recipe for
writing a new backend, and the two costs this does NOT buy. Then
`apps/desktop/src-tauri/src/file_system/volume/backends/DETAILS.md` § "Per-backend decisions".

Already shipped, and the reason this is a finish rather than a start: the seam design
(`crates/cmdr-fs/src/volume/host/`, nine files), the staging split (`3f11fea44`), the app-side adapters (`fe33825a8`),
the `cmdr-archive` pilot (`6d435cdf7`, and it genuinely uses the seams), and the `SmbConnectionChanged` →
`VolumeConnectionChanged` rename (`057cc9e64`) that older plans list as still blocking.

⚠️ Two things this does not buy, both measured, both worth saying before anyone sells it internally: **`pnpm check` does
not get faster** (every Rust check shares one `rustInputs` set and runs `--workspace`), and full app builds get about
11% SLOWER after a backend edit because of the extra relink. The win is the inner loop on the backend itself, measured
at 83 to 85% for the index crate.

## The work: extract `cmdr-smb`

In order. Steps 1 to 3 have shipped; step 4 is everything that's left, and it's the large one.

1. ~~**Split `network/`.**~~ **Done.** `crates/cmdr-smb/` holds the protocol helpers (`build_smb_addr`, the `classify_*`
   / `is_auth_error` pair, the share-listing vocabulary, and the two `smb2` listing calls); discovery, upgrade, mounts,
   the keychain, and the UI wiring stayed in the app. The nine-module `network` ↔ SMB cycle is cut, but NOT at the line
   this plan predicted: `smb_upgrade.rs:220` draws no edge at all (the type it imports lives in `cmdr-fs`, which
   `--no-externs` drops). The real weld was an `impl From<ConnectionState> for network::VolumeConnection` in
   `smb/state.rs`, misread as a `network →` edge because `cargo-modules` attributes an impl to the module defining the
   type it PRODUCES. The backend now converts into the seam's `VolumeConnection` and emits through
   `events::volume_mapping`. Measured on cargo-modules 0.26.0: the nine-module component became six (all
   `backends::smb::*` parent ↔ child), and `network` is in no cycle but its own pair with `mdns_discovery`.

2. ~~**Turn the two registry reach-backs into a `Weak` handle.**~~ **Done.** Both are the backend asking about ITSELF,
   and a `SelfHandle<SmbVolumeInner>` (a `Weak` plus the registry's `Retirement` flag) answers both with no seam at all.
   The residual gap is closed the way it had to be: the registry writes "you left" into a flag the volume publishes
   through `Volume::retirement`, at the two ways out of the registry and deliberately NOT on a replace (a re-root hands
   the id to a share that is still live). Shared-lifecycle work, so every backend inherits it. Two things fell out that
   were not planned: `SmbVolume::instance_id` and both `downcast_ref` calls are gone (identity is a pointer now), and
   the reconnect state machine moved from `SmbVolume` onto `SmbVolumeInner`, where it already only ever reached — which
   makes the `use super::*` question smaller, since `state.rs` and `reconnect.rs` no longer touch the instance.

3. ~~**Switch the seam calls and repoint the re-export paths.**~~ **Done.** `SmbVolume` takes a `VolumeHost` in
   `connect_smb_volume` and keeps it on the share-scoped `SmbVolumeInner`; nothing in the backend names an app symbol
   any more. The real counts, against the 13 this plan predicted: **24 call sites**, because "13" counted seam METHODS
   where the code has arms. Listings 14 (three in `notify_mutation`, ten in the watcher, one archive refresh),
   credentials 3, indexing 4, events 2, settings 1, activity 1, analytics 1, plus **four `tokio::spawn` sites** (the
   watcher, the watcher-death reconnect, the scan pool's member reconnects, the streaming-read producer) that the plan
   didn't count at all. The seven re-export paths were seven exactly.

   Three things fell out that weren't planned:

   - **`smb/events.rs` is gone.** Its `AppHandle` static was only ever needed for two things, and both moved: state
     transitions became `host.events()`, and `emit_fell_back_to_os_mount` went to `network/os_mount_notice.rs`, which
     already owned the once-per-server decision.
   - **`volume_host::host()` no longer falls back to `VolumeHost::detached()`.** Only the frontend event channel (and
     the app's runtime) needs a running Tauri app; the listing cache, secret store, index handle, priority tracker, and
     settings module are all process-global. A detached fallback would have left every app-side backend test asserting
     against a cache nothing writes to.
   - The `#[cfg(any(target_os = ...))]` guards around the backend's index calls are gone: `IndexNotifier` compiles
     everywhere and the app's adapter gates its own MTP arm.

4. **Steps 4 and 5 are ONE milestone with the prod-code move, and the move must not land first.** Measured 2026-08-21 by
   doing the move and compiling: prod code in `crates/cmdr-smb/src/volume/` with the suites left app-side gives **158
   errors in `cargo check -p cmdr --all-targets`**, and none of them are fixable by anything short of the widening step
   4 exists to refuse.

   The SMB suites are WHITE-BOX tests of the backend, not black-box tests through `Volume`. What they reach, per file
   (`inner:N` counts `.inner` field accesses):

   - `smb_test.rs` (995 lines, `inner:23`) — `to_smb_path`, `to_display_path`, `map_smb_error`,
     `fits_one_compound_write`, `SmbReadStream`, `SMB_STREAM_CHANNEL_CAPACITY`, `filetime_to_unix_secs`,
     `directory_entry_to_file_entry`, `fs_info_to_space_info`, `ConnectionState`, `open_scan_pool` / `close_scan_pool`.
     Its ONLY app dependency is `priority::foreground::note_foreground_activity_on`.
   - `smb_test_support.rs` (221, `inner:1`) — builds an `SmbVolumeInner` by struct literal, so every field of it.
   - `smb_integration_test.rs` (797, `inner:16`), `smb_media_fetch_integration_test.rs` (164, `inner:3`),
     `smb_retirement_test.rs` (93, `inner:2`), `smb_soak_test.rs` (413, `inner:1`), `smb_transfer_semantics_test.rs`
     (733, `inner:1`) — between them they touch `client`, `tree`, `scan_pool`, `watcher_cancel`, `unmounted`, `params`,
     `self_handle`, `do_attempt_reconnect`, `transition_to_disconnected`.
   - `smb_archive_integration_test.rs`, `smb_full_concurrency_test.rs`, `smb_streaming_integration_test.rs`,
     `smb_stress_test.rs` — `list_directory_impl`, `negotiated_max_write`, `InlineReadStream`, `CLIENT_LOCK_TICKET`.
   - `smb_conformance_test.rs` and `smb_transfer_safety_test.rs` reach nothing private.

   **`.inner.client` / `.inner.tree` / `.inner.scan_pool` are the session's guts**, so there is no narrow
   `testing`-gated white-box surface to build: a surface that satisfies these suites is the whole struct.

   **Do it the way the archive pilot did**: move prod and tests in one milestone. The archive pilot left exactly ONE
   app-side test (`archive_watch_integration_test.rs`, the half that asserts on the app's listing cache) and moved the
   rest. The same split applies here — `smb_test.rs` and `smb_test_support.rs` are pure backend and belong in the crate;
   the suites that drive `write_operations`, `operation_log`, or `volume::manager` are the app half and need SPLITTING
   rather than moving.

   **`smb_watcher/archive_refresh_test.rs` (178 lines) is the model split, and the cheapest one to do first.** Its
   backend half — a `Modified` event on a supported-archive name reaches `refresh_archive_listings`, a non-archive name
   doesn't — becomes a `RecordingListings` assertion in the crate with no app in it. Its app half, what a refresh DOES
   to the listing cache, already exists as
   `listing/listing_host.rs::the_archive_refresh_re_reads_the_listings_under_its_path`, whose doc comment points at this
   file and needs repointing when it goes.

   **The prod move itself is mechanical, about half an hour**, and it compiles clean once done
   (`cargo check -p cmdr-smb --all-targets` was green; only the app-side test target failed). The recipe, verified:

   - `git mv` `backends/smb/*.rs` to `crates/cmdr-smb/src/volume/`, and `backends/smb_watcher.rs` to
     `crates/cmdr-smb/src/volume/watcher.rs`. Being a child of `volume/` is what turns
     `pub(in …::backends) spawn_watcher_death_reconnect` into a plain `pub(super)`.
   - `Cargo.toml` gains `cmdr-fs`, `tokio` (`macros`, `rt`, `sync`, `time`, and deliberately not `rt-multi-thread`),
     `tokio-util` (default-features off, matching `cmdr-fs`), `futures-util`, `unicode-normalization`, plus
     dev-dependencies on `cmdr-fs/testing` and `tokio/rt-multi-thread`.
   - In `volume/mod.rs`: the `use super::{…}` vocabulary block becomes `use cmdr_fs::volume::{…}`, `cmdr_smb::`
     self-references become `crate::`, `super::super::smb_watcher::` becomes `super::watcher::`, and
     `SmbConnectionParams` plus `SmbVolume::volume_id` become `pub` with doc comments (`#![deny(missing_docs)]`).
   - App side: `backends/smb.rs` replaces `backends/smb/`, holding `pub use cmdr_smb::volume::*;` — the shape
     `backends/archive.rs` already has. `#[path]` on a module declared in `backends/smb.rs` resolves relative to
     `backends/`, so the suites' paths lose their `../` prefix.
   - `SmbConnectionState` drops out of `backends/mod.rs`'s re-export block (only the backend used it).
   - `detach_session_for_test` becomes `#[cfg(any(test, feature = "testing"))] pub`, and the app's `testing` feature
     forwards `cmdr-smb/testing`. That forward also turns `smb2/testing` on for every app DEV target, which is harmless
     (`smb-e2e` already does it) but worth knowing.
   - Keep `#![allow(dead_code)]` on the backend only while the suites are away: `volume_id`, `PoolSlots::any_alive`, and
     `with_smb_sync` have no non-test caller. Delete the allow when they land, or a genuinely dead item in this backend
     stops being a finding.

## Guard it: the module-cycle ratchet

Nothing stops a subsystem re-welding, and it has already happened twice unobserved. Re-measured on 2026-08-21 with
`cargo-modules` 0.27.0: `cmdr-index`'s largest component is **19** (an older plan claims six) and `cmdr` is **11**
(claims ten). Modules in some cycle total 187, against a claimed post-work 132.

⚠️ **The tool version moves the absolute numbers**, so measure before AND after a cut on whatever version is installed
rather than diffing against a figure written down here. Step 1 was measured on 0.26.0, which reports `cmdr` at 128 of
528 modules in a cycle where 0.27.0 reports 126 of 522.

⚠️ **The obvious check has a bad failure mode, and this needs deciding before it is built.** Most of that regrowth is
not coupling: `lifecycle/state.rs` became `lifecycle/state/` with eight children, which improved the code and would have
tripped a max-SCC ratchet. Three options:

- **(a)** Build it as originally specced and accept that it fires on file splits, re-baselining by hand each time.
- **(b)** Ratchet on max SCC **after collapsing parent-child hubs**, which measures cross-subsystem welding and ignores
  subdivision. More code, better signal.
- **(c)** Drop the check, keep only the traps documentation, and treat cycle measurement as an on-demand tool.

**Recommendation: (b).** It is the metric the analysis actually reasons with, and (a) will get silenced the first time
it fires on a good change. About a day either way.

❗ Before trusting any `cargo-modules` number, read `scripts/check/checks/DETAILS.md` § "Rust module cycles". There are
five traps that make raw output wrong, including that a re-export resolves to its defining module and that splitting a
file into submodules grows max SCC with zero new coupling.

Two smaller findings from the same measurement, neither owned by this effort: `write_operations::*` now has an 11-module
sibling tangle
(`analytics, conflict_slot, error_classification, eta, event_sinks, manager, state, status_cache, types, unique_name, validation`)
with no parent node in it, making it the app crate's largest genuine design tangle; and two production `use super::*`
globs at `lifecycle/manager/start.rs:9` and `manager/phased.rs:22` inflate `cmdr-index`'s component for free, about 30
minutes to de-glob.

## Then: FTP as the proof

The milestone the whole effort exists for, and the one that shows the seams survive a backend that is not SMB. **Blocked
on one product decision**: FTP's concurrency knob. `AppBackendSettings` resolves through a namespace-keyed table with
exactly one `"smb"` row (`file_system/backend_settings.rs:37`), and a namespace with no row gets a conservative built-in
of two. Whoever ships FTP decides global versus per-server, the default (likely one), and whether it is exposed at all.

Everything else is settled: the seams survived contact with the real app with no signature change.

## Adjacent, and worth its own effort

**Per-crate check lanes with narrowed `Inputs`** are the only thing that makes `pnpm check` faster, and they are
independent of any extraction. `inputs.go:46` defines one `rustInputs` and `cargo-workspace.go:254` returns
`--workspace` for every cargo lane. A day or two, and explicitly not part of this plan.
