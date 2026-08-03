# Per-filesystem backend crates

Status: planned, 2026-08-03. Not started. Every number below is measured against `main` at `905935df5` by a read-only
survey, not estimated. Where a claim is extrapolated rather than measured, it says so.

FTP(S), S3, and SFTP are on the near roadmap. This plan makes "a filesystem backend is its own crate" the shape they're
written in, and validates that shape against one mature backend first.

## Why, and the one place the motivation doesn't hold

Two motivations, both David's, in priority order.

1. **A clear, simple API between the app and a backend**, so an agent working on one doesn't carry the other. This is
   the dominant reason, and it is genuinely delivered. See below.
2. **Faster builds and check runs when a backend is unchanged.** Partly delivered, and one part of it is wishful. See
   below.

### Reason 1 holds, and the mechanism is enforcement, not design

The `Volume` trait is **already** the API and it is **already** below the app, at `crates/cmdr-fs/src/volume/mod.rs`. So
are all 14 types it exchanges. Nothing about the API needs inventing.

What a crate boundary adds is that the API becomes the _only_ way through. Today `SmbVolume` reaches into
`listing::caching`, `network::keychain`, `index_host`, `analytics::posthog`, `priority::foreground`,
`file_system::smb_concurrency`, and `get_volume_manager()` across 23 sites, and nothing stops the 24th. After the
boundary, each is either a named seam or a compile error.

The enforcement already exists and generalizes: `scripts/check/checks/index-crate-isolation.go` asserts no `tauri` /
`tauri-specta` / `cmdr` in a guarded crate's `cargo metadata` tree and caps its public surface. Adding a backend crate
to `guardedIndexCrates` is a one-line change plus a ceiling.

The concrete agent-workflow win: `cargo check -p cmdr-archive` becomes a complete verification loop for an agent that
never opens the app crate.

### Reason 2: what will and won't happen

**`pnpm check` will NOT get faster, and no amount of extraction changes that.** `scripts/check/checks/inputs.go` defines
`rustInputs` as `apps/desktop/src-tauri/**` + `crates/**` + the manifests, shared by every Rust check. Any Rust edit
anywhere busts every Rust check's cache. `desktop-rust-clippy` and `desktop-rust-tests` also run `--workspace`, so the
work doesn't shrink either. **Getting a check-runner win requires new per-crate check lanes with narrowed `Inputs`.**
That's separate, deliberate work, not a side effect of this plan. It is not scheduled here.

**Full app builds get slightly slower after a backend edit.** `docs/notes/index-extraction-baseline.md` measured +11%
for "index edit, then `cargo build`", because the app still relinks. Expect the same sign.

**The real win is the scoped inner loop**, and it transfers regardless of crate size, because it comes from _not
compiling the 245k-line app_: the index extraction measured `cargo check --lib` −83% (4.35 s → 0.75 s) and
`cargo test --lib --no-run` −85% (23–30 s → 3.55 s).

**Release builds may get modestly faster**: the index extraction took a clean release build 214 s → 188 s (−12%),
because more crates give cargo more codegen units. Don't expect −12% again; the index was 28% of the tree, archive plus
SMB are about 7%.

### The finding that shapes the whole plan

**The payoff is overwhelmingly for the backends not yet written.** A `cmdr-ftp` written as a crate from day one costs
almost nothing extra and gets the full benefit. Retrofitting is where all the cost sits, and two of the four existing
backends don't clear the bar (below). So: **extract exactly one existing backend to validate the seam set against
something mature and real, then write every new backend as a crate.** Retrofitting the rest is judged separately, per
backend, after FTP ships.

## The host seam set

Every app-crate reach-through across all four backends clusters into seven concerns. This list is the design input; it's
complete as of the survey.

- **Listing cache** — `notify_directory_changed`, `try_get_watched_listing`, `refresh_archive_listings`,
  `find_listings_for_path_on_volume`, `patch_listing_after_local_mutation`. Needed by all four. The highest-leverage
  seam: a 4-method trait whose exchanged types (`DirectoryChange`, `MutationEvent`, `FileEntry`) are already in
  `cmdr-fs`. `notify_directory_changed` already does index sync, cloud-badge invalidation, and pane diff behind one
  call, so one seam method covers three app concerns.
- **Runtime handle** — backends use `tauri::async_runtime::spawn` because watcher OS threads have no reactor (that
  constraint is real; see `file_system/CLAUDE.md`). `cmdr-index` already solved this: the host injects a
  `tokio::runtime::Handle`.
- **Typed event emit** — `EventSink`-shaped, mirroring `write_operations/event_sinks.rs`'s `OperationEventSink`. The
  payload types keep their `tauri_specta::Event` derives **app-side**; the backend calls `sink.connection_changed(…)`.
- **Credentials** — a 2-method trait over `network::keychain`. The store underneath is already pluggable.
- **Index notification** — `on_watch_gap`, `resume_after_reconnect`. Note `smb_watcher.rs` already imports
  `cmdr_index::{WatchGap, WatchScope}` directly, so half of this is already a crate-to-crate edge.
- **Settings accessor** — `file_system::smb_concurrency()` today; per-backend concurrency generalizes.
- **Priority and analytics** — `priority::foreground`, `analytics::posthog::capture`.

Registration is **not** a seam: it's already solved by the "backends never register themselves" rule
(`mtp/volume_wiring.rs`, `network/smb_upgrade.rs`). Cancellation (`CancellationToken`), progress (`ListingProgress`),
and error mapping (`friendly_error/`) are already in `cmdr-fs`.

**Model to copy**: `crates/cmdr-index/src/indexing/host/` (1,124 lines), with
`apps/desktop/src-tauri/src/priority/host_policy.rs` as the app-side adapter shape.

## Design decisions

### 1. Design the seams against SMB, implement them first on archive

Archive's entire coupling is three seams. SMB's is 23 sites across all seven. If the seam set is designed against
archive, SMB will have to invent the rest and may have to redesign what archive built. So: design once from SMB's needs
and the FTP/S3/SFTP requirements, then land archive as the first _implementation_, because it's the cheapest way to get
the crate scaffolding, the isolation check, the doc pair, and the runtime injection right without touching a network
path.

### 2. A seam trait object may be called per mutation, never per entry

Thin LTO at the workspace root is what kept the index extraction's hot paths within ±2%, and `Volume` is already
`dyn`-dispatched so no _new_ dynamic dispatch appears at the volume level. But a `ListingHost` called once per mutation
is free and one called per directory entry is not. This rule goes in the seam docs, the way
`crates/cmdr-index/src/indexing/host/DETAILS.md` states its equivalents.

### 3. `local_posix` is permanently app-resident, in writing

`local_posix.rs` is 848 non-test lines and is the **hardest** extraction, not the easiest. It reaches
`crate::file_system::git` at ten sites, and `file_system/git/` is 6,402 lines including a `gix`-backed repo walker and a
`.git` watcher — the git portal is _implemented as_ `LocalPosixVolume` hooks. It's also the only caller of the real-FS
reader in `listing/reading.rs`, which serves the non-volume listing path too, and it's the FSEvents watcher's peer.

Extracting it means extracting git or inventing a git seam with exactly one implementor. Someone will eventually propose
"completing the set", so the docs must pre-refuse it, the way `file_system/volume/DETAILS.md` pre-refuses re-adding a
`scanner()` hook.

### 4. MTP is out of scope and is its own project

Not a judgment about value, just size. Seven event payload types carry `specta::Type` + `tauri_specta::Event` derives
**inside the transport layer**; six `cfg(test)` blocks gate real behavior and would silently flip when the code becomes
a dependency; and `backends/mtp.rs`'s `test_hooks` is `pub(in crate::file_system::volume)`, a visibility with **no
cross-crate equivalent** — that needs a redesign, not a mechanical rewrite. Also `backends/mtp.rs` (1,194 lines) is a
veneer over `src/mtp/connection/` (~5,000 lines); extracting one without the other buys nothing.

## Milestones

### P0 — Design the seam set (no code)

Deliverable: the trait set (`ListingHost`, `EventSink`, `CredentialStore`, `IndexNotifier`, runtime `Handle`, settings
accessor), designed against **SMB's 23 sites and the FTP/S3/SFTP requirements**, not against archive's three. Plus the
doc that says which seam a backend uses for what.

Where it lives is part of the decision: a new `crates/cmdr-volume-host/`, or a `host/` module inside `cmdr-fs`. Model:
`crates/cmdr-index/src/indexing/host/`.

**Gates everything else.**

### P1 — Foundation moves

- **P1a**: move `file_system::staging::StagingTemp` (or the half archive needs) down beside the markers already at
  `crates/cmdr-fs/src/staging.rs`. The guard stayed in the app because it registers with write-op state, so this either
  moves with an injected registry or becomes a narrower "name me a temp" seam. Small.
- **P1b**: implement the app-side adapters for the P0 seams, in the `priority/host_policy.rs` shape. Medium. **Nothing
  else can start until this lands.**

P1a and P1b can run in parallel with each other.

### P2 — `cmdr-archive`, the pilot

Move `backends/archive/**` (8,352 lines total, 4,976 non-test). Its complete app-crate coupling:

- `refresh_archive_listings` (one call, listing seam)
- `tauri::async_runtime::spawn` (one site, runtime seam)
- `crate::file_system::staging::StagingTemp` (P1a)
- a rustdoc intra-doc link to `VolumeManager::resolve` that would break `desktop-rust-rustdoc`; rewrite as prose

Everything else it imports (`FileEntry`, the volume types, `ignore_poison`) is already a `cmdr-fs` re-export.

**No `AppHandle`, no `tauri_specta`, no `specta` derive, no credentials, no registry reach-back, no `cfg(test)` behavior
gate.** It's headless (never registers itself), its reading core is already `Volume`-free, and its tests build real
archives on disk — no Docker, no device, no network.

Also required: the manifest (the check runner auto-discovers workspace members, so no Go edit for basic coverage),
`crates/cmdr-archive/{CLAUDE.md,DETAILS.md}` (the doc-pair check requires both), and extending `guardedIndexCrates` in
`index-crate-isolation.go` with a ceiling.

Takes ~10 codec crates out of the app crate's direct manifest: `rc-zip`, `positioned-io`, `zip`, `sevenz-rust2`, `tar`,
`flate2`, `bzip2`, `ruzstd`, `lzma-rust2`, `zeroize`.

The heavy entanglement runs the _other_ way, which is the safe direction, and all of it stays app-side:
`manager/archive_routing.rs` mints and LRU-caps `ArchiveVolume`; `write_operations/archive_edit/` drives
`ArchiveMutator`; `commands/file_system/archive.rs` downcasts to `ArchiveVolume`; `listing/streaming.rs` and
`caching.rs` special-case archive listings; `file_system/watcher.rs` re-registers after LRU eviction.

**P2 ends at a measurement gate, and the gate is real.** Measure `cargo check -p cmdr-archive`, `cargo build` after an
archive edit, and a clean release build, using the same commands as `docs/notes/index-extraction-baseline.md` so the
comparison is sound. **If the numbers don't justify P3, that is a valid answer and P3 doesn't happen.** Record them
either way.

### P3 — `cmdr-smb`, conditional on P2's gate

`backends/smb/**` + `smb_watcher.rs`, 9,616 lines total / 4,273 non-test. Four sub-pieces; 3a and 3b are parallel.

- **P3a**: relocate the `AppHandle` / `tauri_specta` emit (`smb/events.rs`, a `OnceLock<Mutex<Option<AppHandle>>>` set
  from `lib.rs`) behind the P0 `EventSink`. `SmbConnectionChanged` stays in `network/`.
- **P3b**: invert the two registry reach-backs (`reconnect.rs`, `smb_watcher.rs`), both `get_volume_manager().get(id)`
  asking "am I still the live instance?". **This is the one architecturally awkward site in the whole plan.** Either a
  `VolumeRegistry` seam, or move `spawn_watcher_death_reconnect` into `network/smb_upgrade.rs` where the wiring already
  lives. Prefer the second if it works: it needs no new seam.
- **P3c**: the move itself, plus the keychain, posthog, priority, and settings adapters.
- **P3d**: re-home 5,343 lines of `#[cfg(test)] mod smb_*` children, including the Docker-gated integration tests, and
  confirm `desktop-rust-integration-tests`' filter expression still selects them.

### P4 — New backends as crates

FTP, S3, SFTP, each greenfield against the proven seam set. Fully parallel with each other. This is the milestone the
plan exists for.

## Risks

- **The seam set is wrong because it was designed from the wrong backend.** Mitigation: Decision 1 — design from SMB's
  23 sites, which are all enumerated in the survey, before archive lands.
- **`cfg(test)` flips silently when code becomes a dependency.** This has bitten this project three times. `cfg(test)`
  is set only for a crate's own test target. Archive has none, which is part of why it's the pilot; SMB and MTP need
  `any(test, feature = "testing")` conversions.
- **A seam ends up on a per-entry hot path.** Mitigation: Decision 2, plus measure rather than assume.
- **Feature forwarding through an extra crate hop.** `smb2 = { features = ["testing"] }` and
  `mtp-rs = { features = ["virtual-device"] }` back the `smb-e2e` and `virtual-mtp` app features; both would need to
  forward through the new crate. Unverified.
- **The SMB Docker integration tests reach app-side helpers through `use super::*`** from `smb/mod.rs`. What that glob
  closes over wasn't determinable without building.

## Not doing, and why it's tempting

- **Extracting `local_posix`** — Decision 3. It looks like the smallest file and is the hardest job.
- **Extracting MTP** — Decision 4. A real project, not a milestone.
- **Per-crate check lanes.** This is the only thing that would make `pnpm check` faster, and it's genuinely worth doing,
  but it's check-runner work rather than extraction work and it shouldn't hide inside this plan.

## Related

- `docs/architecture.md` — the subsystem map.
- `docs/notes/index-extraction-baseline.md` — the measured before/after for the `cmdr-index` extraction; P2's gate uses
  the same commands so the numbers are comparable.
- `crates/cmdr-index/src/indexing/host/DETAILS.md` — the host-seam pattern this plan copies.
- `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` — the `Volume` trait tiering, and the precedent for
  pre-refusing a tempting-but-wrong change.
