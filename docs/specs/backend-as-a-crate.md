# A filesystem backend should be a crate, not thirty reach-throughs

**Problem**: S3, FTP(S), SFTP, WebDAV, and NFS are the top "planned" feature in `feature-status.json`, and there is no
boundary to write them behind. Verifying a change to one backend means compiling 332k lines. Meanwhile the seams that
would fix this already exist and already have a working client.

**Where it stands**: the extraction is DONE. `crates/cmdr-smb/` holds the protocol layer and the backend, and
`cargo check -p cmdr-smb --all-targets` is a complete verification loop with none of the app in it. What remains under
this heading is FTP, which is its own effort and is blocked on one product decision. The module-cycle ratchet that used
to sit under here has shipped.

**Read first**: `crates/cmdr-fs/src/volume/host/DETAILS.md`, which carries the seam set, the nine-step recipe for
writing a new backend, and the two costs this does NOT buy. Then `crates/cmdr-smb/DETAILS.md`, which is what a finished
extraction looks like.

The groundwork under it: the seam design (`crates/cmdr-fs/src/volume/host/`, nine files), the staging split
(`3f11fea44`), the app-side adapters (`fe33825a8`), the `cmdr-archive` pilot (`6d435cdf7`, and it genuinely uses the
seams), and the `SmbConnectionChanged` → `VolumeConnectionChanged` rename (`057cc9e64`) that older plans list as still
blocking.

⚠️ Two things this does not buy, both measured, both worth saying before anyone sells it internally: **`pnpm check` does
not get faster** (every Rust check shares one `rustInputs` set and runs `--workspace`), and full app builds get about
11% SLOWER after a backend edit because of the extra relink. The win is the inner loop on the backend itself, measured
at 83 to 85% for the index crate.

## The work: extract `cmdr-smb` — shipped

Kept as the record of how a backend gets extracted, because FTP and S3 will follow the same five steps.

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

4. ~~**Move the prod code down and re-home the suites.**~~ **Done, and it was one milestone for the reason this entry
   predicted**: prod code in `crates/cmdr-smb/src/volume/` with the suites left app-side gave 158 errors in
   `cargo check -p cmdr --all-targets`, because the SMB suites are WHITE-BOX tests of the backend rather than black-box
   tests through `Volume`.

   **The split rule that came out of it, and the one a new backend should copy**: a cell lives with whatever it ASSERTS,
   never with whatever it connects to. Both sides connect to the same containers. So the `Volume` contract, the byte
   path, the conformance promises, the retirement wiring, and the watcher's routing went into the crate; every cell
   driving `write_operations`, the volume registry, the listing cache, archive routing, or media enrichment stayed in
   the app. Two cells split out of crate-side suites into a new app-side `smb_app_integration_test.rs` (a pane close
   must not kill the watcher; a local file streams onto the share), and `smb_retirement_test.rs` dissolved entirely —
   its registry half was already covered generically by `manager::tests::unregistering_a_volume_retires_it`.

   **`volume::testing` is what makes that split cheap**: one fixture module under the `testing` feature, holding the
   Docker connection, the naming and cleanup helpers, and the byte-integrity hashers, shared across the seam. It hands
   out fixtures and three numbers (`negotiated_max_write`, `session_credits`, `client_lock_tickets_issued`) and never
   `.inner`, which is the line between a fixture module and the white-box surface this step existed to refuse.

   Widened beyond the `detach_session_for_test` this entry sanctioned: `SmbConnectionParams`, `SmbVolume::volume_id`,
   and `ConnectionState`, all three because the public `connect_smb_volume` takes or returns them. `SmbVolumeInner` went
   the other way and is private now.

   Three things fell out that weren't planned. Deleting `#![allow(dead_code)]` found `with_smb_sync` (no caller at all)
   and `PoolSlots::any_alive` (three assertions duplicating the ones beside them), both now gone. Deleting
   `#![allow(unused_imports)]`, which existed because `mod.rs` doubled as the suites' prelude, found a dead `PathBuf`
   import and moved the prelude to `test_support.rs` where it belongs. And `smb_test.rs` (995 lines) split into six
   files colocated with the modules they test, which is what the file-length allowlist entry should have been all along.

5. ~~**Decide the test-visibility question.**~~ **Done**, settled as above and written down in
   `crates/cmdr-smb/DETAILS.md` § "Which side a test lives on".

**What's left of this effort**: FTP (below). The module-cycle ratchet (next section) has shipped.

## Guard it: the module-cycle ratchet — shipped

`module-cycles` is live: warn-only, in the slow group, one `cargo modules` graph per first-party library crate. It
ratchets on **strongly-connected components after parent-child hubs collapse**, which was option (b) of the three this
section used to weigh. Option (a) would have fired the first time somebody split a long file into submodules, and a
ratchet that fires on a good change gets silenced.

Seeded 2026-08-22 on `cargo-modules` 0.27.0 (pinned; a box on another version skips rather than compares numbers that
don't compare): 762 modules across five crates, 16 tangles at 14 homes. The two production `use super::*` globs in
`cmdr-index`'s manager are gone with it, which took that crate's largest raw component from 19 to 15 and dropped
`watch::event_loop` out of the `lifecycle` tangle.

Everything about the metric, the version pin, why it carries a `NotInCI` reason, and the six traps that make raw
`cargo-modules` output lie: `scripts/check/checks/DETAILS.md` § "Rust module cycles". Read it before trusting any number
this tool prints.

**One finding this effort did NOT fix, and somebody should**: `write_operations::*` is an 11-module sibling tangle
(`analytics, conflict_slot, error_classification, eta, event_sinks, manager, state, status_cache, types, unique_name, validation`)
with no parent node in it, which makes it the app crate's largest genuine design tangle and the app crate's whole `max`
number on its own. It's recorded as the baseline, not repaired. It is its own effort.

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
