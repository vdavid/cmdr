# Re-anchor cost (Spike A)

Spike A of `../specs/later/sealed-subtrees-plan.md`, the gate on M2–M4. Phase C makes a periodic full re-anchor the
primary correctness mechanism for a sealed subtree, and a re-anchor is the same O(children) walk the design exists to
avoid, now on a timer. This note measures that walk and answers whether a cadence exists that is both affordable and
tight enough to keep drift tolerable.

Measured 2026-07-20 with `scripts/reanchor-cost`.

## Verdict: go, with conditions

**Go.** A re-anchor of the worst directory on this machine (1.44M entries) costs **96–181 s wall, 19–29 s CPU, zero
writer messages, and a flat 128 KiB of memory** using `getattrlistbulk`. The verification pass it replaces cost 426 s,
pegged the writer queue at its 20,001 cap, and peaked at 1.01 GB (`sealed-subtrees-plan.md` § "The incident"). So one
anchor is roughly a quarter of one verification pass, with none of the queue or memory pressure, and the row/RAM saving
is permanent rather than per-incident. At a cadence of hours, sealing wins clearly.

Three conditions, each falling out of the numbers below:

1. **Schedule anchors on a cost budget, not a fixed clock.** Per-entry cost is not constant: it is 1.9 µs at 100k
   entries and 80 µs at 1.43M. A single global "anchor every hour" is simultaneously wasteful for small sealed dirs and
   unaffordable for the big one. Derive each dir's cadence from its own last measured walk time (suggestion: cadence ≥
   200 × walk time, so background metadata IO stays under ~0.5% duty).
2. **Split the anchor into a count pass and a byte pass.** `readdir` alone is 5.4–10.8 s at ~1.44M entries (5.4–8.1 s on
   `fetch_temp` itself), 10.6–28× cheaper than reading sizes across the six paired runs, median ~17×, and near-linear.
   Count drift (which `expected_totals` cares about) can therefore be refreshed hourly on even the pathological
   directory for ~0.2–0.3% duty, while the expensive byte pass runs every 6–12 h.
3. **Cap the walk and degrade honestly.** `fetch_temp` is growing at 100–250 entries/min and nothing prunes it, so the
   anchor cost grows with it, superlinearly. Phase C needs a "this walk would exceed its budget, skip it and keep the
   aggregate marked approximate" path, otherwise the treadmill reappears.

**What would have made this a no-go:** an unconditional hourly full byte re-anchor. That is 3.3% wall duty forever on
one directory, 1.4M random metadata reads per pass, and it gets worse every day as the directory grows. If Phase C
cannot be made adaptive, stop after M1.

## The numbers

Warm and cold-ish runs, `-runs 2` (run 1 is the first pass in a fresh process, run 2 is immediately after). Full data in
the appendix; this is µs/entry, low to high across both runs.

| directory                  | entries   | enumerate | lstat       | bulk       | bulk wall |
| -------------------------- | --------- | --------- | ----------- | ---------- | --------- |
| test-data, 100k files      | 100,000   | 1.7–3.5   | 7.7–45.6    | 1.9–2.1    | 0.2 s     |
| control, 100k empty files  | 100,000   | 1.9–5.7   | 7.9–40.0    | 1.7–1.9    | 0.2 s     |
| Chrome cache               | 108,145   | 1.4–4.0   | 4.6–97.5    | 2.0–4.0    | 0.2–0.4 s |
| control, 400k empty files  | 400,000   | 1.4–5.6   | 23.1–30.8   | 6.8–7.4    | 2.7–3.0 s |
| control, 1.43M empty files | 1,430,000 | 4.9–7.5   | 108.9–140.6 | 79.9–82.6  | 114–118 s |
| `fetch_temp` (DriveFS)     | 1,440,209 | 4.5–5.6   | 97.7–184.4  | 81.1–125.7 | 117–181 s |

An earlier isolated sweep of `fetch_temp` alone (nothing else running) got the same shape with less variance: bulk 96.3
s / 97.0 s / 99.1 s over three runs (67–69 µs/entry), lstat 148.0 s and 180.4 s, enumerate 5.4 s twice.

Four things stand out.

**`getattrlistbulk` is a large win at ordinary scale and a modest one at pathological scale.** It is 2–24× faster than
`lstat` up to ~400k entries (and beats `readdir` alone, since it fetches names and sizes in one pass), but only 1.2–1.8×
at 1.44M. That wide low-end range is cache state, not noise: a cold-ish first pass gives 4–24×, a warm second pass 2–4×.
Use it, and do not budget as if it collapses the cost: at the size that matters the walk is IO-bound, and a cheaper
syscall does not remove the reads. It also runs at 16–23% CPU versus 21–28% for `lstat`, so it roughly halves CPU for
the same work.

**The cost is IO wait, not syscalls.** CPU time is 16–23% of wall on the big directories, versus 85–91% on the warm
100k-scale `bulk` runs. Sampling `iostat -d disk0` during a `fetch_temp` bulk walk showed a sustained 9,300–11,000
transfers/s at 44–63 MB/s (verified on macOS 26.5.2, `iostat` sampling, 2026-07-20), which is about one random metadata
read per directory entry.

**Warm equals cold above the cache knee, and that knee is capacity.** At 100k entries the second `lstat` run is 5–6×
faster than the first (21× for the Chrome cache) and its CPU share of wall climbs (41→49%, 51→65%, 27→94%): the metadata
is cached, so what's left is syscall work. At 1.43M the second run is no faster than the first and CPU stays near 20%:
the working set no longer fits, so every pass re-reads it from the SSD. This confirms David's Python observation (150.2
s warm vs 150.9 s cold) and explains it.

**Python was not inflating anything.** David's probe measured 106 µs/entry for `readdir` + `lstat` on `fetch_temp`; the
Go tool measures 98–184 µs/entry for the same work. Per-call interpreter overhead is in the noise next to a ~70 µs
metadata read. Any future probe of this can be written in whatever language is convenient.

## The FileProvider hypothesis is refuted

The suspicion was that `fetch_temp` costs 3–4× more per entry than ordinary directories because it lives inside
`com.google.drivefs.fpext`, so each `lstat` might round-trip to Google's FileProvider extension over XPC. It does not.

- `fetch_temp`, the Chrome cache, and the test-data folder all sit on the same volume: `stat -f %Sd` reports `disk3s5`
  for each, and `mount` lists no FileProvider filesystem. The path is inside an extension's _container_, which is
  ordinary APFS storage, not a synthetic mount.
- The decisive control: 1,430,000 empty files created by `reanchor-cost -generate` in `/private/tmp`, outside any
  container, cost **79.9–82.6 µs/entry** with bulk and **108.9–140.6 µs/entry** with `lstat`. That is the same cost as
  `fetch_temp`.

**The variable is entry count, not location.** A flat directory crosses a cache-capacity knee somewhere between 400k and
1.4M entries on this machine, and past that knee every attribute read becomes a random SSD read. That is a more useful
finding than the XPC one would have been: it means any directory of this size is expensive, wherever it lives, and it is
what makes the "cadence must scale with the directory" condition non-negotiable.

One caveat on the control: it was created sequentially and never churned, so its catalog layout is as favourable as it
gets, while `fetch_temp` has been churning for months. The control being _as slow as_ the real directory therefore
bounds the effect of churn-induced fragmentation at roughly nothing on this evidence, but it does not prove
fragmentation is free.

## Does an affordable cadence keep drift tolerable?

Drift in a sealed subtree comes from uncredited deletes: creates are credited through `PropagateDeltaById`, deletes
resolve against the index and no-op for collapsed files, so the aggregate inflates monotonically. Two axes, and they are
independent:

- **Bytes.** Drift per interval ≈ delete rate × mean collapsed file size × cadence. For `fetch_temp` this is **exactly
  zero**: every file is 0 bytes, measured (logical and physical sums are 0 across 1.44M entries). More generally, the
  directories whose walks are expensive are expensive _because_ they hold millions of entries, and on this machine that
  always means tiny files.
- **Counts.** Drift is the raw uncredited delete count, at least the ~30 renames/min DriveFS produces. That is ~43k/day
  against a 1.44M count, so ~3%/day, and it is fixed by the cheap `readdir` pass, not the expensive attribute pass.

So the cadence question splits cleanly, and both halves land inside budget:

| cadence | 1.44M byte pass (~120 s) | 1.44M count pass (~7 s) |
| ------- | ------------------------ | ----------------------- |
| 15 min  | 13% duty                 | 0.8% duty               |
| 1 h     | 3.3% duty                | 0.19% duty              |
| 6 h     | 0.56% duty               | 0.03% duty              |
| 24 h    | 0.14% duty               | 0.008% duty             |

For the motivating directory, an hourly count pass plus a 6–12 h byte pass costs under 0.8% duty and leaves byte drift
at zero. For a 400k-entry sealed dir the byte pass is 3 s, so it can run every few minutes and hold drift under 1%. For
a churny `target/` (tens of GB, a few hundred thousand entries) the same applies, and the seal-size rule keeps rows for
files ≥64 KB anyway, so those deletes are credited exactly.

**The one quadrant this does not cover: ≥1M entries _and_ large collapsed files.** There the 10,000-row cap binds, most
bytes are collapsed, byte drift is large, and the byte pass costs minutes, so no affordable cadence keeps the aggregate
honest. No such directory exists on this machine. Phase C should handle it by showing the approximate state rather than
by walking harder, and M1's counter is the instrument that would tell us whether it exists elsewhere.

## Method and environment

- Machine: Apple M3 Max, 64 GiB RAM, macOS 26.5.2 (build 25F84), Go 1.25.12, darwin/arm64.
- Filesystem: APFS on `/dev/disk3s5` (`/System/Volumes/Data`), 926 GiB, 93% full. All targets, including the controls,
  are on this one volume. The 93% fill is worth remembering before generalizing these numbers to an empty disk.
- The tool times three shapes per directory: `enumerate` (`readdir` only), `lstat` (`readdir` plus one `lstat` per
  entry, the naive re-anchor), and `bulk` (`getattrlistbulk`, names and sizes in batches). Sizes are summed for
  non-directory children only, which is what an aggregate needs.
- Every run cross-checks the methods against each other and reports entry- and byte-count spread. On `fetch_temp` the
  spread is real churn (0.18% across a 20-minute sweep); on static directories it is zero, which is what validates the
  bulk attribute parsing.
- "Cold-ish" means the first pass in a fresh process. Purging the unified buffer cache needs `sudo`, so a true cold
  cache was not available. This matters less than it sounds: above the cache knee, warm and cold are the same number,
  and below it the warm number is the one a repeated timer would actually pay.
- Variance between sweeps is significant when the machine is busy (`fetch_temp` bulk: 96–181 s across sweeps). Ranges in
  this note span every run taken, so treat the high end as the realistic bound, not an outlier.

### Reproducing

```sh
# The three real targets, two runs each.
go run ./scripts/reanchor-cost -runs 2 \
  ~/Library/Containers/com.google.drivefs.fpext/Data/tmp/domain-temp-gdrive-*/fetch_temp \
  ~/Library/Caches/Google/Chrome/Default/Cache/Cache_Data \
  "$HOME/projects-git/vdavid/cmdr/_ignored/test-data/folder with 100000 files"

# The synthetic controls (outside any container; ~3.5 min to build the big one).
go run ./scripts/reanchor-cost -generate 1430000 /private/tmp/control-empty-1430k
go run ./scripts/reanchor-cost -runs 2 /private/tmp/control-empty-1430k
rm -rf /private/tmp/control-empty-1430k
```

Build the controls outside the home folder: 1.4M files under `~` would be indexed by a running Cmdr, which both pollutes
the measurement and hands a dev instance a 1.4M-file directory to reconcile.

## Appendix: full run table

One sweep of all six directories, `-runs 2 -md`, 2026-07-20. Wall times here are noisier than the isolated sweeps
because the six targets ran back to back and evicted each other's cached metadata.

| directory            | method    | run | wall    | cpu    | cpu % | entries   | dirs | logical bytes | physical bytes | µs/entry |
| -------------------- | --------- | --- | ------- | ------ | ----- | --------- | ---- | ------------- | -------------- | -------- |
| control-empty-100k   | bulk      | 1   | 0.17s   | 0.17s  | 97%   | 100 000   | 0    | 0             | 0              | 1.7      |
| control-empty-100k   | bulk      | 2   | 0.19s   | 0.17s  | 91%   | 100 000   | 0    | 0             | 0              | 1.9      |
| control-empty-100k   | enumerate | 1   | 0.57s   | 0.15s  | 27%   | 100 000   | 0    | 0             | 0              | 5.7      |
| control-empty-100k   | enumerate | 2   | 0.19s   | 0.11s  | 57%   | 100 000   | 0    | 0             | 0              | 1.9      |
| control-empty-100k   | lstat     | 1   | 4.00s   | 1.64s  | 41%   | 100 000   | 0    | 0             | 0              | 40.0     |
| control-empty-100k   | lstat     | 2   | 0.79s   | 0.39s  | 49%   | 100 000   | 0    | 0             | 0              | 7.9      |
| control-empty-400k   | bulk      | 1   | 2.97s   | 1.82s  | 61%   | 400 000   | 0    | 0             | 0              | 7.4      |
| control-empty-400k   | bulk      | 2   | 2.70s   | 1.38s  | 51%   | 400 000   | 0    | 0             | 0              | 6.8      |
| control-empty-400k   | enumerate | 1   | 2.23s   | 0.69s  | 31%   | 400 000   | 0    | 0             | 0              | 5.6      |
| control-empty-400k   | enumerate | 2   | 0.56s   | 0.50s  | 88%   | 400 000   | 0    | 0             | 0              | 1.4      |
| control-empty-400k   | lstat     | 1   | 12.32s  | 6.55s  | 53%   | 400 000   | 0    | 0             | 0              | 30.8     |
| control-empty-400k   | lstat     | 2   | 9.23s   | 5.84s  | 63%   | 400 000   | 0    | 0             | 0              | 23.1     |
| control-empty-1430k  | bulk      | 1   | 118.09s | 26.98s | 23%   | 1 430 000 | 0    | 0             | 0              | 82.6     |
| control-empty-1430k  | bulk      | 2   | 114.25s | 25.66s | 22%   | 1 430 000 | 0    | 0             | 0              | 79.9     |
| control-empty-1430k  | enumerate | 1   | 7.06s   | 2.57s  | 36%   | 1 430 000 | 0    | 0             | 0              | 4.9      |
| control-empty-1430k  | enumerate | 2   | 10.76s  | 4.05s  | 38%   | 1 430 000 | 0    | 0             | 0              | 7.5      |
| control-empty-1430k  | lstat     | 1   | 155.67s | 35.93s | 23%   | 1 430 000 | 0    | 0             | 0              | 108.9    |
| control-empty-1430k  | lstat     | 2   | 201.11s | 41.77s | 21%   | 1 430 000 | 0    | 0             | 0              | 140.6    |
| Chrome Cache_Data    | bulk      | 1   | 0.44s   | 0.23s  | 52%   | 108 145   | 1    | 1 215 338 479 | 1 427 664 896  | 4.0      |
| Chrome Cache_Data    | bulk      | 2   | 0.22s   | 0.19s  | 85%   | 108 145   | 1    | 1 215 338 479 | 1 427 664 896  | 2.0      |
| Chrome Cache_Data    | enumerate | 1   | 0.43s   | 0.12s  | 29%   | 108 141   | 1    | 0             | 0              | 4.0      |
| Chrome Cache_Data    | enumerate | 2   | 0.15s   | 0.11s  | 76%   | 108 145   | 1    | 0             | 0              | 1.4      |
| Chrome Cache_Data    | lstat     | 1   | 10.55s  | 2.88s  | 27%   | 108 145   | 1    | 1 215 331 437 | 1 427 660 800  | 97.5     |
| Chrome Cache_Data    | lstat     | 2   | 0.50s   | 0.47s  | 94%   | 108 145   | 1    | 1 215 338 479 | 1 427 664 896  | 4.6      |
| fetch_temp           | bulk      | 1   | 116.78s | 19.09s | 16%   | 1 440 209 | 0    | 0             | 0              | 81.1     |
| fetch_temp           | bulk      | 2   | 181.20s | 28.54s | 16%   | 1 441 566 | 0    | 0             | 0              | 125.7    |
| fetch_temp           | enumerate | 1   | 8.09s   | 2.72s  | 34%   | 1 439 016 | 0    | 0             | 0              | 5.6      |
| fetch_temp           | enumerate | 2   | 6.48s   | 2.82s  | 44%   | 1 440 404 | 0    | 0             | 0              | 4.5      |
| fetch_temp           | lstat     | 1   | 140.66s | 38.88s | 28%   | 1 439 518 | 0    | 0             | 0              | 97.7     |
| fetch_temp           | lstat     | 2   | 265.69s | 57.97s | 22%   | 1 440 858 | 0    | 0             | 0              | 184.4    |
| test-data 100k files | bulk      | 1   | 0.21s   | 0.21s  | 98%   | 100 000   | 0    | 7 869 642     | 409 600 000    | 2.1      |
| test-data 100k files | bulk      | 2   | 0.19s   | 0.17s  | 91%   | 100 000   | 0    | 7 869 642     | 409 600 000    | 1.9      |
| test-data 100k files | enumerate | 1   | 0.35s   | 0.10s  | 30%   | 100 000   | 0    | 0             | 0              | 3.5      |
| test-data 100k files | enumerate | 2   | 0.17s   | 0.13s  | 76%   | 100 000   | 0    | 0             | 0              | 1.7      |
| test-data 100k files | lstat     | 1   | 4.56s   | 2.30s  | 51%   | 100 000   | 0    | 7 869 642     | 409 600 000    | 45.6     |
| test-data 100k files | lstat     | 2   | 0.77s   | 0.50s  | 65%   | 100 000   | 0    | 7 869 642     | 409 600 000    | 7.7      |

### Side finding: `fetch_temp` is growing fast

The plan recorded 1,138,220 children from the index on 2026-07-19 22:44. A direct `readdir` on 2026-07-20 saw 1,429,917,
and within the ~2 h of measurement that followed it reached 1,441,566: roughly 100–250 net new entries per minute, and
nothing prunes it. Whatever we build has to assume this directory keeps getting more expensive, which is the reasoning
behind condition 3 above.
