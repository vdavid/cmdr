# A filesystem backend should be a crate, not thirty reach-throughs

**Problem**: S3, FTP(S), SFTP, WebDAV, and NFS are the top "planned" feature in `feature-status.json`, and there is no
boundary to write them behind. `SmbVolume` reaches into the app at 30 places today, nothing stops the 31st, and
verifying a change to one backend means compiling 332k lines. Meanwhile the seams that would fix this already exist and
already have a working client.

**Size**: about a week for the SMB extraction, of which the test re-homing is the bulk. FTP afterwards is its own effort
and is blocked on one product decision.

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

In order. Only 5e is large.

1. **Split `network/`.** Half a day. Design settled, no product decisions. Protocol helpers (`build_smb_addr`,
   `is_auth_error`, `smb_types`) move down into the crate; discovery, upgrade, and UI wiring stay in the app. This also
   cuts a back-edge nobody has noticed: `network/smb_upgrade.rs:220` imports
   `crate::file_system::volume::SmbConnectionState`, and because `cargo-modules` resolves a re-export to its DEFINING
   module, that single line welds SMB and `network/` into a live nine-module cycle. Nothing in `network/` names
   `backends::smb` textually, so this looks like nothing and is load-bearing. Prerequisite for everything below.

2. **Turn the two registry reach-backs into a `Weak` handle.** Half a day, plus one small design call.
   `reconnect.rs:306` and `smb_watcher.rs:48` both call `get_volume_manager()`. The "one architecturally awkward site"
   older plans worried about dissolved when the seams landed: both are the backend asking about ITSELF, and `Weak`
   answers both with no seam at all. ⚠️ **Residual gap, genuinely open**: a volume can be removed from the registry
   without being superseded or unmounted, and nothing on the volume records that, so "am I retired?" stays unanswerable
   from inside. Needs an answer before this approach is safe.

3. **Switch the 13 real seam calls and repoint the seven re-export paths.** A day. Fully settled. The 13: keychain ×3,
   `notify_directory_changed` / `refresh_archive_listings` ×2, `try_get_authoritative_listing` ×3, `smb_concurrency`,
   `priority::foreground`, `posthog::capture`, and `index_host::index()` ×4. Every one has a live app-side implementor
   already. The seven re-export paths (`FileEntry` ×4, `ListingProgress` ×3) are mechanical repoints to `cmdr_fs::`.
   Nine more sites are protocol helpers that simply move into the crate with the code.

4. **Decide the test-only visibility.** An hour, but it is a judgment call per site. `detach_session_for_test` is
   `pub(in crate::file_system::volume)`, which has no cross-crate equivalent. Either
   `#[cfg(any(test, feature = "testing"))] pub`, which is real surface widening, or move its test into the crate. ❌
   Don't default to widening.

5. **Re-home 5,845 lines of `smb_*_test.rs`.** Several days, and the bulk of the cost. Including the Docker-gated
   integration tests, and confirming `desktop-rust-integration-tests`' name filter still selects them. The archive pilot
   showed the real work is SPLITTING tests that grew to cover both sides, not moving them. Also unverified: whether
   `smb2 = { features = ["testing"] }` forwards correctly through the extra crate hop for the `smb-e2e` feature.

## Guard it: the module-cycle ratchet

Nothing stops a subsystem re-welding, and it has already happened twice unobserved. Re-measured on 2026-08-21 with
`cargo-modules` 0.27.0: `cmdr-index`'s largest component is **19** (an older plan claims six) and `cmdr` is **11**
(claims ten). Modules in some cycle total 187, against a claimed post-work 132.

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
