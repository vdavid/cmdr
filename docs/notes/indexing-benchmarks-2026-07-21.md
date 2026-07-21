# Indexing benchmarks, 2026-07-21

Measured on David's M3 MacBook Pro, boot volume `/`, machine otherwise idle (load < 5, no builds, no other Cmdr instance
active). Dev build at commit `0ecdf4f44`, launched with `CMDR_LOG_RAM_USE=1` and `CMDR_RECONCILE_LATENCY_SPIKE=1`,
against an empty `cmdr-dev-bench` data dir.

Two independent memory sources: the app's own `phys_footprint` (logged per line) and an external `ps` sampler at 2 s
intervals. They disagree by design; see "Memory" below.

## Fresh scan (truncate + parallel guarded walker)

| run                                                | entries       | dirs        | wall       |
| -------------------------------------------------- | ------------- | ----------- | ---------- |
| 2026-07-20 baseline, before the `proc` exclusion   | 6,033,360     | 593,422     | 68.1 s     |
| 2026-07-20, after the exclusion, machine contended | 6,112,752     | 626,045     | 74.2 s     |
| **2026-07-21, quiet machine**                      | **6,001,637** | **625,371** | **52.7 s** |

841 GB of physical bytes indexed. Roughly 23% faster than the baseline while covering ~32,000 MORE directories: before
the `proc` exclusion the walker hit its 32-consecutive-failure backstop inside the MacDroid phone's `/proc` and pruned
legitimate neighbouring subtrees as collateral.

## Reconcile (serial BFS, in place)

| run                 | reads       | wall        | dirs/s      | timeouts |
| ------------------- | ----------- | ----------- | ----------- | -------- |
| 2026-07-20 baseline | 593,436     | 1,309.0 s   | 453.4       | 1        |
| **2026-07-21**      | **587,553** | **476.9 s** | **1,232.0** | **0**    |

**2.75x faster.** Read time fell from 1,208.6 s to 367.5 s. The whole improvement is concentrated where the analysis
predicted:

- synthetic (File Provider) reads: 10,064 at 84.94 ms mean, 854.9 s total → **8,446 at 4.84 ms mean, 40.9 s total**
- ordinary reads: 583,372 at 0.61 ms → 579,107 at 0.56 ms (unchanged, as expected)

Latency histogram: 552,911 reads under 1 ms, 12 in the 1-5 s band, 2 in 5-15 s, **zero at or past the 15 s timeout**
(the baseline had one).

## The cost budget fired twice, and one was wrong

- `~/Library/CloudStorage/MacDroid-googlePixel9ProXL` — 30.1 s. **Correct.** An Android phone over a File Provider
  mount, genuinely slow per read.
- `~/projects-git/vdavid/cmdr` — 30.0 s. **False positive.** Slow because it is BIG (cargo `target/` across worktrees,
  `node_modules`, and the 200k/100k/50k/20k test fixtures), not because it is pathological. Every read inside it is
  fast.

10,177 directories were left undescended across the two.

**Acted on.** The budget now scores read LATENCY against the entry count a read returned, never cumulative time, and
charges only the slow reads; see `apps/desktop/src-tauri/src/indexing/DETAILS.md` § "The reconcile cost budget". The
analysis that led there:

**The budget's metric is wrong.** Cumulative read time cannot distinguish "expensive per read" from "lots of cheap
reads", so it penalises whichever subtree is largest. The measured pathology was always "1.7% of directories consume 71%
of read time", which is a statement about PER-READ LATENCY. A mean- latency (or cost-per-entry) threshold would refuse
the phone and leave a large healthy repo alone. Anchor depth is not the fix: raising the budget just moves which large
directory gets refused.

Data safety held: the whole reconcile issued 4 deletions total, and the skipped subtrees kept their rows and epochs, as
the unit tests require.

## The fresh parallel scan is ~10% incomplete

The reconcile reported `+656,352 -4 ~197,003`. Those additions are not new files. The fresh scan had **abandoned five
directories at `LOCAL_LIST_TIMEOUT` (15 s)** and skipped their subtrees:

```
.../Library/Caches/Firefox/Profiles/…/cache2/entries
.../Library/Caches/Google/Chrome/Default/Cache/Cache_Data
.../Library/Caches/cmdr/WebKit/NetworkCache/…/Resource
.../cmdr/_ignored/test-data/folder with 100000 files
.../cmdr/_ignored/test-data/folder with 200000 files
```

The serial reconcile read all five without trouble (10.8 s, 7.2 s, 4.9 s, 3.9 s, 1.8 s), because serial reads on an idle
machine are far cheaper than parallel ones under rayon contention. Index rows went from 6,001,637 after the scan to
**6,663,048** after the reconcile.

**Acted on.** The walker's guard now measures STALLED PROGRESS instead of elapsed time: a read publishes each batch it
delivers, and it is abandoned only when it delivers nothing for 15 s (or trickles past a per-entry allowance). These
five directories are read to completion; a disconnected mount is still abandoned in 15 s. See
`apps/desktop/src-tauri/src/indexing/DETAILS.md` § "The walker's progress timeout".

**This matters for the swap-scan plan** (`docs/notes/swap-scan-feasibility.md`): replacing the reconcile with a parallel
build would have made every rescan ~10% incomplete on this machine, and the missing 10% was exactly the big directories
whose sizes users are most likely to look up: the 52.7 s figure bought its speed partly by giving up. The progress
timeout removes that objection, at the cost of the scan now spending however long those directories honestly take. Any
swap-scan design should re-measure the fresh-scan wall time before comparing it with the reconcile's.

## CPU and memory

Reconcile, 234 samples over 7m57s:

- CPU: 42.7% average of one core, 191.1% peak (the walk is serial; peaks are the writer and aggregator)
- RSS: 1,301.9 MB average, 1,320.2 MB peak, and it does NOT fall after the walk
- `phys_footprint`: 332-777 MB during, 371 MB at the end

Fresh scan: `phys_footprint` 29 MB → 903 MB peak, 678 MB at completion.

**On the discrepancy:** RSS counts mapped SQLite pages that macOS can reclaim under pressure; `phys_footprint` is the
accounting that reflects real memory pressure. `phys_footprint` is the honest number to quote, and it settles back down
while RSS does not. Not a leak, but worth knowing that `ps`-style monitoring will report Cmdr as a 1.3 GB process.

## NAS scan: not measured

Blocked on credentials. `/Volumes/naspi` is mounted by Finder as GUEST (`acct = "No user account"` in the Keychain
entry), and indexing an SMB volume requires a direct smb2 connection, which needs real credentials.
`network::smb_upgrade` looked for `smb://naspolya/naspi`, `smb://naspolya`, `smb://192.168.1.111/naspi`, and
`smb://192.168.1.111`, and found none.

Worth investigating separately: Cmdr's Keychain lookup does not find the entry Finder created for the same server,
because Finder stores an internet-password item keyed by server + protocol rather than the `smb://…` account strings
Cmdr searches for. Whether a guest direct connection is possible at all is a separate question.
