# Drive indexing module

Background-indexes volumes into per-volume SQLite DBs with recursive size aggregates, so listings show directory sizes.
Local disk, SMB shares, and MTP storages each get their own DB.

## Module map

Per-file roles: DETAILS § "Module structure" or `codegraph_search`. The load-bearing groupings:

- **Lifecycle / state**: `state.rs`, `routing.rs`, `queries.rs`, `manager.rs`.
- **Write path**: `writer/`, `scanner.rs` (jwalk, LOCAL only), `volume_scanner.rs` (`Volume`-trait scan, SMB/MTP),
  `aggregator.rs`, `reconciler.rs` + `event_loop.rs`.
- **SMB / MTP / freshness**: `freshness.rs`, `smb_index.rs` / `mtp_index.rs`, `smb_watch.rs` / `mtp_watch.rs`.
- **Read path**: `enrichment.rs` (`ReadPool`), `store.rs`, `verifier.rs`, `expected_totals.rs`, `pending_sizes.rs`.
- **Support**: `partial_agg.rs`, `metadata.rs`, `firmlinks.rs`, `watcher.rs`, `memory_watchdog.rs`, `events.rs`,
  `retention.rs`.

IPC in `commands/indexing.rs`; frontend in `src/lib/indexing/`; search in `src/search/`.

## Must-knows (invariants and guardrails)

Every invariant holds PER volume id; DETAILS has the why and mechanism.

- **The `INDEX_REGISTRY` (`Mutex<HashMap<VolumeId, IndexInstance>>`) is the authority** (absent key = disabled, no
  `Disabled` phase). The mutex guards lifecycle ONLY; reads route by volume id through the per-volume `ReadPool`, never
  under it. Enrichment SKIPS when `get_read_pool_for` is `None` (unindexed volume = zero DB work).

Writer + connection + schema discipline (one writer thread per DB, bounded `sync_channel`):

- **`start_indexing` is lock-first**: reserve the registry slot atomically BEFORE building `IndexManager`, else two
  starts for one volume race writer threads on its DB. **`stop_indexing` / `clear_index` drop the guard BEFORE the
  shutdown drain** (else deadlock).
- **Reconciler/event loops hold a READ connection, never a write one** (write-mode pragmas can `SQLITE_BUSY` and silently
  kill live indexing). **`IndexWriter` owns the shared `Arc<AtomicI64>` ID counter** — don't allocate from `MAX(id)`
  (uncommitted inserts → stale → double-assign).
- **Schema invariants.** Register the `platform_case` collation on every connection (not persisted; else the raw
  `sqlite3` CLI fails on `name` — use `index-query`). Don't drop `UNIQUE (parent_id, name_folded)` (guards racing writers,
  a multi-TB ghost-size hazard) nor `name_folded` (keeps the rebuild fast). The scanner uses `INSERT OR IGNORE`, never
  `INSERT OR REPLACE` (which reassigns IDs, orphaning children, slow on a populated DB).
- **Mid-scan partial aggregation has four easy-to-break rules** that otherwise ship wrong sizes — DETAILS § "Key
  decisions".
- **The index is a disposable cache**: schema mismatch / corruption drops + rebuilds; no migrations or user-facing DB
  errors. Gate only `scan_completed_at` writes (absence ⇒ heal to a rescan).
- **Defer `root` auto-start until FDA is decided** (`should_auto_start_indexing`): a scan from `/` stacks TCC popups over
  the FDA modal. FDA gates ONLY `root` — don't route SMB/MTP through it (not TCC-protected).

SMB/MTP indexing (dedicated DETAILS sections — read before touching this area):

- **Gated on a `direct` (smb2) connection; an `os_mount` upgrades first** (`start_indexing_for_smb` refuses with a TYPED
  `SmbIndexGateReason`, never a substring); MTP has no smb2 gate. Scans use `volume_scanner`, not jwalk, with three
  round-trip disciplines (cancel, timeout, `autoreleasepool`).
- **Freshness has ONE transition table (`freshness.rs`); don't branch elsewhere.** No journal ⇒ a persisted index
  loads **Stale** on launch (not a bug); mid-scan disconnect keeps an honest partial + Stale, only user-cancel resets
  to gray.
- **Live watch → index runs with NO pane open** (`apply_smb_change` hooks `notify_directory_changed` before the pane
  early-return; don't remove the hook).
- **Deletes resolve against the INDEX, not a live stat**: delete only a known entry; an unknown name/handle is a no-op
  (a recreate heals via the add). Local FSEvents `item_removed` stat-verifies.
- **Threads + resources.** Memory watchdog: one GLOBAL 16 GB budget (`stop_all_indexing`), not per-volume; start is
  idempotent. The writer thread (and any ObjC/Cocoa-calling thread) must wrap work in `objc2::rc::autoreleasepool` on
  macOS (else multi-GB leaks). Spawn tasks with `tauri::async_runtime::spawn`; `tokio::spawn` panics from the sync
  `setup()` hook (where indexing can start).

Flows, decisions, and gotchas: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
