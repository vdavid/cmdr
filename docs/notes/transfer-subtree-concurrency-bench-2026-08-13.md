# What the subtree window is worth (2026-08-13)

`merge.rs` walked a directory tree with a plain serial loop, and the concurrent driver fans out only across TOP-LEVEL
sources. Select one folder — what a user actually does — and it is one source, so the copy takes the SERIAL driver and
nothing inside it ever overlapped. `network.smbConcurrency` (advertised 1-32, default 10) did nothing for the commonest
copy there is.

This note is the measurement behind putting one operation-wide window inside that walk
(`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "One window for the whole
operation").

⚠️ **Everything below is Docker Samba on loopback, and loopback systematically understates what this kind of change is
worth AND overstates how wide the window should be.** That is the standing finding of
`transfer-concurrency-window-bench-2026-08-02.md`: a Docker SMB number is a correctness and regression signal, not a
latency signal. Read the SHAPE of these curves, not the multiple, and ❌ don't quote the width they favor.

## Method

`volume/copy_concurrency_bench.rs`, the same `#[ignore]`d harness and the same rules as the 2026-08-02 note: the window
is swept on unchanged production code through the real `network.smbConcurrency` setting, reps are round-robin with the
first full pass discarded as warm-up, and every rep verifies from one destination listing that every file landed at its
exact size with no `.cmdr-tmp-` survivors.

The one thing this run adds is a corpus SHAPE:

- **`many-small (loose files)`** hands the copy N files as N top-level sources. N ≥ 3 with a window > 1 means the
  CONCURRENT driver, so this row measures the top-level window — what 2026-08-02 measured.
- **`many-small (one folder)`** puts the identical N files one level down and hands the copy the FOLDER as its single
  source. One source is under the concurrent path's three-source threshold, so this shape runs on the SERIAL driver at
  **every** window width, and every byte goes through `merge.rs`'s walk.

Same corpus, same connection, same session, one variable: whether the user selected the files or the folder holding
them.

**Why the `window = 1` row IS the before-number for the folder shape.** A width of 1 makes `FileWindow::is_serial()`
true, and a serial window awaits each leaf inline — byte for byte the walk this change replaced. And because one source
never reaches the concurrent driver, nothing else varies down that column. So the folder table is a controlled
before/after inside one run: row 1 is the old behavior at ANY setting, and the rest is what the setting now buys.

Reproduce:

```sh
cd apps/desktop/src-tauri
CMDR_BENCH_TARGET=docker SMB_CONSUMER_GUEST_PORT=11480 CMDR_BENCH_SHAPES=folder-ab \
  CMDR_BENCH_REPS=5 CMDR_BENCH_WINDOWS=1,2,4,8,16,32 \
  cargo test --release --lib concurrency_bench -- --ignored --nocapture --test-threads=1
```

### Environment

- 2026-08-13. macOS 26.5.2, Apple M3 Max, 16 logical cores, release build.
- Docker Samba container on `127.0.0.1:11480`, share `public`, `CMDR_BENCH_DEST=existing`. 5 reps plus a discarded
  warm-up, round-robin over windows.
- **The machine was at load average ~17** with eight agents building. Load hits every arm of an interleaved round-robin
  equally, so the RELATIVE comparisons hold; ❌ treat every absolute MB/s here as indicative only.

## Results: 500 × 16 KiB (7.8 MiB)

**`many-small (loose files)`** — the top-level window, unchanged by this work. Included as the control.

| window |   median |      min |     max | files/s | peak in flight |
| -----: | -------: | -------: | ------: | ------: | -------------: |
|      1 |  1.200 s | 978.9 ms | 2.434 s |     417 |              1 |
|      2 |  1.305 s | 567.0 ms | 2.766 s |     383 |              1 |
|      4 |  1.085 s | 597.9 ms | 1.305 s |     461 |              3 |
|      8 | 919.0 ms | 624.1 ms | 1.416 s |     544 |              7 |
|     16 | 928.5 ms | 508.3 ms | 1.528 s |     539 |             15 |
|     32 | 821.2 ms | 671.4 ms | 2.229 s |     609 |             31 |

Serial pre-check floor: 65.9 ms for 500 files (132 µs/file) = 8% of the fastest run.

**`many-small (one folder)`** — the subtree window. Row 1 is the before-number.

| window |   median |      min |      max | files/s | peak in flight |
| -----: | -------: | -------: | -------: | ------: | -------------: |
|      1 |  1.055 s | 945.6 ms |  1.861 s |     474 |              1 |
|      2 | 619.2 ms | 569.9 ms |  1.043 s |     808 |              3 |
|      4 | 588.4 ms | 516.5 ms |  1.108 s |     850 |              5 |
|      8 | 649.8 ms | 523.7 ms | 770.9 ms |     770 |              9 |
|     16 | 634.7 ms | 548.8 ms | 827.4 ms |     788 |             17 |
|     32 | 535.8 ms | 491.2 ms | 714.5 ms |     933 |             33 |

Serial pre-check floor: 99.6 ms for 500 files (199 µs/file) = 19% of the fastest run.

## Reading it

- **The defect is real and the fix lands.** 1.055 s → 588 ms at window 4 is **1.79×**, spreads ([945.6-1861] vs
  [516.5-1108] ms) disjoint at the medians and overlapping only in the tails. Before this change every row in that table
  would have read like row 1, because the width had nowhere to apply.
- **The window genuinely opens, at every width.** Peak in flight tracks it: 1, 3, 5, 9, 17, 33. It reads `W + 1` because
  the top-level source's own in-flight-table row sits alongside its `W` leaf rows — the leaves get rows of their own
  precisely so a dump can name the file that wedged (`volume/DETAILS.md` § "One window for the whole operation").
- **Most of the win is in the first step, 1 → 2.** 1.055 s → 619 ms is 1.70× of the total 1.97×. Beyond 4 the curve is
  noise on this target: 4 / 8 / 16 are 588 / 650 / 635 ms with spreads that overlap almost entirely.
- **A folder now copies FASTER than the same files selected loose** (588 ms vs 1.085 s at window 4). That is not a
  surprise and not a new win: the merge walker lists each destination level ONCE and matches names in memory, while the
  loose path asks the top level per source. It does mean the two paths are no longer differently-shaped, which was the
  point.
- ❌ **This run does NOT say the window should be wider than 32, or even that 32 is good.** The loopback artifact from
  2026-08-02 is fully present: the probe costs 132-199 µs here against 4.76 ms on a real NAS, so the per-file round trip
  never becomes the ceiling and extra width keeps buying something. A Docker-only sweep always recommends "wider".

## What real hardware said, and why it outranks this

David measured SMB→SMB on real hardware (2026-08-12, ~2.8 MB average files, each concurrency level against its own
never-read directory so page cache couldn't inflate it):

| concurrency | rate    | ms/file |
| ----------: | ------- | ------: |
|           1 | 4 MB/s  |     519 |
|           4 | 10 MB/s |     224 |
|           8 | 10 MB/s |     218 |
|          16 | 9 MB/s  |     328 |
|          24 | 8 MB/s  |     273 |

**The useful window is 4-8 and it degrades past it.** That is the number to design against; the loopback table above
agrees on the first half (nearly all the win is in place by 4) and is silent on the second half by construction, because
degradation from too many concurrent files needs a real transport to appear at all.

**Open, and deliberately not settled here**: the width comes from `copy.rs::transfer_concurrency`, whose SMB side is the
user's `network.smbConcurrency`, default **10** — just past the measured useful window. Reusing that one number is the
right call (a second private width is exactly the `W²` trap this design exists to avoid), but whether the shipped
DEFAULT should drop from 10 toward 6-8 is a product decision on a user-visible setting, not a cleanup. Nothing in this
note changed it.

## What this does NOT show

- **Nothing here ran against a NAS or over WAN.** The 2026-08-02 rule stands: a Docker SMB number is a correctness and
  regression signal only. A NAS re-run of `CMDR_BENCH_SHAPES=folder-ab` is the measurement that would settle both the
  real multiple and the default-width question above.
- **Only the many-small shape was swept in folder form.** A few-large folder is link-bound for the same reason the loose
  few-large shape is, so nothing was expected there and nothing was measured.
- **MTP was not measured and cannot be.** Its `max_concurrent_ops()` of 1 makes the window serial by construction, which
  is asserted rather than timed
  (`merge_window_tests.rs::a_cap_of_one_keeps_a_subtree_strictly_one_operation_at_a_time`).
