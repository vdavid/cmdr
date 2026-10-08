# mimalloc v3 against v2 and the system allocator, and why we keep v3 (2026-09-23)

**What this settles:** whether switching Cmdr's global allocator would cut its footprint, and at what cost. **Decision:
keep mimalloc v3 for now.** The system allocator saves a median ~95 MiB at idle, but it's 30–45% slower on search
queries and leaves multi-hundred-MiB peaks for minutes after a big free. mimalloc v2 buys nothing. (Reversed for macOS
once the search penalty was gone and the post-burst settle was measured: `allocator-slack-release-2026-09-27.md`, and
`crates/cmdr-fs/DETAILS.md` § "Which global allocator" for the current split.) The idle gap looked like thread churn,
but pooling the walker's threads left it unchanged (`walker-thread-pool-2026-09-27.md`).

The live-versus-slack method this builds on: `rust-heap-attribution-2026-09-23.md`. Per-round raw numbers:
`allocator-comparison-2026-09-23.csv` (one row per round, condition, and sample point; bytes).

## Setup

- **Binaries**: release (`pnpm tauri build --no-bundle`, thin LTO). `v3` is the shipped build (`libmimalloc-sys` 0.1.49,
  mimalloc 3.3.2). `v2` sets the `v2` feature on `libmimalloc-sys` in `crates/cmdr-fs/Cargo.toml`, which unifies across
  the graph (mimalloc 2.3.2; verified: the binary lacks v3's `page_commit_on_demand`). "System" ran in the v3 binary
  through a runtime switch (`CMDR_EXP_ALLOC`, one relaxed load and a branch per call in every condition).
- **Live bytes**: an allocator-independent counter of requested `Layout` bytes, striped over 64 cache-padded atomics,
  logged every 5 s with the thread count and tokio thread starts and stops.
- **Five conditions, run concurrently each round** (same machine state, fresh launches): A v3; B v2; C system; D v3 with
  tokio's blocking pool capped (keep-alive 300 s, max 128); E system with the same cap.
- **Data**: `sqlite3 .backup` of prod's DBs plus APFS clones of the rest, secrets and the lock excluded; analytics,
  update checks, crash and error reports, and the global shortcut off; each instance on its own data dir and MCP port,
  verified with `lsof` every round.
- **Protocol per round**: launch; sample at 5 and 10 min (idle; CPU is the CPU-time delta between them); rescan the root
  index; three bursts (an MCP search for `*.pdf`, which loads a 6–7 M-entry arena, then navigation to a 200,000- and a
  100,000-entry folder); sample at +5, +10, and +13 min (settled). Heap = tag 100 plus `Malloc *` dirty and swapped.
- **Noise**: load average 5–310 from other agents' work. Round 3's snapshot was past the replay limit, so it did a
  startup full scan instead of a replay (and had almost no walker churn).

## Memory

MiB, median [min–max] over five rounds:

| reading                     | A v3             | B v2             | C system         | D v3 + cap | E system + cap  |
| --------------------------- | ---------------- | ---------------- | ---------------- | ---------- | --------------- |
| idle footprint              | 301 [280–317]    | 285 [253–362]    | 209 [189–269]    | 303        | 229 [188–270]   |
| idle heap ÷ live            | 1.85 [1.55–2.07] | 1.75 [1.69–2.42] | 1.20 [1.15–1.66] | 1.95       | 1.17            |
| post-burst peak footprint   | 926 [875–1380]   | 1009 [917–1470]  | 1436 [824–2237]  | 969        | 1619 [973–2534] |
| settled footprint           | 849 [807–946]    | 893 [757–1040]   | 766 [691–953]    | 901        | 762             |
| settled slack (heap − live) | 266              | 263              | 112              | 251        | 121             |

- **Idle, paired against A per round**: C −70, −121, −11, −102, −106 (median ~−95); B +18, +51, −8, −32, −49; the noise
  control D +24, −15, −25, +21, +14. The system allocator's idle saving is real and repeatable.
- **Settled, paired against A**: C −164, −86, −19, −180, +133, against a noise floor of ±95. Not conclusive.
- **Transients**: after a big free, the system allocator peaks at 1.4–2.5 GiB against 0.9–1.4 for v3, and stays there
  for minutes, because macOS malloc keeps freed pages dirty and returns them lazily. A user would watch Activity Monitor
  sit at 2.5 GB after a search.
- **Idle CPU is identical across allocators** (~15.5% at this point in each run, all of it background indexing work).
- **The settled footprint is mostly live data, in every condition**: ~500–560 MiB still live 13 min after the last MCP
  search. Allocator-independent, and a follow-up of its own (measured before the MCP arena's 30 s drop landed).

## Throughput

Seven interleaved rounds, rotating order, release test binaries; ratio against v3, median [range], below 1 is faster:

- Scan `/Applications` (342,000 entries, scan plus writer flush): v2 0.98, system **0.88** [0.80–1.68].
- List a 200,000-entry folder: v2 1.02, system 0.99 (noise).
- Search arena load (7.2 M entries): v2 0.99, system 0.93.
- Search queries: a rare literal v2 1.15, system **1.45**; "report" v2 1.01, system **1.29**; `*.pdf` v2 1.14, system
  **1.36**; one letter v2 0.96, system 1.10. A focused re-run confirmed 1.3–1.6× for the system allocator.
- Criterion `index_benchmarks`: all medians within 0.93–1.15, noise-dominated on this machine.

A query that matches nothing slows too, so the scan loop probably allocates per entry or per chunk. Unverified; removing
that allocation would also remove the system allocator's search penalty.

## The thread churn, and why it predicts the slack

- **Tokio's blocking pool isn't it.** Tauri's default runtime started only 22–93 threads per run, and capping it
  (conditions D and E) changed nothing, so D and E served as noise controls.
- **The index walkers are.** `crates/cmdr-index/src/indexing/scanner/walker/engine.rs` spawns a watchdog plus
  `num_threads` workers per walk, and `reconcile/reconciler/rescan/mod.rs` spawns a `rescan-subtree` thread per subtree
  reconcile. A diagnostic build logged 7,351 `index-walk`, 459 watchdog, and 328 `rescan-subtree` threads in ~20 min;
  runs reached ~29,000 threads in 40 min.
- **Churn seemed to predict mimalloc's idle slack**: the round with ~55 threads (the startup-scan round) showed a
  system-versus-v3 idle gap of −11 MiB, and the rounds with 700–25,000 threads showed −70 to −121. That was one
  low-churn round, and it didn't hold: with the walker pooled, five paired rounds with a 3–20× churn difference showed
  no slack difference (`walker-thread-pool-2026-09-27.md`).

## What switching would take

- **To the system allocator**: drop `#[global_allocator]` from `apps/desktop/src-tauri/src/main.rs`, the `mimalloc`
  dependency, and the `use mimalloc as _` in `crate_deps.rs`; rewire `query_mimalloc_heap`'s consumers
  (`memory_diagnostics`, the memory watchdog's attribution, `events/index_mapping.rs`) to the malloc zones; and flip the
  `IOAccelerator` trap in `docs/tooling/memory-debugging.md` and `apps/desktop/CLAUDE.md`. Linux would get glibc malloc,
  whose per-thread arenas behave very differently under churn, so it needs its own measurement. Test binaries already
  use the system allocator.
- **To v2**: one feature flag. Not recommended: no measured gain.
- **Capping tokio's pool**: trivial, and measured to do nothing.
