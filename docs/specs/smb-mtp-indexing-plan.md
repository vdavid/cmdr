# SMB + MTP drive indexing, with an "admittedly stale" state

Status: planning, decisions D1–D8 resolved with David (2026-06-19). Worktree `smb-mtp-indexing` (branched off local
`main`). Not yet started. Scope: **both SMB and MTP** in v1; search box stays local-only with multi-volume search as a
required fast-follow.

## The ask, in one line

Let the drive-indexing feature (full scan → live watch → directory sizes + search) cover SMB (NAS) shares and MTP
(phone) devices, not just the local disk. The novel piece is a third freshness state — **admittedly stale** — for
volumes that can disconnect and have no event journal to roll forward.

## Feasibility verdict: yes, and the hard part isn't where you'd guess

The watching half — the thing that sounds scary — already exists and is verified:

- **SMB**: `smb2` implements SMB2 `CHANGE_NOTIFY` (`Watcher`, `FileNotifyEvent`,
  `FileNotifyAction{Added, Removed, Modified, RenamedOldName, RenamedNewName}`), and Cmdr already long-polls it in
  `file_system/volume/backends/smb_watcher.rs` (recursive watch on the share root, 200 ms debounce, overflow handling
  via `STATUS_NOTIFY_ENUM_DIR` → full refresh).
- **MTP**: `mtp-rs` exposes PTP interrupt-endpoint events
  (`DeviceEvent::{ObjectAdded, ObjectRemoved, ObjectInfoChanged, StoreAdded, StoreRemoved, …}` via `next_event()`), and
  Cmdr runs an event loop in `mtp/connection/event_loop.rs`. **Caveat (verified):** PTP events carry an opaque _object
  handle_ (a `u32`), not a path — this is a **protocol property** (events are `code + 3×u32`), not an mtp-rs or Cmdr
  deficiency. mtp-rs preserves the handle and exposes `get_object_info(handle) → ObjectInfo { parent, filename, … }`, so
  a handle resolves to (parent handle + name) and you walk parents to the root for a full path. Cmdr currently
  _discards_ the handle and blanket-refreshes open panes. So MTP needs a **handle→path pre-step** (see the dedicated
  pre-step milestone and rabbit hole #2) — and that pre-step is cleaner for the index than for the live pane, because
  the index can store the handle per entry (precedent: the `inode` column) so even `ObjectRemoved` (object already gone)
  resolves via the index.

**But today both watchers feed only the live folder view (pane refresh), not the index — and not via a reusable bus.**
The SMB watcher calls `caching::notify_directory_changed(volume_id, parent, change)` directly (it patches
`LISTING_CACHE` and enriches inline); the MTP loop emits a device-wide pane-refresh signal scoped to open listings.
Neither is a publish/subscribe channel you can simply add a second subscriber to. The index is a per-volume SQLite DB,
and the system is hardwired to exactly one volume: `"root"` at `/`. So the real work is three things, none of which is
"make watching work":

1. **Generalize the indexer from one volume to many** (the structural refactor).
2. **Add a scan path that walks over the `Volume` trait** (SMB/MTP listings) instead of jwalk (local only), and route
   the existing watch events into the per-volume index writer.
3. **Build the freshness state model and its UX** — the actual product novelty.

`IndexManager` was already written volume-aware (`IndexManager::new(volume_id, volume_root, app)`, DB path already
`index-{volume_id}.db`). The only true hardcoding is that the global `INDEXING: Mutex<IndexPhase>` holds a single
manager and `start_indexing` passes `"root"`/`/`. That is a real but contained refactor, not a rewrite.

## Why these volumes are genuinely different (the core insight)

Local disk gets its freshness for free from **FSEvents with a historical event journal**: on launch we replay from
`last_event_id` (or rescan if the gap > 10M), so "the app was off" self-heals to fresh cheaply. SMB `CHANGE_NOTIFY` and
MTP PTP events have **no journal** — they deliver events only while you're connected and actively watching. Any gap (app
off, volume asleep, wifi blip, USB unplug, session drop, server-side notify overflow) loses those events
_irrecoverably_. There is no `sinceWhen` to roll forward.

So for SMB/MTP, freshness is **binary**: continuously watched since the last scan ⇒ we know what's where; any
interruption ⇒ we cannot know what drifted. That's exactly the user's framing — "Cmdr was off / your phone was
unplugged, so we maybe missed some changes." The three states the user described map cleanly onto this:

1. **Fresh** — full scan done AND watch unbroken since. Authoritative.
2. **Admittedly stale** — an index exists, but watch continuity broke. Data may have drifted; the user is fine browsing
   it anyway, with an obvious rescan affordance.
3. **(Re)scanning** — a full scan is running.

Local disk has #1 and #3; #2 is the new state, and it's _only_ meaningful for volumes that can disconnect and lack a
journal.

## The freshness state model

One per-volume freshness value, surfaced to the UI as four visible states (David's colors):

- **Disabled (gray)** — indexing off for this volume (user choice, or the default-off for SMB/MTP until enabled).
- **Scanning (blue)** — full scan in progress (initial or rescan). Maps to existing `Initializing`/`Running`+scanning.
- **Fresh (green)** — scan complete and watch unbroken since. Authoritative.
- **Stale (yellow)** — index exists, watch continuity broken. Browsable, clearly marked, one-click rescan.

Internally this is the existing `IndexPhase` (Disabled/Initializing/Running/ShuttingDown) plus a per-volume
**`freshness`** signal that only `Running` volumes carry. The load-bearing facts:

- **Freshness lives partly in-memory, partly in `meta`.** `meta.scan_completed_at` (already exists) proves a scan
  finished. But "watch unbroken since" _cannot_ be persisted as fresh across an app restart, because the restart itself
  is a continuity break. So on launch, **every persisted SMB/MTP index loads as Stale** (we weren't watching while off)
  — unlike local disk, which replays its journal to Fresh. This is correct and honest, not a limitation to fix.
- **Fresh ⇒ Stale transition triggers** — note these are _different code paths_ in `smb_watcher.rs`, not one trigger:
  - **watcher task died** (disconnect, SMB session drop, MTP device gone): the watcher loop returns; the freshness layer
    must observe that and flip to Stale.
  - **`CHANGE_NOTIFY` overflow (`STATUS_NOTIFY_ENUM_DIR`)**: today the SMB watcher _keeps watching_ and emits a
    FullRefresh — it does **not** kill the task. So "overflow ⇒ Stale" is a policy we must add: the watcher needs to
    _signal overflow upward_ to the freshness layer (or, better, treat overflow as "rescan the affected subtree" since
    the watch is still live). Don't conflate it with disconnect; the control flow is opposite.
  - **app launch with a pre-existing SMB/MTP index** (covered above).
  - MTP: same triggers once M0.5 + M4 land. **MTP Fresh is as strong as SMB (D4): green ⇔ a live connection is up and
    watched.** We assume a plugged-in phone emits all its updates; the moment the connection drops, it's Stale like any
    other.
- **Interrupted / disconnected-mid-scan ⇒ "not indexed" (gray), discard the partial (D-interrupted).** If a network scan
  dies mid-walk (NAS/phone vanishes), the partial index is worthless: the disconnection itself means we'd be stale
  anyway, so there's nothing to salvage. So an interrupted scan resets the volume to plain **gray / not-indexed** (same
  as never-scanned), discarding partial data, with the normal "Turn on indexing" / rescan affordance — no distinct
  "interrupted" state, no browsable half-snapshot. The existing `scan_completed_at`-absent ⇒ no-Fresh mechanism already
  prevents a half-scan from reading Fresh; we just present it as gray rather than keeping the partial rows live.
- **Mid-scan reconnect.** SMB `do_attempt_reconnect` respawns the watcher. If a reconnect lands _during_ a scan, the
  re-armed watcher's buffer must still cover the gap relative to the scan snapshot — i.e. the pre-arm-before-scan
  ordering (below) has to survive a mid-scan watcher respawn, not just the initial arm. Otherwise the post-scan state is
  a false Fresh. Handle reconnect-during-scan as: finish-or-abort the scan, then decide freshness from whether the watch
  buffer is continuous.
- **Stale ⇒ Fresh** happens only via a full rescan (then continuous watch). **This binary is a v1 scoping choice, not a
  fundamental limit.** A cheaper narrowing exists and we're choosing not to build it in v1: `verifier.rs` already does a
  per-directory readdir + mtime diff-and-correct on navigation, so a "verify the visible subtree on reconnect" pass
  could shrink what's marked stale without a full rescan. We defer it deliberately (keeps the rescan-cost-is-a-real-
  decision story honest), but the model should leave room for it.
- **The watcher must run while the volume is connected, not only while its folder is open.** For the Fresh claim to
  hold, the SMB watcher's lifetime has to be tied to the index being live for that volume, not to a pane showing that
  share. Confirm/adjust `smb_watcher.rs` start/stop ownership during M2 (the MTP event loop is already
  connection-scoped, which is the model to match).
- **Scan/watch ordering must mirror local disk**: arm the watcher (start buffering) _before_ the scan snapshot, so
  changes during the scan aren't lost in the gap between "finished listing" and "started watching." Local does this with
  `DriveWatcher` at `sinceWhen=0`; SMB/MTP must do the equivalent (pre-arm `CHANGE_NOTIFY` / event loop, buffer, replay
  after scan completes).

### Accepting stale: passive badge, plus a one-time dialog the first time it happens (D2)

Stale is primarily a **quiet yellow badge** — not clicking rescan _is_ the acceptance, no per-reconnect nagging. But the
**first time any external drive's index goes stale**, show a single dialog so the user learns the concept:

- Body: explains the drive may have changed while disconnected; sizes/search might be a bit off; the yellow status is
  always visible and a rescan refreshes it.
- Buttons: **[Never show again]** **[Close]**.
- "Never show again" is resettable in Settings (see below). Even with it off, the yellow badge still shows.

## UX / UI design

### Per-drive badge: two placements (D8, D3)

The status badge appears in **two** spots, both reusing the existing colored-indicator + `use:tooltip` pattern that
`VolumeBreadcrumb.svelte` already uses for the SMB connection light and USB-speed ring:

1. **Always-visible, next to the dropdown trigger** — beside the existing green/yellow SMB light, reflecting the
   **active drive's** index status at all times (not only when the dropdown is open). This is the primary surface.
2. **Inside the dropdown, per-drive** — on each **drive** row (after `.volume-label`, alongside `smb-indicator`).
   **Not** on the "Favorites" group or non-drive entries (D8) — only real volumes.

Four states, AA+ contrast, each with a hover/focus tooltip:

- **Gray (disabled)** — "Indexing is off for this drive. Turn it on to see folder sizes and search here."
- **Blue (scanning)** — "Indexing… 42,000 files so far" + scan progress/ETA where available (reuse the tier-1/tier-2 ETA
  machinery, re-calibrated for network/USB cost).
- **Green (fresh)** — "Indexed and up to date. Last indexed 2026-06-19, took 2 min, 14 s." (per-drive last-scan
  duration; see below).
- **Yellow (stale)** — "This drive may have changed while it was disconnected. Sizes and search might be a bit off.
  Rescan to refresh." (active voice, no "error/failed").

**Click the badge → a small menu** (the app's dropdown/popover primitive, not a native menu, for theming):

- Disabled: "Turn on indexing for this drive".
- Fresh/Stale: "Rescan now", "Turn off indexing for this drive".
- Scanning: "Stop indexing".
- Footer (non-interactive): **last indexed: `<date>` · took `<duration>`**.

### First-connect notification (D6)

External-drive indexing is **off by default**. The **first time a new drive is selected**, show a notification (only if
Settings ▸ Drive indexing is on, and only if "Ask for each drive" is on):

- Buttons: **[Don't ask again for any drives]** **[Don't ask again for this drive]** **[Enable indexing]**.
- "Enable indexing" turns it on for that drive (kicks off the scan).
- "Don't ask again for this drive" remembers per-drive silence.
- "Don't ask again for any drives" flips the global "Ask for each drive" toggle off.

### Settings ▸ Drive indexing additions

- **"Ask for each drive"** toggle (ON by default) — gates the first-connect notification (D6).
- **"Re-enable notifications for all drives"** button — clears all per-drive "don't ask" silences. Disabled, with an
  explanatory tooltip, if the user has never silenced a specific drive.
- **"Notify me if any external drive index goes stale"** toggle (ON by default) — gates the one-time stale dialog (D2).
  Helper text: "You'll still see the yellow status if you turn this off."

### Last-index duration, per drive

`scan_duration_ms` and `scan_completed_at` already live in each volume's `meta`. Surface them in the badge tooltip and
menu footer. Local disk already has this data, so its badge renders too (D8) — a consistent "every drive has a status"
model. Format with the existing duration/date formatters (ISO dates, friendly durations).

### Global indicator interplay (D3 resolved)

The status badge **next to the dropdown trigger** (placement 1 above) carries the active-drive freshness always-visible,
right by the SMB light — that's the answer to D3. The toolbar-corner `IndexingStatusIndicator` hourglass stays as-is (an
activity glyph, not per-volume). Optionally add a one-line stale note to the size-cell tooltip when the active volume is
stale; keep it light.

## Architecture changes

### 1. One volume → many (the registry)

Replace the single `INDEXING: Mutex<IndexPhase>` with a registry keyed by volume id (e.g.
`Mutex<HashMap<VolumeId, IndexPhase>>`, or a `DashMap`, or a manager-of-managers). Every public entry point in
`state.rs` (`get_status`, `get_dir_stats[_batch]`, `trigger_verification`, `start/stop/clear`, `force_scan`) gains a
volume-id dimension. The lock-first reservation (`Disabled → Initializing`) becomes per-volume. `ReadPool`,
`PendingSizes`, and the memory watchdog become per-volume (or volume-aware).

**Routing reads is the subtle part.** Enrichment (`enrich_entries_with_index`) currently assumes the one root DB and
_early-returns_ on `/Volumes/`, `/mnt/`, and `mtp-*` paths via `scanner::should_exclude` — a deliberate skip that avoids
DB work and "Parent path not found" log spam on every network-mount refresh (see `enrichment.rs` / DETAILS.md). The flip
is **"skip if no index is registered for this volume," not "always route."** So the fast early-return _survives_ for any
volume without a registered index DB (which, in M1, is every volume except root) — behavior is preserved exactly. Only a
volume that _has_ a registered index gets routed to its `ReadPool`. The listing already carries its `volume_id` (the
cache snapshot has it — see `partial_agg::collect_hot_paths`, which filters by volume_id). Without this nuance, M1 would
either be cosmetic or would regress perf/logging on the exact network mounts this feature targets.

De-risking: **M1 does this refactor with local disk still the only indexed volume** — same behavior, new shape — so the
registry lands and is proven green before any network/USB backend exists. The regression proof is `integration_tests.rs`
passing unchanged.

### 2. A `Volume`-trait scan path

The jwalk scanner is local-FS-only (`getattrlistbulk`) and `should_exclude` deliberately blocks `/Volumes/` + mtp. SMB
and MTP need a scanner that walks via the `Volume` trait's recursive listing (the same API the live pane uses), pulling
sizes from SMB stat / MTP `ObjectInfo`. Everything downstream of `EntryRow` (ID assignment, the single-writer thread,
the aggregator, `dir_stats`) is backend-agnostic and reused as-is. The new walker must be:

- **cancelable** at every round trip (volume can vanish),
- wrapped in the project's `blocking_with_timeout` discipline (network/USB syscalls block 30–120 s — see
  `src-tauri/CLAUDE.md`),
- tolerant of partial completion → reset to **gray / not-indexed**, discarding the partial (D-interrupted); the existing
  `scan_completed_at`-absent ⇒ no-Fresh mechanism already prevents a half-scan reading Fresh,
- **wrapped in `objc2::rc::autoreleasepool`** on macOS, like the index writer thread: the walker and the SMB
  `stat_via_volume` path call through to NSURL/`NSFileManager`-adjacent metadata code on long-lived threads, and the
  `indexing/CLAUDE.md` rule (unpooled ObjC autoreleases leak multi-GB over hours) applies directly.

### 3. Watch events → index writer

This is **not** "add a second subscriber" — there's no bus (see Feasibility). It's a concrete integration with two real
constraints:

- **SMB integration point + ordering.** `smb_watcher.rs` → `caching::notify_directory_changed` already patches the
  listing cache _and_ enriches the new entry inline for the open pane. The index write (translate the `DirectoryChange`
  into `UpsertEntryV2`/`MoveEntryV2`/`DeleteEntryById`/`DeleteSubtreeById` via `store::resolve_path`) has to hook here
  too, and the **ordering must be decided**: the inline pane-enrich reads the size _before_ the index write lands, so
  naively the pane shows the pre-event size and never refreshes (the index write doesn't re-emit a pane diff). Resolve
  by sequencing the index write first and having it emit `index-dir-updated` for the affected dir (the existing FE
  refresh path), or by enriching the pane from the just-written index. Pick one in M2; don't leave it implicit. This
  couples the listing module to the indexer — keep that coupling explicit and one-directional (listing notifies indexer,
  not vice versa).
- **MTP gets its per-event signal from the M0.5 pre-step.** Raw PTP events are pathless device-wide pings (rabbit hole
  #2); M0.5 adds the `handle → path` resolver (`GetObjectInfo` → parent chain → full path). M4 feeds those resolved
  targeted changes into the per-volume writer, and resolves `ObjectRemoved{handle}` via the **handle stored per index
  entry** (the object is gone, so `GetObjectInfo` can't help — the index can).

Both paths keep the **stat-verify-before-delete** rule (SMB/MTP coalescing can deliver false removals too) and the
single-writer-per-DB invariant (the per-volume writer is the only writer for that volume's DB; watch translation
enqueues messages, it never writes directly).

## Rabbit holes and difficulties (what to watch)

1. **MTP identity instability (the big one).** The MTP volume id is an FNV hash of USB bus+port topology
   (`location_id`), so an index only re-matches if the phone is replugged into the _same port_. `mtp-rs` exposes a
   stable `serial_number`, but Cmdr doesn't use it for the id, and many Android devices in MTP mode report none.
   Consequence: without a serial, MTP indexing forces a rescan on every connection — gutting the feature's value (a
   same-port replug still re-matches; a different port doesn't). M4 starts by switching the MTP volume id to prefer
   `serial_number` when present. That ripples wider than persistence: the volume id `"mtp-{device}:{storage}"` is
   _parsed by string-splitting on `:`_ in `event_loop.rs` and keys `get_listings_by_volume_prefix`, `PathHandleCache`,
   the event debouncer, and the disconnect registry. A serial containing `:` (some devices) breaks `split(':')`. M4 must
   audit every volume*id \_parser*, not just every persistence site — and this `:`-split is itself fragile under
   `.claude/rules/no-string-matching.md`.
2. **MTP events are pathless (protocol), and the fix is a clean pre-step (resolved: build it, index MTP for real).** The
   handle-not-path property is the PTP wire format, not a bug; mtp-rs already exposes
   `get_object_info(handle) → {parent, filename}`. The pre-step (its own milestone, M0.5) resolves `ObjectAdded{handle}`
   → `GetObjectInfo` → parent handle + name → walk/cached parents → full path, and emits a **targeted** dir change
   instead of today's blanket open-pane refresh — which _also_ improves the live pane on its own. For the index, store
   the MTP handle per entry (precedent: the `inode` column + `find_entry_by_inode`), so `ObjectRemoved{handle}` resolves
   by index lookup even though the object is gone. **D4 (resolved):** assume a plugged-in phone emits all its updates,
   so MTP Fresh is as strong as SMB Fresh — green ⇔ live connection up and watched; any disconnect ⇒ Stale. **Android
   handle re-keying** on media rescans is bounded by the same stale→rescan cycle (a rescan rebuilds the handle map), so
   it can't silently corrupt.
3. **Network/USB calls block indefinitely.** Every scan/list/stat can hang 30–120 s on a slow or hung mount
   (`src-tauri/CLAUDE.md` is emphatic). The scanner, the watch→index translator, and the freshness transitions all run
   off the IPC thread and must use `blocking_with_timeout`; a vanishing volume must pause/teardown, never hang or crash
   a writer thread.
4. **Scan cost and ETA recalibration.** Local jwalk does ~5M files in ~2 min via bulk syscalls. SMB/MTP do per-dir round
   trips, plausibly 10–100× slower. The two-tier calibrated/rough ETA is tuned for local; it needs its own per-backend
   seed. The "last indexed: took N min" display becomes genuinely informative _and_ the rescan cost becomes a real
   user-facing decision (which is exactly why #2 — accept stale — matters).
5. **`CHANGE_NOTIFY` reliability varies by server.** Windows, Samba, and consumer-NAS firmwares differ in recursive
   notify support and overflow behavior. We can't assume Fresh is rock-solid everywhere; overflow must downgrade to
   Stale (or trigger a targeted subtree rescan). Evidence-anchor any per-server findings (per `docs.md`).
6. **Disconnected-but-index-exists ⇒ latent offline browsing.** Because the index persists on disk, a disconnected
   NAS/phone _could_ be browsed from its stale index. That's a tempting feature and a real expectations trap. **Out of
   scope for v1**, but design the state model so a disconnected volume's index is never silently served as if live, and
   so offline browsing remains a clean future addition (D5).
7. **Index proliferation and cleanup.** Local has one DB; now every share and every phone-port pair spawns one. Need a
   retention policy (LRU/size cap, "forget this drive" action, prune on schema bump) so `Application Support` doesn't
   grow unbounded. New, since local never had this problem.
8. **Multi-volume resource coordination — global watchdog, but parallel scans are fine (resolved).** The per-volume 16
   GB number is a _catastrophe stop_ (warn at 8 GB, hard-stop at 16 GB to protect the machine), **not** expected usage.
   Real scan memory is dominated by the in-memory accumulator maps (scale with _directory_ count) + the bounded 20K
   writer channel — hundreds of MB for normal volumes. The bottleneck for SMB/MTP is the network/USB wire, not RAM. So
   **allow scans to run in parallel** (David's call) and replace the per-volume watchdog with a single **global** memory
   budget as the safety net — no one-at-a-time serialization. **Measure actual peak RSS during a real scan in M1** to
   confirm the budget number (we have the tooling; `memory_watchdog.rs` reads `mach_task_info`). The global budget lands
   in M2 with the first concurrent-capable backend; retention/cleanup still waits for M5.

9. **SMB indexing requires the `direct` (smb2) connection, not the macOS `os_mount` (resolved).** `CHANGE_NOTIFY`
   watching runs through smb2 anyway, and smb2 parallelizes listing well (David's point), so indexing is gated on the
   `smbConnectionState == 'direct'` mode. An `os_mount` volume must upgrade first — the path already exists
   (`upgrade_smb_to_direct`). Make "Turn on indexing" trigger/await the upgrade when needed; if the upgrade fails,
   indexing stays disabled with an honest tooltip.
10. **Per-volume path/case semantics.** `name_folded` + the `platform_case` collation is currently a compile-time
    macOS-vs-Linux decision. SMB shares can be case-insensitive (Windows) or sensitive (Samba/ext4); MTP is its own
    namespace. The **one-DB-per-volume boundary makes this more tractable than it sounds**: `platform_case` is
    registered per-connection and `name_folded` is computed at insert time, so each volume's DB can pick its own case
    policy cleanly. The real risk isn't the storage — it's that the SMB server's case-sensitivity is _unknowable without
    probing_ (create `Foo`/`foo`, or read a server capability), so v1 may have to assume case-insensitive and document
    the approximation. Firmlink normalization is local-only and must be bypassed for virtual SMB/MTP paths.
11. **Stale data must never weaken destructive-op safety.** Copy/move/delete must keep verifying against the live FS
    before acting and never trust the index for the destructive decision (re-stat, safe-overwrite). Confirm the stale
    path doesn't let a wrong size or a phantom entry drive a real operation. (Search-result stale-labeling is a _future
    multi-volume_ concern, not v1: under D7 the searchable index is local-disk-only, and local disk is the journaled
    volume that loads Fresh — so the v1 search index is effectively never Stale.) "Protect the user's data" is
    non-negotiable.
12. **Search is single-volume by construction — a milestone, not a footnote.** `search/index.rs::load_search_index`
    loads exactly one `ReadPool` (root's DB) into one in-memory `SearchIndex`, and `WRITER_GENERATION` is a single
    process-global atomic shared by every writer. So there is no per-volume search today, and any writer mutation fires
    staleness for the single index. True multi-volume search (N in-memory indexes, N generations, merge, per-result
    volume attribution + freshness labeling) is comparable in size to the indexing refactor itself. **v1 recommendation
    (D7): keep the search box local-disk-only.** SMB/MTP indexes feed _sizes and dir-stats_ (the headline win David
    named) but not full-text/AI search. Promote multi-volume search to its own future effort. Don't let this hide in a
    rabbit-hole list and detonate in M5.

13. **FDA gate doesn't apply to SMB/MTP enable.** `should_auto_start_indexing` defers _local_ auto-start until the user
    decides about Full Disk Access (scanning `/` triggers TCC popups). Network shares and USB devices are **not**
    TCC-protected, so a per-volume "Turn on indexing" for SMB/MTP must be **FDA-independent** — it shouldn't wait on the
    FDA gate, and the per-volume registry reservation must not route SMB/MTP starts through
    `should_auto_start_indexing`. State this so a future agent doesn't "consistency"-wrap them in the gate and silently
    block NAS indexing.

## Decisions (resolved with David, 2026-06-19)

- **D1 — Scope of v1: BOTH SMB and MTP.** MTP is in, contingent on the handle→path pre-step (M0.5), which David asked
  for and which is feasible (rabbit hole #2). Not MTP-droppable anymore.
- **D2 — Stale acceptance: passive badge, plus a one-time dialog the first time it happens.** Buttons [Never show again]
  [Close]; "Never show again" resettable via the Settings toggle "Notify me if any external drive index goes stale
  (you'll still see the yellow status if you turn this off)". See UX § Accepting stale.
- **D3 — Always-visible badge next to the dropdown trigger,** by the existing green/yellow SMB light, carrying the
  active drive's freshness. Corner hourglass stays as-is. See UX § badge placement 1.
- **D4 — MTP Fresh is as strong as SMB.** Assume a plugged-in phone emits all updates: green ⇔ live connection up and
  watched, Stale the moment it disconnects. No weaker-claim wording.
- **D5 — Offline browsing of a disconnected index: out of scope for v1** (David saved the idea for later). Model stays
  design-compatible for it.
- **D-interrupted — An interrupted/disconnected mid-scan resets to "not indexed" (gray), discarding the partial.** A
  half-scan is worthless once disconnected (we'd be stale anyway), so no distinct "interrupted" state and no browsable
  half-snapshot.
- **D6 — Off by default; first-connect notification.** First time a new drive is selected: notification with [Don't ask
  again for any drives] [Don't ask again for this drive] [Enable indexing]. Suppressed if Settings ▸ Drive indexing is
  off or "Ask for each drive" is off. Settings gets an "Ask for each drive" toggle (ON) + a "Re-enable notifications for
  all drives" button (disabled w/ tooltip until the user has silenced a specific drive). Local stays auto. See UX §
  First-connect notification + Settings.
- **D7 — Search box local-disk-only in v1.** SMB/MTP indexes power sizes + dir-stats, not the search box. **Multi-volume
  search must fast-follow** (David: explicit) — its own effort right after v1, not deferred indefinitely.
- **D8 — Badge on every drive incl. local,** but only on real **drive** rows in the dropdown (not "Favorites"/groups),
  plus the always-visible one by the trigger (D3).

## Milestones

Sequential is fine and expected (we're not in a hurry). Each milestone ends green on the right `pnpm check` scope.

### M0 — Decisions + model sign-off ✅

D1–D8 + D-interrupted resolved (above). Freshness states locked: gray (disabled / not-indexed, incl. interrupted-scan),
blue (scanning), green (fresh), yellow (stale) — plus all transitions and persistence rules. Copy inventory drafted. No
code.

### M0.5 — Make MTP change events pathful (standalone pre-step)

David's requested pre-step; valuable on its own (the live MTP pane gets targeted refreshes instead of blanket re-lists)
and the foundation for MTP indexing (M4).

- Add a handle→path resolver in the MTP layer: `ObjectAdded{handle}` → `get_object_info(handle)` → parent handle +
  filename → walk/cached parents to the storage root → full virtual path. Cancelable, timeout-wrapped, `autoreleasepool`
  if it touches ObjC.
- In `event_loop.rs`, emit a **targeted** dir-change for the resolved path instead of the blanket
  `emit_directory_changed`; fall back to blanket refresh on resolution failure (parent not cached, handle invalid).
- Note: `ObjectRemoved` can't resolve via `GetObjectInfo` (object gone) — for the live pane it falls back to blanket
  refresh; the index path (M4) will resolve it via a per-entry stored handle instead.
- **Tests:** unit-test the resolver (cached parent / uncached-parent walk / invalid handle / root object); the
  targeted-vs-blanket decision is a pure branch — test it. Manual device verification that adding a file in a non-open
  subdir now targets the right dir.
- **Docs:** `mtp/connection` CLAUDE.md/DETAILS for the resolver + the blanket fallback rationale.
- **Checks:** `pnpm check rust` + manual device pass.

### M1 — Multi-volume index registry (local-only, no behavior change)

The structural refactor in isolation, fully de-risked because behavior is unchanged.

- Replace the single `INDEXING` mutex with a per-volume registry; per-volume `ReadPool`/`PendingSizes`/watchdog;
  per-volume lock-first reservation.
- Route enrichment/verification/IPC reads by the listing's `volume_id`. The `should_exclude` gate becomes **"skip if no
  index is registered for this volume"** (not "always route") — so the existing fast early-return survives for every
  unindexed volume and behavior is byte-identical in M1; only a volume with a registered index DB gets routed.
- **Tests (TDD where it bites):** the registry's per-volume lifecycle transitions and the routing decision are pure-ish
  and should be **test-first** (extend the `is_initializing_phase`-style classifier pattern from `indexing/DETAILS.md` §
  Testing bar). Reuse `stress_tests_lifecycle.rs` patterns for concurrent start/stop across two volume ids. Integration:
  existing `integration_tests.rs` must pass unchanged (the regression proof). E2E: `indexing.spec.ts` unchanged.
- **Docs:** update `indexing/CLAUDE.md` (the single-volume assumption is now a registry — this is a must-know change)
  and `indexing/DETAILS.md` (architecture: registry, routing, per-volume pools).
- **Measure peak RSS during a real local scan** (via `memory_watchdog.rs` / `mach_task_info`, or sampling the process)
  to get the hard number that sets M2's global memory budget and confirms parallel scans are safe (David's #3).
- **Checks:** `pnpm check rust` then full `pnpm check`. Smoke-test one indexing test first per `test-infra-smoke-first`.

### M2 — SMB indexing end-to-end + freshness state + resource coordination

- **Gate on the `direct` (smb2) connection** (rabbit hole #13): "Turn on indexing" triggers/awaits
  `upgrade_smb_to_direct` if the volume is `os_mount`; if the upgrade fails, indexing stays off with an honest tooltip.
- `Volume`-trait scan path: cancelable at every round trip, `blocking_with_timeout`-wrapped, `autoreleasepool`-wrapped
  on macOS (the walker + `stat_via_volume` touch NSURL on long-lived threads), partial-completion → **reset to gray /
  not-indexed**, discarding the partial (D-interrupted).
- Decide and implement the **SMB watch→index integration point and ordering** (the `notify_directory_changed` coupling
  - index-write-vs-pane-enrich sequencing — see Architecture §3); pre-arm-before-scan ordering that survives a mid-scan
    watcher respawn.
- **Sub-step (isolate it): change the SMB watcher's lifetime from pane-scoped to volume-index-scoped.** Today
  `smb_watcher.rs` start/stop couples to pane visibility; the index needs it to run while the volume's index is live.
  This interacts with `do_attempt_reconnect`. Do it as its own commit with a **regression check that closing a pane does
  not tear down the index watcher** — easy to half-do otherwise.
- Implement the `freshness` signal + the full transition table for SMB: load-as-Stale-on-launch, watcher-died→Stale,
  overflow (signal upward; rescan-subtree or Stale), incomplete-scan, mid-scan reconnect.
- **Resource coordination (rabbit hole #8): allow parallel scans; make the memory watchdog global** (replace the
  per-volume 16 GB stop with one global budget). No one-at-a-time serialization — the wire is the bottleneck, not RAM.
  **Measure peak RSS during a real scan (in M1) to set the budget number.**
- **Tests:** TDD the freshness transition table (pure state machine, mock watcher-died / overflow / launch-with-index /
  incomplete / mid-scan-reconnect inputs) — real red→green per `tdd-red-green.md`. Backend integration against the
  `test/smb-servers/` fixtures: scan a fixture share, mutate it, assert the index reflects the change while Fresh.
  **First confirm the fixtures can simulate a mid-session disconnect** (the `flaky` container / lease model may not
  expose a programmatic session-drop); if not, assert the Fresh→Stale transition at the watcher-task-died seam in a unit
  test instead. (`test-infra-smoke-first`: run 1–2 fixture tests before the full lane.)
- **Docs:** `indexing/DETAILS.md` (SMB scan path, the watch→index integration decision + ordering, freshness model + the
  no-journal rationale, the global memory budget + parallel-scan stance); a guardrail line in `indexing/CLAUDE.md`;
  cross-link from `file_system/volume/backends` SMB docs.
- **Checks:** `pnpm check` incl. the SMB-fixture lane.

### M3 — Freshness UX (per-drive badge + last-duration + menu)

Can overlap M2's backend once the IPC shape is fixed.

- Per-drive status badge in `VolumeBreadcrumb.svelte` (4 states, tooltips, AA+ contrast, `prefers-reduced-motion`),
  click-menu (turn on/off, rescan, last-indexed footer), last-index-duration surfacing (local disk included).
- IPC: extend the index-status response to be per-volume and carry `freshness` + `scan_completed_at` +
  `scan_duration_ms`.
- **Tests:** Vitest for the badge state→color/copy mapping and the menu items per state (pure mapping, unit-testable);
  a11y test for the badge (focusable, tooltip via `aria-describedby`, matching the `IndexingStatusIndicator.a11y`
  pattern). Manual MCP verification in the running app: connect an SMB share, watch blue→green, simulate disconnect →
  yellow, rescan → blue→green; screenshot both themes.
- **Docs:** `src/lib/file-explorer/navigation` CLAUDE.md/DETAILS for the new badge; copy inventory in this spec.
- **Checks:** `pnpm check svelte desktop` + a11y lane.

### M4 — MTP indexing end-to-end (live, on top of M0.5)

Builds on the pathful pre-step (M0.5). MTP gets a real live-watched index that claims Fresh exactly like SMB (D4: green
while the connection is up and watched, Stale on any disconnect).

- **MTP identity fix:** switch the MTP volume id to prefer `serial_number`; fall back to topology `location_id` with a
  documented "same-port-only" limitation surfaced in the tooltip. **Audit every volume*id \_parser*** (the `:`-split in
  `event_loop.rs` and its keyed caches: `get_listings_by_volume_prefix`, `PathHandleCache`, the debouncer, the
  disconnect registry), not just persistence sites; a serial containing `:` must not break parsing.
- **MTP scan** over the `Volume` trait (cancelable, timeout- and `autoreleasepool`-wrapped), storing the **MTP object
  handle per entry** in the index (reuse/extend the `inode` column precedent) so removals resolve by index lookup.
- **Watch→index:** the M0.5 resolver feeds targeted dir changes into the per-volume writer; `ObjectRemoved{handle}`
  resolves via the stored handle. Apply the freshness model (Fresh while live-watched; Stale on disconnect/launch).
- **Tests:** identity-derivation + volume_id-parser tests (serial present / absent / contains `:` / port change);
  handle→entry lookup for removals; integration with the MTP mock layer if one exists, else documented manual-device
  verification. **Re-run the data-safety tests yourself** per `verify-delegated-work.md`.
- **Docs:** MTP indexing + identity + per-entry-handle caveats in the MTP backend docs and `indexing/DETAILS.md`.
- **Checks:** `pnpm check` + manual device pass.

### M5 — Hardening

- Index retention/cleanup (LRU/size cap, "forget this drive", prune on schema bump). **Pruning a Stale index must
  transition that volume to Disabled, not leave a dangling Stale badge.** (The global memory budget already landed in
  M2; scans run in parallel, no serialization.)
- Disconnect-storm resilience.
- Search: per D7, no multi-volume search in v1 — so M5 only verifies the search index stays correct under the registry
  (single-volume, local-disk) and that SMB/MTP volumes are cleanly excluded from the search box. (True multi-volume
  search is a separate future effort, not hardening.)
- **Tests:** retention policy unit tests (incl. prune-Stale→Disabled); a "connect/disconnect 20×" stress test (mirrors
  `stress_tests_lifecycle.rs`).
- **Docs:** finalize `indexing/CLAUDE.md` must-knows; update `docs/architecture.md` map entry; status of this spec.
- **Checks:** full `pnpm check --include-slow`.

## Parallelization

Mostly sequential by design (each milestone builds on the last). Genuinely-safe parallelism:

- **M0.5 (MTP pathful) is independent of M1 (index registry)** — different subsystems (MTP layer vs index core), so they
  can proceed in parallel. M4 needs both.
- Within **M3**, the badge-rendering/Vitest work and the IPC-shape change can proceed together once the response struct
  is agreed. M1 must land fully before M2; M2's freshness backend must land before M3's UX is meaningful; M4 needs
  M0.5 + M2. Do **not** parallelize M1 internally — it's the load-bearing refactor and wants one careful pass with the
  full check suite between steps.

## Testing summary

- **Pure functions/state machines test-first** (TDD): the MTP handle→path resolver branches (M0.5), per-volume registry
  transitions (M1), freshness transition table (M2), badge state→copy mapping (M3), MTP identity derivation + volume_id
  parser (M4).
- **Integration**: `integration_tests.rs` unchanged as the M1 regression proof; SMB-fixture scan+mutate (+disconnect if
  the fixtures support it) (M2); MTP mock/device, incl. handle→entry removal lookup (M4).
- **A11y**: new badge (M3), matching the existing `IndexingStatusIndicator.a11y.test.ts` mock pattern exactly.
- **Manual MCP**: the full connect→fresh→stale→rescan loop in the running app, both themes (M3).
- **Stress**: concurrent multi-volume scans under the global memory budget (M2); connect/disconnect cycling (M5).

## Copy inventory (sentence case, active voice, no "error/failed")

- Badge tooltips: see UX section (gray/blue/green/yellow).
- Menu items: "Turn on indexing for this drive", "Turn off indexing for this drive", "Rescan now", "Stop indexing".
- Footer: "Last indexed: 2026-06-19 · took 2 min, 14 s".
- Stale note (size-cell tooltip, if D3 picks it): "This drive's index may be stale."

All copy is provisional and goes through the human-reviewed UI-copy path per principle 6 (humans to humans).
