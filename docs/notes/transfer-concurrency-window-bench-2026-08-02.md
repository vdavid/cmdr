# What the transfer concurrency window is worth (2026-08-02)

The M4.3 baseline: wall-clock against the copy driver's concurrency window, swept 1-32 on unchanged production code,
against a real NAS and a Docker Samba container. M4.3 proposed replacing
`min(src.max_concurrent_ops(), dst.max_concurrent_ops(), 32)` on the premise that "this is where the throughput upside
sits". These numbers are what that premise has to survive.

**The headline: on the deciding target it does not.** 74% of the fastest many-small run is spent in a serialized
per-file destination probe that no window width can overlap. The window is worth ~14% at best; the probe is worth up to
~74%.

Keep this note until the pre-check (below) is either fixed or ruled out. It's the before-baseline any re-measurement
compares against, and it records the two shapes' bottlenecks separately, which is the thing a single throughput number
hides.

## Method

`volume_copy_concurrency_bench.rs` (`#[ignore]`d, in
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/`). Its module docs carry the full rationale; the
load-bearing parts for reading these numbers:

- **The window is swept on unchanged production code.** The destination side comes from `set_smb_concurrency` (the real
  `network.smbConcurrency` setting); the source side from a `LocalPosixVolume` wrapper reporting a fixed
  `max_concurrent_ops` of 64. So `min(src, dst, 32)` resolves to exactly the swept value and the driver never knows it's
  being measured.
- **Reps are round-robin, not blocked per window**, with the first full pass discarded as warm-up. A NAS warms up and a
  laptop throttles; blocking all reps of window 1 before any rep of window 32 hands that drift to one end of the curve.
- **Every rep verifies**: one destination listing, every file present, every size exact, no `.cmdr-tmp-` survivors. A
  fast run that dropped a file can't look good.
- **`window = 1` is the SERIAL driver**, not a one-wide concurrent window (`use_concurrent_path` needs
  `concurrency > 1`). It's what a user setting `network.smbConcurrency = 1` gets, so the row is honest, but the 1 -> 2
  step is partly a driver switch rather than window width.

Reproduce:

```sh
cd apps/desktop/src-tauri
SMB2_TEST_NAS_PASSWORD=... CMDR_BENCH_TARGET=nas CMDR_BENCH_REPS=9 \
  CMDR_BENCH_LARGE_COUNT=32 CMDR_BENCH_LARGE_MIB=8 \
  cargo test --release --lib concurrency_bench -- --ignored --nocapture --test-threads=1
```

### Environment

- 2026-08-02. macOS 26.5.2, Apple M3 Max, 16 logical / 16 physical cores, release build.
- **NAS (the deciding target)**: QNAP TS-464 "Naspolya", `192.168.1.111`, share `naspi`, direct `smb2` (not the OS
  mount), wired gigabit. RTT (`ping -c 3`): min/avg/max = 3.372 / 3.729 / 3.959 ms. 9 reps.
- **Docker (corroboration only)**: Samba container on `127.0.0.1:10480`, share `public`. Loopback, so the per-file
  latency a wider window exists to hide barely exists; a Docker curve says nothing about a real network on its own. 5
  reps.
- The machine is shared with other work. Load hits every arm of an interleaved round-robin equally, so the RELATIVE
  comparisons below survive it; **treat every absolute MB/s as indicative**.

## NAS, many-small: 500 x 16 KiB (7.8 MiB), 9 reps

Every file fits one compound `CREATE+WRITE+FLUSH+CLOSE` frame, so the copy is close to pure per-file round trip. This is
a folder of documents, and the shape a wider window should help most.

| window | median  | min     | max     | files/s |
| -----: | ------- | ------- | ------- | ------: |
|      1 | 7.578 s | 7.420 s | 8.314 s |      66 |
|      2 | 6.040 s | 5.085 s | 7.116 s |      83 |
|      4 | 4.700 s | 3.886 s | 5.198 s |     106 |
|      6 | 3.940 s | 3.640 s | 4.270 s |     127 |
|      8 | 3.752 s | 3.398 s | 4.357 s |     133 |
|     10 | 3.522 s | 3.247 s | 3.700 s |     142 |
|     12 | 3.302 s | 3.167 s | 3.630 s |     151 |
|     16 | 3.245 s | 3.069 s | 3.442 s |     154 |
|     24 | 3.278 s | 3.121 s | 3.502 s |     153 |
|     32 | 3.224 s | 3.086 s | 3.402 s |     155 |

**Serial pre-check floor: 2.378 s for 500 files (4.76 ms/file) = 74% of the fastest run.**

Reading it:

- **12 / 16 / 24 / 32 are one number.** 3.302 / 3.245 / 3.278 / 3.224 s, with spreads that overlap almost entirely. By
  the "overlapping spreads means no measurable difference" rule, there's nothing to choose between them.
- **8 -> 16 is 1.16x** (3.752 -> 3.245 s). The spreads touch only at the tails ([3.398-4.357] vs [3.069-3.442]), so it's
  probably real, but it's 14%, not a multiple.
- **4 -> 16 is 1.45x** (4.700 -> 3.245 s), spreads clearly disjoint. This is the row that matters for the defect below.
- Peak in-flight tracked the window at every step (31 at window 32), so the window genuinely filled. It plateaued
  because it ran out of things to win, not because it stopped opening.

## The discriminator: it was never the window

"The window stopped helping" and "the window was never the bottleneck" bend a curve identically, so the curve alone
can't choose between them. The floor measurement can.

`volume_copy.rs`'s concurrent spawn loop awaits `dest_volume.get_metadata(&dest_item_path)` once per top-level source,
**on the driver task, before the file's task is spawned** (the `PreparingNext` phase, the call that was the driver's
last log line in the 2026-07-31 wedge). On SMB that's one round trip per file that no window width can overlap: a batch
of N files carries a hard floor of `N x RTT` however wide the window gets.

`serial_precheck_floor` measures it directly rather than inferring it from the asymptote: the same call, on the same
connection, for the same file count, serialized the same way, against paths that don't exist (what a copy into a fresh
directory actually probes, and the cheap answer, so it if anything understates). It runs outside the driver, so no
production code is instrumented.

At 500 files it's **2.378 s of a 3.224 s best run**. Subtract it and 846 ms is left for all the actual copying. The
curve flattens at 12 because that's where the window gets wide enough to finish files faster than the driver can hand
them out, and the hand-out rate is one round trip per file, serialized.

**The deep-merge path in this same module already avoids exactly this call shape.** Per
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` ("Scan-as-you-merge"), a merge lists the
destination level ONCE, builds a `name -> FileEntry` map, and does no per-child `get_metadata` probes. The top-level
spawn loop is the one place still paying N round trips for what one listing answers.

## NAS, few-large: 32 x 8 MiB (256 MiB), 9 reps

Each file is far past the QNAP's negotiated `max_write` (~1 MiB), so every one takes the staged streaming writer, which
already pipelines up to 32 wire WRITEs _within a single file_. The harness asserts `write_is_single_shot == false` for
this shape, so a "large" file that quietly fit one frame can't benchmark the fast path twice.

| window | median  | min     | max     | MB/s (indicative) |
| -----: | ------- | ------- | ------- | ----------------: |
|      1 | 3.994 s | 3.816 s | 4.341 s |                67 |
|      2 | 3.452 s | 3.215 s | 4.273 s |                78 |
|      4 | 3.097 s | 2.847 s | 3.783 s |                87 |
|      6 | 3.003 s | 2.776 s | 3.396 s |                89 |
|      8 | 2.965 s | 2.645 s | 3.631 s |                91 |
|     10 | 2.875 s | 2.627 s | 3.394 s |                93 |
|     12 | 2.790 s | 2.626 s | 3.187 s |                96 |
|     16 | 2.752 s | 2.528 s | 3.168 s |                98 |
|     24 | 2.811 s | 2.564 s | 3.049 s |                96 |
|     32 | 2.768 s | 2.526 s | 2.868 s |                97 |

Serial pre-check floor: 142.610 ms for 32 files (4.46 ms/file) = 5% of the fastest run.

**This shape is link-bound, not window-bound.** ~97 MB/s of payload is a saturated gigabit link. Everything from window
4 up sits inside everyone else's spread; 8 -> 16 is 2.965 -> 2.752 s with spreads [2.645-3.631] vs [2.528-3.168], which
is no measurable difference. There's no throughput to win here at any window width, and none to lose either.

Note the per-file probe cost is the same ~4.5 ms as in the many-small shape. It's invisible here only because 32 files
of 8 MiB amortize it; it's a per-FILE tax, so it scales with file count, not bytes.

## Docker (corroboration)

Loopback, 5 reps. Included because it isolates what is network latency and what isn't; it doesn't decide anything.

many-small, 500 x 16 KiB:

| window | median  | min      | max     |
| -----: | ------- | -------- | ------- |
|      1 | 5.875 s | 5.076 s  | 7.279 s |
|      2 | 4.346 s | 3.759 s  | 4.836 s |
|      4 | 2.844 s | 2.408 s  | 4.160 s |
|      6 | 2.520 s | 2.065 s  | 4.310 s |
|      8 | 2.119 s | 1.799 s  | 3.097 s |
|     10 | 1.873 s | 1.428 s  | 2.413 s |
|     12 | 1.696 s | 1.551 s  | 2.060 s |
|     16 | 1.540 s | 1.338 s  | 1.987 s |
|     24 | 1.320 s | 1.141 s  | 1.372 s |
|     32 | 1.024 s | 922.2 ms | 1.182 s |

Serial pre-check floor: 246.051 ms for 500 files (492 us/file) = 24% of the fastest run.

few-large, 32 x 16 MiB (512 MiB; 16 MiB because this Samba negotiates an 8 MiB `max_write`, so 8 MiB files are
single-shot here and the harness's shape assertion rejects them):

| window | median   | min      | max     |
| -----: | -------- | -------- | ------- |
|      1 | 2.498 s  | 1.989 s  | 2.592 s |
|      2 | 1.602 s  | 1.227 s  | 1.903 s |
|      4 | 1.110 s  | 1.036 s  | 1.842 s |
|      6 | 1.213 s  | 952.5 ms | 1.912 s |
|      8 | 1.134 s  | 930.2 ms | 1.624 s |
|     10 | 1.066 s  | 835.2 ms | 1.129 s |
|     12 | 975.4 ms | 731.6 ms | 1.547 s |
|     16 | 1.072 s  | 763.3 ms | 1.954 s |
|     24 | 1.099 s  | 823.8 ms | 1.400 s |
|     32 | 976.2 ms | 955.4 ms | 1.300 s |

Serial pre-check floor: 26.182 ms for 32 files (818 us/file) = 3% of the fastest run.

Docker many-small keeps improving all the way to 32 where the NAS plateaus at 12, and the floor share (24% vs 74%) says
why: on loopback the per-file probe costs 492 us instead of 4.76 ms, so it never becomes the ceiling and the extra
window keeps buying parallel work. **This is the reason a loopback curve can't decide M4.3**: it recommends a wider
window for a reason that doesn't exist on a real network. few-large agrees with the NAS: flat from window 4.

## The defect the sweep prices: `network.smbConcurrency` is inert

`LocalPosixVolume::max_concurrent_ops()` is `clamp(available_parallelism() / 2, 4, 16)`. Across the Mac line that's 4 on
an 8-core Air, 5 on a 10-core M4 Pro, 7 on a 14-core M4 Max, 8 on this 16-core M3 Max. `SmbVolume::max_concurrent_ops()`
is the `network.smbConcurrency` setting, default 10, range 1-32.

`min(src, dst, 32)` therefore resolves to the LOCAL side on every Mac Cmdr ships to: a CPU-core heuristic picks the
window for a NETWORK transfer, because `min()` lets the least network-relevant side win. Consequences, priced off the
NAS many-small table:

- **The setting does nothing above 8** (above 4 on an Air). Its own description promises "how many file transfers Cmdr
  runs in parallel on a single SMB connection", range 1-32, default 10. A user who raises it to 16 gets 8. That copy is
  currently false.
- **An 8-core Mac copies at window 4 today: 4.700 s.** At the setting's own default of 10 it would be 3.522 s, **25%
  faster**, spreads disjoint ([3.886-5.198] vs [3.247-3.700]). This is the largest measured user-facing loss in the
  whole sweep, and it isn't a tuning question: it's the setting being overridden by core count.
- **On this 16-core M3 Max the same fix is worth nothing measurable**: window 8 -> 10 is 3.752 -> 3.522 s with spreads
  [3.398-4.357] vs [3.247-3.700], overlapping. Anyone measuring this on a high-core Mac will correctly conclude the
  change does nothing, and be wrong about the machines most users have.

## What this says about M4.3

1. **Replacing the window formula for throughput isn't supported by the evidence.** On the deciding target it's worth
   ~14% on many-small at best and nothing measurable on few-large. That isn't where the upside sits.
2. **Removing the local-side cap on a network transfer IS supported**, but as a defect fix, not a throughput play: it
   makes an advertised setting real and recovers 25% on the low-core Macs that most users have. It needs a signal on the
   `Volume` trait for "this cap is about local CPU, don't let it bound a network peer"; raising
   `LocalPosixVolume::max_concurrent_ops()` outright would also change local-to-local copies, which nothing here
   measured.
3. **The serial per-file destination probe is the real target.** 74% of the best many-small run, 4.76 ms/file at 3.7 ms
   RTT, and the fix shape already exists in this module (one dest listing, `name -> FileEntry` map, the deep-merge
   path). It deserves its own milestone and its own before-and-after against this note, not a bolt-on.
4. **"Let the credit budget decide" has no well-defined file-level value** and isn't a candidate. Credits gate WRITE
   frames connection-wide; the window gates concurrent FILES, and each file's `FileWriter` already pipelines 32 wire
   writes against those same credits. Settled before this sweep; the numbers here don't reopen it.
