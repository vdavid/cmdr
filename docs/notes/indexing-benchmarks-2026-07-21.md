# Indexing benchmarks, 2026-07-21

Measured on David's M3 MacBook Pro, boot volume `/`, machine otherwise idle (load < 5, no builds, no other Cmdr instance
active). Dev build at commit `0ecdf4f44`, launched with `CMDR_LOG_RAM_USE=1` and `CMDR_RECONCILE_LATENCY_SPIKE=1`,
against an empty `cmdr-dev-bench` data dir.

Two independent memory sources: the app's own `phys_footprint` (logged per line) and an external `ps` sampler at 2 s
intervals (`scripts/cpu-rss-sampler`: `go run ./scripts/cpu-rss-sampler`, which matches the app's executable, not any
process with the repo path in its arguments). They disagree by design; see "Memory" below.

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

## NAS scan (2026-07-22): multi-connection scan pool, after measurement

The full fresh SMB index scan of David's QNAP NAS (`/Volumes/naspi`, share `naspi` on `192.168.1.111`) with the
multi-connection scan pool (`smb/scan_pool.rs`, `SCAN_POOL_SIZE = 4`) plus the M2 batched writer (periodic
transactions, ~2 s commit window).

**Read this caveat before the numbers: warm ARC, not a cold-vs-cold comparison.** The cold baseline below had already
walked all 2.64M entries earlier the same morning, which warmed the QNAP's ZFS ARC (~41-47 GB resident). This "after"
run is therefore the **warm/mixed** regime on the NAS side, not cold. The 2.25x wall-time improvement conflates the
pool's speedup with NAS-side ARC warmth; it is **not** an isolated measurement of the pool, and **not** cold-vs-cold. The
isolated cold directory-listing ceiling (~3.8x over 4 connections at flat disk latency) lives in the smb2 probe:
`~/projects-git/vdavid/smb2/docs/benchmark-findings.md` §§ dated 2026-07-22. A true cold-vs-cold full-scan comparison
needs the ARC evicted first (reboot the NAS), which this run deliberately did not do (read-only, hands-off).

| run                                   | conditions          | wall        | entries       | dirs       | dirs/s   | entries/s   |
| ------------------------------------- | ------------------- | ----------- | ------------- | ---------- | -------- | ----------- |
| baseline, before the pool             | cold ARC            | 1,816.9 s   | 2,642,879     | 71,231     | ~39      | ~1,454      |
| **with the pool + M2 batched writer** | **warm/mixed ARC**  | **806.6 s** | **2,643,000** | **71,233** | **88.3** | **3,277**   |

2.25x faster wall time in the warm regime (`1,816.9 / 806.6`); dirs/s and entries/s scale the same (2.26x, 2.25x).
Totals match the baseline within live-NAS drift (entries +121, dirs +2). Indexed size `5,531,872,108,675` bytes
(5.53 TB), identical to the baseline.

**Pool engaged and ran clean.** Log confirmed `smb scan pool: opened 4/4 extra connections`, a `fresh scan (truncate)`
(not a reconcile), and a clean `scan pool: closed` at the end. Across the whole 806.6 s scan there were **zero** pool
health events: no dead members, no member reconnects, no `ConnectionLost` / `SessionExpired`, no per-listing timeouts,
and no main-session fallback. So the pool's failure paths (sibling-retry, member reconnect) were never exercised by this
run; they stay covered by the unit and Docker-SMB integration tests, not this measurement.

**M2 writer kept up.** Periodic transactions committed 387 times over the scan (~one per 2.08 s, matching the ~2 s
window). The writer's `queue_depth` stayed low for most of the run and peaked at **1,699** during listing bursts, well
short of the pre-M2 backlog that spiked into the thousands in the June bench. It never became the hard bottleneck (the
scan finished at 3,277 entries/s).

**Memory.** Peak `phys_footprint` (the honest number, logged with `CMDR_LOG_RAM_USE=1`) was **1,023 MB** (~1.0 GB)
during the NAS scan, versus the baseline's 1.14 GB peak. The external `ps` sampler (`scripts/cpu-rss-sampler`, watching
the dev app PID) reported RSS peak 1,908.8 MB (`ps` over-counts vs `phys_footprint` by design, see "Memory" above) and
CPU peak 284.7% of one core (~2.85 cores, the pool's parallel listing); its averages are diluted because the sampler
window overran the scan into post-completion idle, so quote the peaks, not the averages.

**Honesty bound held.** The root's `recursiveSizeComplete` is `false`, so the app shows the NAS total as a lower bound
(`≥ 5.53 TB`): the intentionally-skipped NAS system dirs `@Recycle` and `@Recently-Snapshot` each report size 0 with
`recursiveSizeComplete: false` (unknown, not claimed as exact), exactly as the `min_subtree_epoch` design requires.

**How it was connected (for repeating this).** `/Volumes/naspi` is not OS-mounted by default, and a guest direct smb2
connection fails on this share (`STATUS_LOGON_FAILURE`; the share requires real credentials). The working recipe:
`mount_network_share` with `username = "david"` and the password (from the login-keychain internet-password item keyed
by server `192.168.1.111` + protocol `smb`, which is where Finder stores it, not under an `smb://…` account string),
which registers a direct smb2 `SmbVolume`, then `enable_drive_index` on volume id `smb-192-168-1-111-445-naspi`. A fresh
dev data dir gives fresh-scan (truncate) behavior for free.

# Run 2, same day, after the three fixes, on a BUSY machine

Re-run at commit `b3ca98045` (after the ledger-debt fix, the latency-based cost budget, and the progress-based walker
watchdog), deliberately under real working load (load average 12-24) because that is the representative case. Wall-clock
here is NOT comparable to the idle numbers above; the counts are.

## The walker fix holds, and recovered more than predicted

| run                   | conditions | entries       | dirs        | dirs abandoned          |
| --------------------- | ---------- | ------------- | ----------- | ----------------------- |
| run 1, before the fix | idle       | 6,001,637     | 625,371     | **5** (656,476 entries) |
| run 2, after the fix  | load ~11   | 6,989,382     | 656,543     | **0**                   |
| run 2b, after the fix | load 24    | **7,273,543** | **678,319** | **0**                   |

Zero abandoned directories and zero timeouts under load 24, which is stronger evidence than the idle run: contention is
precisely what used to push those five reads past 15 s. The recovery (+987,745 entries over the pre-fix run) EXCEEDS the
656,476 the five named directories held, because directories BELOW an abandoned one were never queued and so never
appeared in any log. The true loss was always larger than the log could show.

## The cost budget is still size-biased, and this is the third iteration on it

Under load the budget fired FIVE times (twice when idle). Slow-read counts against subtree size:

| subtree                                   |    dirs | slow reads | fraction  |
| ----------------------------------------- | ------: | ---------: | --------- |
| `.cache/github-copilot/project-context`   |      62 |         14 | **22.6%** |
| `CloudStorage/MacDroid-googlePixel9ProXL` |      91 |         18 | **19.8%** |
| `Library/pnpm/store`                      |   6,669 |         62 | 0.93%     |
| `projects-git/vdavid/cmdr`                | 105,441 |        101 | **0.10%** |
| `CommandLineTools/SDKs/MacOSX13.3.sdk`    |   6,828 |          4 | **0.06%** |

Two orders of magnitude of separation. Genuinely pathological subtrees run ~20% slow reads; healthy ones are at or below
1% and get refused anyway.

**Diagnosis: the budget is an absolute total (10 s) while the OPPORTUNITY to accumulate scales with subtree size.** A
105,441-directory repo reaches 10 s of slow time eventually however healthy it is. The SDK is the same failure from the
other end: four unlucky reads condemning 6,828 directories, having barely cleared the 3-read sample floor.

Load is NOT the main cause, contrary to the first reading of this data: ordinary reads averaged 1.04 ms against 0.56 ms
idle, only 1.86x slower.

**Acted on: the rule is now a FRACTION, not a total.** A subtree is refused when more than 5% of the reads charged to it
were pathological, over at least 10 slow reads, having wasted more than 5 s. That is size-invariant by construction (the
phone trips at 19.8% at any size; the repo at 0.10% never does), and it gets all five of the subtrees above right with
~4× of margin on each side. The floors are measured too: the SDK's four slow reads are what proved a floor of three too
low. Full rationale, arithmetic per subtree, and the residual prefix/latch risk:
`apps/desktop/src-tauri/src/indexing/DETAILS.md` § "The reconcile cost budget".

## Separate finding: eight reads take exactly 5.000 s

All sandboxed app containers, all within 7 ms of a round five seconds:

```
5007.2  Library/Containers/com.apple.AMSUIPaymentViewService/Data
5006.6  Library/Containers/com.apple.Sound-Settings.SoundIntents/Data
5004.4  Library/Group Containers/.SiriTodayViewExtension/Library
5002.8  Library/Containers/com.google.drivefs.finderhelper.findersync/Data
5002.3  Library/Containers/com.apple.Music.MusicStorageExtension/Data/Library
5001.6  Library/Group Containers/G7HH3F8CAK.com.getdropbox.dropbox.sync/ssa_events
5001.4  Library/Containers/com.apple.sharing.ShareSheetUI/Data/Library/Application Scripts
5001.3  Library/Containers/com.apple.lighthouse.BiomeSELFIngestor/Data/Library/Preferences
```

Eight reads landing that tightly on a round number looks like a timeout rather than disk latency, and it cost 40 s of
the walk. **But two obvious explanations are already refuted, so treat this as unexplained rather than diagnosed:**

- NOT a permission/TCC stall. The run's own log records `FDA probe: read OK on …/Safari/History.db → FDA`, so the build
  had Full Disk Access, and only four `Operation not permitted` lines appear in the whole run (none of them these
  paths).
- NOT inherent to the directories. Timed from a shell the next day, the same four paths read in **74-190 ms** (`ls -f`,
  warm).
- NOT a Cmdr constant: there is no 5-second timeout anywhere in `indexing/` or `file_system/` (`busy_timeout = 5000` is
  SQLite's, on a different path entirely).

What remains: something transient during that window. `spotlightknowledged.updater` was at 84% CPU and `fileproviderd`
at 33% while the scan ran, so contention on those specific containers is plausible but unproven. Before chasing this,
reproduce it: if a later run shows no 5 s cluster, it was environmental and there is nothing to fix.

### Investigation, 2026-07-22: did not reproduce; best theory is a TCC container-authorization stall

Probed the eight exact paths with a throwaway Go+cgo tool (`scratchpad/probe/main.go`) that hits each one through BOTH
code paths Cmdr uses: `getattrlistbulk(2)` in a loop (mirroring `scanner/walker/bulk_read.rs` attr-for-attr) and
`readdir` + `lstat` per entry (mirroring the serial reconcile). Three rounds each.

**The 5 s cluster did not reproduce, but not for a reason that clears it.** From an agent shell without Full Disk
Access, all eight paths fail with `EPERM` (`Operation not permitted`) in 3-8 ms through both readers, because the dirs
are `0700`-owned by the user yet TCC-gated as sandbox app-data containers: the owner's own Unix permission is not enough,
and without FDA the kernel refuses at the gate before any read can be slow. The Cmdr run had FDA (its log recorded the
probe), so its opens were ALLOWED, and then stalled. The probe cannot reproduce the stall from an unprivileged shell, and
`sudo` is not available non-interactively here. So "did not reproduce" here is "could not exercise the real path", not
evidence the stall is gone.

**What the probe and path analysis DID establish:**

- **All eight are TCC-protected sandbox containers** (`~/Library/Containers/*/Data`, `~/Library/Group Containers/*`), and
  that is the ONLY property common to all eight. This is the discriminating fact the earlier refutations missed.
- **File Provider is NOT the common thread.** `fileproviderctl dump` shows the registered File-Provider domains belong to
  the actual cloud MOUNT points (the `CloudStorage` roots for Dropbox, Google Drive, iCloud, MacDroid), not to these
  sandbox helper containers. Only two of the eight relate to a cloud app at all (`com.google.drivefs.finderhelper.findersync/Data`,
  `…getdropbox.dropbox.sync/ssa_events`), and neither is a dataless file-provider root. So `fileproviderd` contention
  does not explain the other six (`AMSUIPaymentViewService`, `Sound-Settings`, `Music`, `ShareSheetUI`, `Biome`, Siri),
  which are plain Apple sandbox containers.
- **Refutation #1 was too strong.** FDA does not remove TCC from the access; it makes TCC ALLOW instead of DENY. The
  allow decision for a sandbox container still round-trips the authorization/container machinery. Having FDA and seeing
  few `EPERM` lines rules out a TCC *denial*, not a TCC *stall*.
- **Refutation #3 re-verified.** No 5-second timeout exists in the walker or open path. The only `from_secs(5)` in
  `local_reconcile/` is `cost_budget.rs`'s `MIN_SLOW_TIME_WASTED` (accumulated waste before refusing a subtree), a
  different mechanism; the rest are heartbeats and SMB/MTP reconnect timers.

**Best-supported theory (still partly speculative on the exact constant):** the 5.000 s is an authorization/XPC reply
timeout in the TCC or `containermanagerd` path that mediates every open of a sandbox container, even for an FDA process.
Under the recorded daemon storm (`spotlightknowledged.updater` 84%, `fileproviderd` 33%, both leaning on the same
XPC/authorization machinery), that mediation hit its ~5 s reply timeout, after which the kernel proceeded and the read
completed fast (the dirs are tiny, hence the same paths reading in 74-190 ms warm and uncontended). Non-container
directories never pay this because they are not TCC-mediated. What is PROVEN: the eight are exactly the TCC-mediated
category, and the stall needs FDA to appear at all. What is SPECULATIVE: that the 5 s is specifically a TCC/`containermanagerd`
reply timeout, and which daemon owns the constant. I could not manufacture the FDA-plus-contention combination to confirm
it.

**Impact is bounded and self-healing, which drives the recommendation.** A 5 s open that then returns entries is NOT
abandoned: the walker's stall watchdog fires at 15 s (`stall_timeout`), and the read delivers its (small) contents well
inside that, so there is no data loss and no skipped subtree, only wall time. The whole cluster cost ~40 s, one-off, only
on an FDA scan while a Spotlight/File-Provider storm runs.

**Recommendation: treat as environmental and close it; do not add a walker defense.** Reasons: (1) it needs FDA plus a
specific daemon storm to appear, so it is rare and not a steady-state cost; (2) when it does fire the reads still succeed
and return correct data, so nothing is lost; (3) any targeted fix (for example a shorter per-open timeout that abandons
the read) would risk dropping legitimately-slow-but-correct reads for a stall that heals itself in 5 s, trading data
completeness for ~40 s of one-off wall time; and (4) the existing 15 s progress watchdog and the fraction-based cost
budget already bound the worst case. If it recurs on a future FDA scan, the way to confirm the theory is to run the probe
WITH FDA (add the built binary to System Settings › Privacy › Full Disk Access, or run it from an FDA-granted Terminal)
while `spotlightknowledged.updater` is hot; the prediction is ~100 ms per path when idle and a ~5 s cluster only under
the storm. Absent that recurrence, there is nothing to fix.

## Reconcile, run 2 (load 12-24, not comparable to the 476.9 s idle figure)

601,357 reads, 817.2 s wall, 639.3 s in reads, 735.9 dirs/s, **zero timeouts**. Ordinary reads 1.04 ms mean, synthetic
2.55 ms (down from 4.84 ms). Peak `phys_footprint` 1.39 GB during the fresh scan of 7.27M entries, versus 903 MB for
6.0M — worth watching, but it tracks the extra million entries rather than growing on its own.

# Swap-scan re-measurement, 2026-07-22

The one question this run answers: with the progress-based walker watchdog now reading big directories to completion
(so the fresh scan is honest-but-slower rather than fast-but-10%-short), is the fresh scan still enough faster than the
reconcile to justify swap-scan? Method: forget the index, run a clean fresh scan, then immediately `rescan` (serial
reconcile in place) on the same machine, back to back, sharing one external background. Dev build at commit `9e0249fc1`,
`CMDR_LOG_RAM_USE=1` and `CMDR_RECONCILE_LATENCY_SPIKE=1`, empty `cmdr-dev-swap-scan-remeasure` data dir. Full method as
above.

**Load conditions.** Not idle: `spotlightknowledged.updater` ~90% of a core (the disk scan itself provokes it),
IntelliJ ~50%, `fileproviderd` ~30%, no competing Rust builds, no other Cmdr scanning `/`. External floor held near load
10–14 across both runs. The load AVERAGES still differ by operation (fresh scan self-drives to ~20–30, reconcile stays
~9–15) because the parallel walker saturates cores while the serial reconcile uses ~one; that gap is intrinsic to the
two designs and is exactly what production would see, not a measurement artifact.

## The two walls and the ratio

| operation                 | wall       | entries / reads | dirs        | abandoned | timeouts | peak `phys_footprint` |
| ------------------------- | ---------- | --------------- | ----------- | --------- | -------- | --------------------- |
| **Fresh scan** (walker)   | **107.2 s**| 6,975,013       | 658,178     | **0**     | 76¹      | 1.08 GB               |
| **Reconcile** (serial BFS)| **897.4 s**| 657,830 reads   | 658,046 re-listed | n/a | **1**²   | 1.08 GB               |

**Ratio: 897.4 / 107.2 = 8.4x.** The reconcile is 8.4x slower than the fresh parallel scan under matched conditions.

¹ All 76 fresh-scan timeouts are per-subdir `os error 60` skips on the `MacDroid-googlePixel9ProXL` File Provider mount
(the Android phone), environmental and identical across runs. Walker subtree abandonment (the 15 s stall watchdog and
the 32-consecutive-failure backstop) fired **zero** times on the clean run.

² The one reconcile timeout is `Library/Caches/cmdr/WebKit/NetworkCache/…/Resource` at 15,005 ms, abandoned at the 15 s
read timeout.

## The completeness objection is resolved

This is the point of the re-measurement. The fresh scan's old 52.7 s (run 1) bought its speed by abandoning five big
directories at the 15 s elapsed timeout, so the following reconcile had to add `+656,352` entries (~10%). With the
progress watchdog, the clean fresh scan indexed 6,975,013 entries with **zero abandoned subtrees**, and the reconcile
right after it reported **`+1848 -1 ~217534`**: only 1,848 genuinely new entries (0.03% of the index), not the old
656,352. The `~217,534` modifications are cargo/build churn in `target/` trees during the ~16 minutes the two runs
spanned, not directories the scan missed. So the fresh scan is now honest AND complete, and it is still 8.4x faster than
the reconcile.

## Reconcile read split (load ~9–15)

`reads=657,830 wall_s=897.4 in_read_s=698.8 dirs_per_s=733.0 timeouts=1`. Split: **ordinary 649,432 reads at 1.04 ms
mean (672.7 s total)**, **synthetic (File Provider) 8,398 reads at 3.11 ms mean (26.2 s total, 0 timeouts)**. Under load
the ordinary reads dominate the wall (672.7 s of 698.8 s), matching run 2; the synthetic reads are no longer the
bottleneck they were idle (854.9 s in the original run 1). Cost budget fired on 2 subtrees, leaving 217 dirs
undescended. Latency histogram: `<1ms=577,264 1-5ms=66,063 5-10ms=6,686 10-50ms=6,450 50-100ms=913 100-500ms=400
500ms-1s=32 1-5s=19 5-15s=2 >=15s=1`.

## What contaminated the first attempt, and how it was handled

The very first fresh scan of this session ran during the tail of another session's Rust build (load 30–35) and took
138.2 s. That number is not comparable to the reconcile, so it was discarded: the index was forgotten and a clean fresh
scan re-run (107.2 s) once the build finished and the floor settled to ~10–14. The reconcile was then run back to back
with that clean scan. Even the contaminated 138.2 s scan against the 897.4 s reconcile is a 6.5x gap, so the conclusion
is robust to the load the fresh scan runs under: the ratio sits in the 6.5–8.4x range and stays wide.
