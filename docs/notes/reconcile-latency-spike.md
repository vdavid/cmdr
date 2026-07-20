# Reconcile per-directory read latency spike

Where the time goes in the serial local reconcile walk, measured rather than reasoned about. The instrumentation is
`indexing/local_reconcile/latency_probe.rs`, off unless `CMDR_RECONCILE_LATENCY_SPIKE` is set.

The question that prompted it: a reconcile rescan of the boot volume once ran 2h03m and never finished, while a fresh
parallel scan of the same volume takes minutes. The hypothesis under test was that per-directory read latency,
multiplied by ~600,000 directories and paid serially, is the whole story (10 ms average alone would be 1.7 hours).

## Verdict

**The hypothesis did not hold.** Serial per-directory latency is not uniform, and it isn't 10 ms. The measured
distribution is extremely skewed: 93% of directory reads cost under 1 ms, and the walk's cost concentrates in a tiny
synthetic minority. A full reconcile of `/` completed in **21m 49s**, not two hours.

Two facts frame everything below:

- **92.3% of the walk's wall time is spent inside `read_dir` + `lstat`** (1,208.6 s of 1,309.0 s). The DB diff, the
  writer, and the queue account for the other 8%. So the walk really is read-bound; that part of the hypothesis holds.
- **1.7% of the reads take 65% of the read time.** 10,064 reads under a synthetic (File Provider) root cost 854.9 s,
  while 583,372 reads on real local disk cost 353.8 s at a 0.61 ms mean.

So the fix direction is "don't pay for synthetic filesystems in a serial walk", not "shave the per-directory constant".

## Collect

Run the app with the spike on, from a worktree whose index has a COMPLETED prior scan (`meta.scan_completed_at` set),
then force a rescan so it takes the reconcile route rather than truncate + fresh scan:

```sh
CMDR_RECONCILE_LATENCY_SPIKE=1 pnpm dev --worktree <slug>
# wait for `Scan: complete` and for meta.scan_completed_at to appear, then:
CMDR_INSTANCE_ID=dev-<slug> ./scripts/mcp-call.sh indexing '{"action":"rescan","volumeId":"root"}'
```

Confirm the route with `local scan: reconcile rescan` in the log; a `reconcile_latency_spike_enabled` line confirms the
env gate took. Rollups land every 30 s on target `indexing::reconcile_latency`, plus a `final` one when the walk ends.

Knobs: `CMDR_RECONCILE_LATENCY_SPIKE_PERIOD_S` (default 30), `CMDR_RECONCILE_LATENCY_SPIKE_TOP_N` (default 30).

**Gotcha that cost a run**: editing any `.rs` file while `pnpm dev` is up rebuilds and restarts the app, killing the
scan in flight. Finish the code before you start collecting.

## The run

Measured on 2026-07-20, Darwin 25.5.0, boot volume `/`, 6.0M entries / 593,435 directories. A fresh parallel scan of the
same volume in the same session took **68.1 s**; the reconcile walk took **1,309.0 s**, 19× slower.

Final rollup, quoted verbatim from
`~/Library/Application Support/com.veszelovszki.cmdr-dev-reconcile-timing/logs/cmdr.log`:

```
17:51:48.840 INFO indexing::reconcile_latency  reconcile_latency final reads=593436 wall_s=1309.0 in_read_s=1208.6 dirs_per_s=453.4 timeouts=1
17:51:48.840 INFO indexing::reconcile_latency  reconcile_latency final hist <1ms=552254 1-5ms=32706 5-10ms=3431 10-50ms=3159 50-100ms=212 100-500ms=177 500ms-1s=1477 1-5s=16 5-15s=3 >=15s=1
17:51:48.840 INFO indexing::reconcile_latency  reconcile_latency final split synthetic_reads=10064 synthetic_mean_ms=84.94 synthetic_total_s=854.9 synthetic_timeouts=1 real_reads=583372 real_mean_ms=0.61 real_total_s=353.8 real_timeouts=0
```

Histogram as shares of 593,436 reads: `<1ms` 93.06%, `1-5ms` 5.51%, `5-10ms` 0.58%, `10-50ms` 0.53%, `50-100ms` 0.04%,
`100-500ms` 0.03%, `500ms-1s` 0.25%, `1-5s` 16 reads, `5-15s` 3 reads, `>=15s` 1 read.

That `500ms-1s` bucket is the interesting one: 1,477 reads, so at least 739 s of cost by itself (61% of all read time),
and almost all of it is one mount (below).

### The stall, in the periodic rollups

The walk's throughput collapsed for eight minutes and then recovered completely. Cumulative `reads` per 30 s rollup:

- `wall_s=30.0` → 20,204 reads (673 dirs/s), the shallow warm-cache start.
- `wall_s=90.7` → 35,135. Fifty-five reads in that window.
- `wall_s=120.9` → 35,188. Fifty-three reads.
- ... eight minutes of ~55 reads per 30 s, ~535 ms each, every one of them inside the phone mount ...
- `wall_s=514.1` → 35,916 (69.9 dirs/s cumulative, the floor).
- `wall_s=544.5` → 72,030. It left the mount and read 36,000 directories in one 30 s window.
- `wall_s=1309.0` → 593,436, finishing at 453 dirs/s cumulative.

So ~836 directories cost ~454 s (7.6 minutes), about 35% of the entire walk.

### Top slow directories

Slowest 30, from the same `final` rollup (ms, path):

1. 15,005.8 — `~/Library/Containers/com.google.drivefs.fpext/Data/tmp/domain-temp-gdrive-.../fetch_temp` (the one
   `LOCAL_LIST_TIMEOUT` hit)
2. 10,969.2 — `~/projects-git/vdavid/cmdr/_ignored/test-data/folder with 200000 files`
3. 6,145.1 — `~/projects-git/vdavid/cmdr/_ignored/test-data/folder with 100000 files`
4. 5,774.9 — `~/Library/Caches/Google/Chrome/Default/Cache/Cache_Data`
5. 4,780.1 — `~/Library/Caches/Firefox/Profiles/*/cache2/entries`
6. 4,355.1 — `~/Library/CloudStorage/MacDroid-googlePixel9ProXL/proc/1069/task/1148/attr`
7. 4,193.1 — `~/Library/Caches/cmdr/WebKit/NetworkCache/Version 17/Records/*/Resource`
8. 3,085.5 — `~/Library/CloudStorage/MacDroid-googlePixel9ProXL/proc/1069/task/1148/ns`
9. 2,912.2 — `~/projects-git/vdavid/cmdr/target/debug/deps`
10. 1,987.6 — `~/Library/CloudStorage/MacDroid-googlePixel9ProXL/proc/1069/task/1141/ns`

Ranks 11–30 are the same three families: more `MacDroid-.../proc/**` (15 of the 30 overall), more Rust
`target/debug/deps` (three more repos), and a few genuinely huge local directories (`/opt/homebrew/share/man/man3`,
`/private/tmp`, a bun cache dir). The single-read outliers at the top are big-directory cost (200,000 entries is 200,000
`lstat` calls); the aggregate cost is the phone mount.

## What this actually says

1. **A mounted Android phone is being indexed through `~/Library/CloudStorage`, including its `/proc`.** MacDroid
   exposes the device as a File Provider, so the boot-volume walk descends into the phone's kernel pseudo-filesystem:
   thousands of `proc/<pid>/task/<tid>/{fd,ns,attr,net,map_files}` directories, each ~500 ms over XPC, all of it
   worthless index content that churns every time the phone runs. This is the dominant cost in the walk and it's a
   correctness problem before it's a performance problem.
2. **The File Provider timeout is real but rare here**: exactly one read hit the 15 s cap (DriveFS `fetch_temp`). A
   worse provider state (Drive offline, phone busy) would multiply that, and each one is 15 s paid serially. The 2h03m
   run this spike was chasing is most plausibly that state, not this one; **this run did not reproduce it**, and nothing
   here proves what the 2h03m run was doing.
3. **Ordinary local directories are not the problem.** 0.61 ms mean, 93% under 1 ms. Multiplying that by 593k gives ~6
   minutes of unavoidable serial read time. Parallelizing the walk would cut that, but it's the smaller half of today's
   number and none of the pathological half.

Fix directions, in the order the data supports them: exclude synthetic/File-Provider roots (or at least device-mount
pseudo-filesystems) from the boot-volume walk; then treat a per-directory read budget as a first-class limit rather than
only a 15 s hang guard; parallelism is third.
