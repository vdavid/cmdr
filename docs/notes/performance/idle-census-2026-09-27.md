# Where `main` stands against the targets, and the next levers (2026-09-27)

**What this settles:** how a release build of `main` (`e9fd713ad`) measures against the two targets in `README.md` on a
clone of prod's data, whether the ~500 MiB still live after search bursts (follow-up #2) survives the MCP arena's 30 s
drop, and which cuts are left that the hub doesn't already list.

- **RAM, idle**: met, barely. 263–273 MiB footprint at 10–37 min with indexing on, 194–196 MiB with indexing off.
- **RAM, after search bursts**: missed. The live data is gone within 30 s, but the footprint settles 25–115 MiB above
  its pre-burst level (316–387 MiB) and stays there past 20 min. What's left is allocator slack, and it isn't
  purge-tunable.
- **CPU, idle**: missed on this machine. Main process 2.2% of a core with indexing on, all of the excess driven by ~160
  FS events/s from sibling agents' builds; 0.85–1.3% with indexing off.

## Setup

- **Binary**: `pnpm tauri build --no-bundle` of `e9fd713ad` (release, thin LTO, universal), run bare with its own
  `CMDR_INSTANCE_ID`, `CMDR_DATA_DIR`, `CMDR_MCP_PORT`, and `CMDR_SECRET_STORE=file`. Isolation checked with `lsof`
  before measuring: zero prod files open, one listener on 127.0.0.1, its own WebKit dir.
- **Data**: an APFS clone (`cp -c`) of prod's data dir while prod was down, `-wal` and `-shm` included, secrets, the
  crash report, and the lock excluded. 5.2 M-row root index. Analytics, update checks, crash and error reports, and the
  global shortcut off; media indexing off and proactive mode off, as in prod. Every run starts from a fresh clone.
- **Runs**: A (indexing on; idle, then the burst protocol), B (indexing off; idle only), C and D (indexing on, launched
  together; D with `MIMALLOC_PURGE_DELAY=0`; the burst protocol). A replayed 1.9 M journal events in 85 s; C and D found
  a 13 M-event gap and rescanned (356 s).
- **Machine**: David's dev Mac, the heavy case (`README.md` § "Methodology rules"). Load average 3–138 during the runs,
  with sibling agents building in four worktrees and mounting E2E SMB fixtures.

## A better CPU instrument: per-thread time with names

This recipe now ships as the `cpu_diagnostics` MCP tool (`docs/tooling/mcp.md`, `crates/cmdr-fs/src/process_cpu/`).

`ps -M` has no thread names. `proc_pidinfo(pid, PROC_PIDLISTTHREADS)` plus `PROC_PIDTHREADINFO` per handle returns each
thread's name and user and system time in nanoseconds, for any process of the same user, no root needed (verified on
macOS 27.0, a 25-line C tool against Dock and against Cmdr, 2026-09-27). Snapshot it every minute and diff:

- Match threads on handle plus name; a thread that exists only in the later snapshot counts in full.
- **Process delta minus the live threads' deltas** is the CPU spent by threads that exited inside the window. It's how
  the churn threads (walkers, `rescan-subtree`, tokio's blocking pool) show up at all.
- `PROC_PIDTASKINFO` adds context switches and syscall counts for the whole task, a usable wakeup rate where `top`'s
  `IDLEW` isn't.

`sample` stayed as misleading as the hub says: a parked tokio worker showed 1,630 of 44,730 samples at a non-syscall PC
inside `_pthread_cond_wait`, and 1,798 at `__gettimeofday` under a timed wait. Neither is work.

## Idle CPU

Run A, 17:09–17:36 (26.5 min, from 8 min after launch), % of one core, verified on release `e9fd713ad`, per-thread
`proc_pidinfo` deltas, 2026-09-27:

- Main process **2.22%** (three sub-windows: 2.64, 2.30, 1.81). WebContent 0.73%, GPU helper 0.31%.
- `tokio-rt-worker` ×24: 0.97. `index-writer`: 0.56. `notify-rs` fsevents + debouncer loops ×3: 0.21. Exited threads:
  0.22. `main`: 0.07. An unnamed thread: 0.06. `JavaScriptCore libpas scavenger`: 0.06. `mDNS_daemon`: 0.05.
  `agent-wake-loop`, `importance-writer`, and the rest: under 0.02 together.
- The load behind it: ~300,000 live FS events in 31 min (~160/s) and 91,000 writer messages in the window, 31
  removal-storm subtree rescans, nearly all under siblings' `target/` dirs and Chrome's and Firefox's caches.

Run B, indexing off, 18:10–18:32 (22 min):

- Main process **1.16%** (0.85 in the first half, 1.33 in the second). WebContent 0.11%, GPU helper 0.03%.
- `tokio-rt-worker` 0.76 (four core workers carry it, ~87% of it system time, at 89–104 context switches a second for
  the whole process). `notify-rs` 0.12. Exited threads 0.14. `mDNS_daemon` 0.08. JSC scavenger 0.03. `main` 0.03.
- The pane on `~` (hidden files shown, as in prod) re-listed 68 times in 23 min as agents rewrote dotfiles, and an E2E
  run's SMB fixture mounted and unmounted `/Volumes/café` once.

**Indexing-driven share: ~1.1–1.4% of the 2.2%** (A minus B), of which the writer is 0.56 and the rest is the live event
loop and reconciler on tokio workers. On this machine it's load from sibling agents' build output and browser caches.
Build output stays indexed by decision (issue #236, closed as not planned).

## Idle memory

MiB, verified on release `e9fd713ad`, `memory_diagnostics` with `rustHeapCensus`, 2026-09-27:

- **Run A, indexing on**: t = 10 min: footprint 263, heap 206, live 103, slack 102. t = 20: 267, 211, 103, 107. t = 30:
  266, 209, 105, 104. The SQLite slab is 64 of the live bytes, with 23 read connections open.
- **Run B, indexing off**: t = 20: footprint 194, heap 146, live 80, slack 65. t = 30: 196, 147, 81, 66.
- **`memory_diagnostics` shows 55 MiB of `IOSurface` (tag 88) in the main process in both runs**, but none of it is in
  the footprint above: it's WebKit's layer backing, owned and paid for by WebContent
  (`main-process-iosurface-2026-09-27.md`). The window was visible, 2,230 × 1,380 pt on a 2× display.

## Follow-up #2: after search bursts

Protocol from `allocator-comparison-2026-09-23.md`: (A only) a root rescan, then three bursts back to back, each an MCP
search for `*.pdf` (5.8 M-entry arena), a navigation to the 200,000-entry folder, back to `~`, the 100,000-entry folder,
and back. Census at intervals after the last burst.

**The live data is gone.** The arena drops 30 s after the last agent call (`Search idle timeout reached` in the log),
and live bytes fall back to the pre-burst level right there:

- A: live 105 before, 477 at the last burst, **98 at +30 s**, 99–108 from +1 to +20 min.
- C: 89 before, 456, 92 at +1 min, 107–108 from +5 to +20 min.
- D: 91 before, 459, 95 at +1 min, 122–125 from +5 to +20 min.

**What stays is slack**, footprint in MiB:

- A: 273 before; 1,055 at the last burst; 412 at +1 min; 371–397 from +2 to +20 min. Slack 205–250 against 110 before.
- C: 292 before; 1,011; 356 at +1 min; 316–364 from +2 to +20 min (337 at +20). Slack 151–211 against 155.
- D (`MIMALLOC_PURGE_DELAY=0`): 282 before; 803; 418 at +1 min; 352–383 from +2 to +20 min. Slack 169–265 against 131.
  **Immediate purging doesn't help once things settle**; it only lowered the burst peak.

Why it's slack and where it sits:

- The heap grows from 6 to 22 mimalloc arenas of 128 MiB during the bursts, and they stay mapped (normal). A minute
  after the drop, the new arenas still hold ~160 MiB dirty against 35 before; over the next eight minutes that memory
  moves from dirty to swapped (A: tag-100 dirty 262 → 88, swapped 87 → 227) instead of being released. So it's committed
  and not purged, and memory pressure compresses it where a purge would have freed it.
- `blockSpaceBytes` ends within 35 MiB of its pre-burst level (A 135 → 135–146, C 103 → 112–137, D 106 → 121–157) while
  slack grows by 50–140, so most of the retained memory sits outside any page's blocks: free arena slices mimalloc still
  holds committed. With purging immediate (D) it's the same, so it isn't the purge delay. Which state those slices are
  in stays open.
- The three bursts loaded the arena **twice**: the first search's live walk wrote rows behind the arena (nine abandoned
  locations), so the next search reloaded all 5.8 M entries (`reloading 'root's arena, a walk wrote rows behind it`,
  `search/CLAUDE.md`'s one exception). Two arenas plus a merge peak in flight together is the likely reason the heap
  grew to 22 arenas (inferred from the log, not isolated).

## The levers

Ranked by expected payoff over effort. Measured parts are marked; the rest is estimated. Not repeated here: the walker
pool (#1), per-entry search allocations (#3), mDNS gating (#5), and indexing build output (#6).

1. **The space poller asks macOS for the expensive free-space figure every 2 s.** `volumes/nsurl.rs::get_volume_space`
   reads `NSURLVolumeAvailableCapacityForImportantUsageKey`, which goes through CacheDelete and IOKit: **6.5 ms of CPU a
   call, 5.9 of it system time, against 0.04 ms for `NSURLVolumeAvailableCapacityKey` and under 0.001 ms for `statfs`**
   (measured, 50 calls each on `/`, Swift against Foundation, this machine, 2026-09-27). The boot volume is polled every
   2 s for the low-space check whether or not a pane shows it, so that's at least 3.3 ms/s, **~0.33% of a core
   in-process**, plus work in the `deleted` daemon; each other local volume on screen adds as much. It fits run B, where
   tokio's time was mostly system time. Cut: poll the cheap key every 2 s and refresh the important-usage figure only
   when the cheap one moves past the readout's threshold or every few minutes, keeping the Finder-matching number for
   the readout and copy validation. Size: small (`nsurl.rs`, `space_poller/`). Estimate: −0.3 to −0.6% of a core. A
   tradeoff only in how fresh the purgeable part of the readout is.
2. **The search arena reloads in full after every walk that wrote rows**, which is what turns a burst into two arenas, a
   merge peak, and 16 extra 128 MiB arenas that leave slack behind. Cut: apply what the walk wrote to the warm arena (an
   overlay of new rows plus tombstones, both small) instead of reloading 5.8 M entries, or at least drop the stale arena
   before the reload's merge so the two don't overlap. Size: medium; the coverage model in `search/execute/coverage.rs`
   decides what the overlay must honor. Estimate: burst peak −300 to −400 MiB; the settled slack likely shrinks with it
   (unmeasured). The mapped arena (#114) removes the merge peak too, but not the reload.
3. **`recompute_min_subtree_epoch` reads every child of each ancestor to find the child dirs.** Its query filters
   `c.is_directory = 1`, but the only index on `parent_id` is `idx_parent_name_folded`, so SQLite seeks the table once
   per child, dirs or not. On the cloned index: **101 ms for a 91,706-entry folder (Chrome's cache), 81 ms for 68,590, 7
   ms for 18,745**; with `CREATE INDEX … ON entries(parent_id) WHERE is_directory = 1` the same queries take **17–19
   µs** (the plan becomes a covering-index search). The index builds in 0.5 s over 5.2 M rows and adds 6 MB on disk
   (measured with `sqlite3 .timer`, warm cache, 2026-09-27). It was the biggest item on the writer in one 60 s sample
   during churn (about half of the writer's running samples), and the same filter serves
   `recompute_recursive_has_symlinks`, `read/coverage.rs`, and `entries.rs`'s child-dir listings. Size: small, but it's
   a schema change (`store/CLAUDE.md`: bump `SCHEMA_VERSION`, which rescans every user once;
   `CREATE INDEX IF NOT EXISTS` on open would add it without one, David's call). Estimate: up to half the writer's CPU
   under tree-shape churn (writer 0.56% here), and shorter writer backlogs after removal storms. The single-window share
   is a sample, so treat it as a direction.
4. **Settled, no cut: the 55 MiB of `IOSurface` in the main process isn't in its footprint.** It's WebKit's layer
   backing, charged to WebContent, where a window this size costs ~49 MiB against a bare `WKWebView`'s 120
   (`main-process-iosurface-2026-09-27.md`).
5. **WebContent costs 0.73% with indexing on against 0.11% with it off**, with a pane on `~`: index-driven size updates
   reaching the frontend. Above the 0.36% median in `webcontent-idle-fixes-2026-09-23.md`, which ran with less churn.
   Worth a look from the frontend side (coalescing size updates for rows whose readout doesn't change). Estimate: up to
   −0.5% of a core in WebContent on a churning machine.
6. **Small**: every subtree reconcile calls `VolumeWork::drive_is_listed`, which runs `getfsstat` through
   `volumes/mounts.rs::has_mount_identity`. One call is 4.5 µs (14 mounts), but it showed ~470 samples in the churn
   window, so it may be contending on the mount-list lock. The boot volume can't be unlisted, so root could skip it.

## Still open

- Which state mimalloc's retained post-burst slices are in, given that immediate purging doesn't release them.
- Where tokio's system time goes with indexing off beyond the space poll (0.76–1.07% against the ~0.33% the poll
  explains).
