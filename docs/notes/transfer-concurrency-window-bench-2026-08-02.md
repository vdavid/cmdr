# What the transfer concurrency window is worth (2026-08-02)

The M4.3 baseline: wall-clock against the copy driver's concurrency window, swept 1-32 on unchanged production code,
against a real NAS and a Docker Samba container. M4.3 proposed replacing
`min(src.max_concurrent_ops(), dst.max_concurrent_ops(), 32)` on the premise that "this is where the throughput upside
sits". These numbers are what that premise has to survive.

**The headline: on the deciding target it does not.** 74% of the fastest many-small run is spent in a serialized
per-file destination probe that no window width can overlap. The window is worth ~14% at best; the probe is worth up to
~74%.

**Both fixes then landed the same day, and the last section of this note has the after-numbers.** The projection was
0.85-1.0 s for the 500-file shape; the measurement came in at 915 ms. Everything between here and there is the
before-baseline, kept as written.

⚠️ **Don't quote "2-3x faster many-small copies" from this note.** The probe skip fires ONLY when the copy creates the
destination folder itself, and the default F5 copy targets the other pane's existing folder, which keeps every probe and
is unchanged. See "How much of the 74% this recovers, and when".

Keep this note as long as the driver has a per-file destination probe at all. It's the before-baseline any
re-measurement compares against, it records the two shapes' bottlenecks separately (the thing a single throughput number
hides), and it's the evidence behind two guardrails in
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § Key decisions.

**Everything here sweeps the TOP-LEVEL window only**, because that was the only window there was: every shape below
hands the copy N loose files as N sources. A DIRECTORY source got no concurrency at all until 2026-08-13 — see
`transfer-subtree-concurrency-bench-2026-08-13.md`, which adds a one-folder shape to this same harness and measures it.

## Method

`volume/copy_concurrency_bench.rs` (`#[ignore]`d, in
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

`volume/copy.rs`'s concurrent spawn loop awaits `dest_volume.get_metadata(&dest_item_path)` once per top-level source,
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

## The two floors, side by side

The same measurement on the two targets:

| target                  |      RTT | probe / file |   floor | best run | floor as % of best |
| ----------------------- | -------: | -----------: | ------: | -------: | -----------------: |
| Docker Samba (loopback) |  <0.1 ms |       492 us |  246 ms |  1.024 s |            **24%** |
| QNAP over gigabit       | 3.729 ms |      4.76 ms | 2.378 s |  3.224 s |            **74%** |

**That divergence is itself a finding, and it's a trap for anyone who benchmarks this project on Docker alone.** Both
terms scale with latency, but not together: the serial probe pays full RTT per file with nothing to hide behind, while
the copy work is spread across the window. So loopback systematically understates a per-file round trip and makes the
window look like the whole story. A Docker-only sweep here recommends "widen the window, it's still climbing at 32". The
NAS says the window stops mattering at 12 and three quarters of the time is somewhere else entirely. Same code, same
harness, opposite conclusions.

Rule of thumb for the next person: **a Docker SMB number is a correctness and regression signal, not a latency signal.**
Anything whose cost is "one round trip per item" needs a real network before it means anything.

## Is 32 still the right ceiling?

Yes, leave it. The NAS plateaus at 12 on both shapes, so the 32 clamp is nowhere near binding on the deciding target.
Docker keeps climbing to 32, but that's the loopback artifact above: its probe is ~10x cheaper, so the ceiling never
appears. Raising the clamp would need a target that still improves past 32, and nothing here is one.

## What this says about M4.3

Three outcomes were on the table: take both changes, take only the pre-check fix, or take neither. The answer is
**neither of the window changes as a throughput play; the local-cap fix on defect grounds only; the pre-check as its own
proposal.**

1. **The window formula is not worth changing for speed. Don't take it.** On the deciding target it's ~14% on many-small
   (8 -> 16, and the spreads touch) and nothing measurable on few-large. Set against a v1.0 release and a change to a
   shared `Volume` trait, that doesn't clear the bar on its own. Said plainly: **if the local-cap defect below didn't
   exist, my recommendation would be to leave `min(src, dst, 32)` exactly as it is.**
2. **The local-side cap IS worth fixing, as a defect.** `min()` lets `LocalPosixVolume`'s `clamp(cores/2, 4, 16)` pick
   the window for a network transfer, so `network.smbConcurrency` does nothing above 4-8 on any Mac we ship to while its
   own description promises 1-32. Worth 25% on an 8-core Mac (window 4 -> 10, spreads disjoint) and nothing measurable
   on a 16-core one. It needs a `Volume` signal for "this cap is about local CPU, don't let it bound a network peer";
   raising `LocalPosixVolume::max_concurrent_ops()` outright would also change local-to-local copies, which nothing here
   measured. **Take it for the honest setting, and take the 25% as a bonus, not as the reason.**
3. **The pre-check is the real prize. Proposal below; not implemented, and out of M4.3 as specified.**
4. **"Let the credit budget decide" has no well-defined file-level value** and isn't a candidate. Credits gate WRITE
   frames connection-wide; the window gates concurrent FILES, and each file's `FileWriter` already pipelines 32 wire
   writes against those same credits. Settled before this sweep; the numbers here don't reopen it.

## Proposal: stop paying N round trips for what one listing answers

**Not implemented.** This is scoped for a decision, not started.

### The win, with the arithmetic

At window 32 the best NAS run is 3.224 s: 2.378 s of serialized probe plus 846 ms of everything else. Today's real
window (8) gives 3.752 s. If the probe cost goes to roughly zero, the projected floor-free run is **~0.85-1.0 s, call it
3.5-4x faster than today** on the 500-file shape.

**Treat that as a projection, not a promise.** It is arrived at by subtraction, and subtraction assumes the residual 846
ms stays the residual. Remove the serial gate and the next bottleneck (wire, server, task scheduling) becomes visible,
and it may well sit above 850 ms. What is _measured_ is the 2.378 s: that much serialized latency is real and currently
un-overlappable. What is projected is what's left after it goes.

The win also scales with file count and RTT, and does nothing for bytes: it's `N x RTT`. Over Tailscale at ~50 ms RTT,
those same 500 files carry a 25 s floor. Over gigabit LAN, 2.4 s. For an 8-file copy, ~40 ms and not worth a line of
code.

### Where the change goes

`volume/copy.rs`, the concurrent spawn loop (the `dest_volume.get_metadata(&dest_item_path).await` at the
`PreparingNext` phase). Three shapes, cheapest first:

- **(a) Skip probes that are provably misses.** The driver _already receives_ the destination's conflicting names as
  `config.pre_known_conflicts`, populated from the FE's `scan_for_conflicts`. Any source not in that set cannot
  conflict, so its probe is a guaranteed miss and can be skipped outright. In the measured case (a copy into a fresh
  directory, zero conflicts) that removes all 500 round trips. Smallest diff by far. **Blocker to check first**:
  `pre_known_conflicts` is a `#[serde(default)]` `Vec<String>` and the frontend sends `?? []`, so "empty" means both
  "scanned, found none" and "nobody scanned". Those must be told apart before anything can be skipped on the strength of
  it, which means an explicit flag (or `Option<Vec<_>>`) crossing IPC. Skipping a probe on an ambiguous empty vec would
  silently convert "would have prompted" into "overwrote".
- **(b) One dest listing inside the driver.** Doesn't depend on the FE having scanned. This is the deep-merge path's
  existing pattern (`copy_directory_streaming` lists a merged level once and builds a `name -> FileEntry` map with no
  per-child probes) applied to the top level. **`SmbVolume::scan_for_conflicts` is literally already this code** (one
  `list_directory_impl`, then an in-memory name match), so the backend work may be nil; it's the driver that doesn't use
  the bulk answer.
- **(c) Batch the stats instead of the listing.** Mirrors `scan_for_copy_batch`, which `SmbVolume` already overrides to
  pipeline N stats over one connection (measured 6.5x at 100 files). Turns `N x RTT` into about `N/W x RTT` rather than
  ~1 RTT, so it's the weakest of the three, but it's immune to the huge-directory and staleness problems below.

Recommendation: **(b), with (a) as a fast follow if the flag lands.** (b) has the best win-to-risk ratio and the least
new vocabulary.

### What could go wrong

- **Staleness, and this is the real one.** A per-file stat is taken moments before that file is written; one up-front
  listing can be minutes stale by the time the last file in a large batch lands. A file that appears at the destination
  mid-batch would be missed, and an Overwrite would replace it with no prompt. The merge path already accepts this
  within a level, but its window is one level, not one multi-minute batch. **This is a data-safety question, not a
  performance one, and it's the reason this is a proposal rather than a patch.** Mitigation worth pricing: re-probe only
  the files the listing said were absent AND that are about to be overwritten, which is usually none of them.
- **The operation's own staging temps appear in the listing.** `.cmdr-tmp-*` siblings the batch is currently writing
  would show up as dest entries and must be filtered; the in-flight-temps machinery already exists for the listing read
  path, so this is wiring, not invention.
- **A huge destination directory inverts the trade.** Listing a 200k-entry folder to copy two files into it is worse
  than two stats. Needs a small-N fallback (keep per-file probes below some count).

### MTP and local destinations

They share this driver, so the blast radius has to be checked, and it cuts differently for each:

- **MTP never reaches this code today**, and that's load-bearing. `MtpVolume::max_concurrent_ops()` is 1, so
  `concurrency` is 1, so `use_concurrent_path` is false and the serial driver runs instead. But an MTP listing is
  pathologically expensive: `volume/copy.rs` already documents an MTP `scan_for_copy` costing ~18 s for 1046 photos on a
  cold cache. **So if this fix is written into the concurrent spawn loop it can't touch MTP, and if it's refactored into
  shared conflict-resolution instead, it turns one cheap probe into an 18-second directory listing on a phone.** Keep it
  in the concurrent loop, or gate it explicitly per backend. This is the single most important scoping constraint on the
  work.
- **Local destinations should keep today's behavior.** `LocalPosixVolume::get_metadata` is a `stat`: microseconds. N of
  them cost nothing, and a listing of a large folder would cost more. So the change must be conditional on the
  destination being expensive per operation, not applied universally. Whatever signal option 2 above introduces for the
  concurrency cap ("this volume is network-backed") is probably the same signal this needs, which is an argument for
  doing them in that order.

### How to verify it

- **Re-run this exact sweep and diff the tables.** The harness is committed and the method is fixed, so before/after is
  a direct comparison rather than a new argument. `serial_precheck_floor` becomes an after-number too: if the fix works,
  its share of the best run collapses from 74% toward single digits.
- **`pnpm check rust-integration-tests`** must stay green, M4.4's `MIN_PEAK_IN_FLIGHT` untouched (it's a floor so the
  formula can change; don't weaken it to make something pass).
- **`volume/merge_tests.rs` is the correctness net**, plus a new case for the staleness risk: a destination file that
  appears after the batch's listing but before that file is written must still be treated as a conflict.
- Worth measuring over Tailscale as well as LAN, since the whole effect is `N x RTT` and WAN is where it's largest.

## After: what shipped, and what it measured (2026-08-02, same day)

Two changes landed, and neither is the one M4.3 specified:

1. **A LOCAL volume's `max_concurrent_ops` no longer bounds a REMOTE peer** (`transfer_concurrency` in `volume/copy.rs`,
   on the new `Volume::operations_are_local()`). The defect from "the sweep prices" above.
2. **The per-file destination probe is skipped for a destination directory the operation itself created**
   (`Volume::create_directory_all` now reports `DirectoryCreation::{Created, AlreadyExisted}`). NOT option (a), (b), or
   (c) from the proposal — the conservative fourth option, which trades away no freshness at all: nothing the user
   already had can be inside a folder that didn't exist a moment ago.

### Method change: the harness had to be able to see it

`timed_copy` created the destination directory before starting the timer, "so neither shows up in the number". That
makes every copy a MERGE into a directory the operation didn't create, which is exactly the case the fix leaves alone —
so the sweep as committed would have reported "no change" for a change worth 2-3x. `CMDR_BENCH_DEST` now picks:
`existing` (the old behavior, and what the before-tables above used), `fresh` (the copy creates it), or `both`.

**Both after-sweeps ran on `smb2` 0.15.0**, before Cmdr moved to 0.16.0 (`cd32b8a5e`), which cut the response deadline
180 s → 30 s and the send deadline 60 s → 20 s. Neither deadline is anywhere near a healthy 3.5 ms-RTT copy, so it
shouldn't move these numbers — but a re-run on 0.16.0 is not comparing identical clients, and if a future sweep sees a
transfer die where these didn't, that is the first thing to check.

**`both` interleaves them, round-robin with the windows**, and that is what makes the two tables below comparable: these
runs happened on a machine at load average ~150 (four other agents building), so the absolute numbers are worse than the
before-tables and mean nothing on their own. Load hits both modes equally, so the RELATIVE comparison is sound. ❌ Don't
compare a `fresh` number here against a before-table number; compare it against the `existing` row beside it.

### NAS, many-small: 500 x 16 KiB, 5 reps, both modes interleaved

| window | existing (probes per file) | fresh (probes skipped) | speedup |
| -----: | -------------------------- | ---------------------- | ------: |
|      4 | 5.038 s [4.930-5.282]      | 2.363 s [2.262-2.643]  |   2.13x |
|      8 | 3.887 s [3.766-4.127]      | 1.690 s [1.223-1.756]  |   2.30x |
|     10 | 3.725 s [3.714-3.742]      | 1.551 s [1.383-1.673]  |   2.40x |
|     16 | 3.419 s [3.299-3.557]      | 1.176 s [0.838-1.377]  |   2.91x |
|     32 | 3.462 s [3.376-3.614]      | 915.1 ms [0.813-1.135] |   3.78x |

Serial pre-check floor, measured in the same run: 2.565 s for 500 files (5.13 ms/file), against 2.378 s (4.76 ms/file)
before — the probe itself didn't get cheaper, the driver just stopped issuing it.

Reading it:

- **Spreads are disjoint at every window.** The narrowest gap is window 4 ([4.930-5.282] vs [2.262-2.643]), and it isn't
  close. This is the clearest signal anywhere in this note.
- **The projection held.** "~0.85-1.0 s, call it 3.5-4x faster than today" was arrived at by subtracting the floor and
  assuming the residual stayed the residual. Measured: 915 ms at window 32, 3.78x against the same window with probes.
  The subtraction was honest.
- **The speedup GROWS with the window**, 2.13x at 4 to 3.78x at 32, because the probe is the serial term: removing it is
  what lets a wider window keep buying anything. Which also means the two fixes compound — the first one is what gets a
  Mac past window 4-8 in the first place.
- **`fresh` has not plateaued at 32** where `existing` did at 16. The window ceiling question from "Is 32 still the
  right ceiling?" is therefore genuinely reopened for fresh-directory copies, and ❌ this note does NOT answer it: the
  sweep stops at 32 because the driver clamps there. Someone raising that clamp needs a run that goes past it.
- **`existing` reproduces the before-table shape** (5.038 / 3.887 / 3.725 / 3.419 / 3.462 against 4.700 / 3.752 / 3.522
  / 3.245 / 3.224, uniformly ~7% slower under load), which is the regression check: a merge into a pre-existing
  directory still does exactly what it did.

### NAS, few-large: 32 x 8 MiB (256 MiB), 3 reps, both modes interleaved

| window | existing (probes per file) | fresh (probes skipped) | speedup |
| -----: | -------------------------- | ---------------------- | ------: |
|      8 | 3.004 s [2.969-3.133]      | 2.973 s [2.822-2.976]  |   1.01x |
|     16 | 2.934 s [2.863-2.982]      | 2.729 s [2.647-2.898]  |   1.08x |
|     32 | 2.843 s [2.807-2.886]      | 2.671 s [2.655-2.858]  |   1.06x |

Serial pre-check floor: 154.8 ms for 32 files (4.84 ms/file) = 6% of the fastest run.

**Nothing, as predicted, and that is the point of measuring it.** Spreads overlap at every window. The probe is a
per-FILE tax, so 32 files owe ~155 ms of it against a ~2.8 s link-bound run: the ~6% the floor line reports, and ~100
MB/s is still a saturated gigabit link either way. A change that had somehow made this shape faster would have meant the
measurement was wrong.

### How much of the 74% this recovers, and when — read this before quoting a speedup

The before-baseline's headline was "74% of the fastest run is a serialized per-file probe". Here is what the shipped fix
actually takes back, in the two cases a user can be in. Same run, same session, same connection.

| destination                       | probes issued | floor paid (measured 2.565 s) | window 32 | vs merge |
| --------------------------------- | ------------: | ----------------------------: | --------- | -------: |
| **Pre-existing folder** (a merge) |  one per file |                          100% | 3.462 s   |        — |
| **Folder this operation created** |          zero |                            0% | 915.1 ms  |    3.78x |

**In the fresh-destination case the skip recovers essentially the whole floor**: 3.462 s - 915 ms = 2.547 s saved,
against a floor measured at 2.565 s in that same run. That is ~99% of it, which is the arithmetic working out — there is
no partial credit here, the probes either all run or none do.

**In the merge case it recovers exactly nothing, by construction, and that is the intended behavior.** A pre-existing
folder is where a conflict can genuinely live, so every probe stays.

**Which case a user lands in is decided entirely by whether the destination folder already exists**, and there is no
middle ground: every top-level probe targets the same `dest_path`, so the skip is all-or-nothing per operation.
`TransferDialog` seeds the destination with the OPPOSITE PANE'S CURRENT FOLDER, which by definition exists. So:

- **The default F5 copy is the top row.** It keeps every round trip and is exactly as fast as before.
- **The bottom row is "the user typed a destination folder that doesn't exist yet"** — a new dated backup folder, a
  fresh export directory. Real, and worth 2-3x when it happens, but not the common path.

❌ So "many-small copies to a network destination got 2-3x faster" is not a true sentence about Cmdr. The true sentence
is "copies into a folder Cmdr creates got 2-3x faster; copies into a folder that was already there are unchanged."

### The listing that is already being paid for

`copy_volumes_with_progress` Phase 0.6 calls `reap_stale_transfer_temps(&dest_volume, dest_path)`, which does one
`list_directory` of the destination — **on every copy, including merges, immediately before the spawn loop**.

That is the same round trip the rejected "list the destination once up front" option (b) would need. So the COST side of
that trade is already zero: the listing happens either way, and using its result would remove the per-file probes from
the merge case too — the common case above.

**It does not answer the objection that killed it.** One up-front listing can be minutes stale by the time the last file
in a large batch lands, so a file appearing at the destination mid-batch would be missed and an Overwrite would replace
it with no prompt. That exposure is the reason for the conservative fix, and David chose it again with this cost fact on
the table (2026-08-02). Recorded here as a fact, ❌ not as a proposal: the next person weighing it should start from
"the round trip is already spent" rather than re-deriving it, and should still have to answer the staleness question.

### What this does NOT show

- **Nothing here was measured over WAN.** The effect is `N x RTT`, so a Tailscale link (~50 ms) should show far more;
  untested.
- **Local destinations were not measured**, and the skip is not conditioned on the destination being remote. That is
  deliberate: it matches what `copy_directory_streaming` already does one level down for a freshly-created level, and a
  skipped `stat` cannot be slower than a performed one. But "no measurable local effect" here is a prediction, not a
  measurement.

## The merge case (listing-answered pre-check): shipped, NOT measured

The listing-answered pre-check (`dest_name_index.rs`, commit `28fd62a37`) extends the win to a copy into a PRE-EXISTING
destination — the ordinary F5 flow, and the case the created-directory skip above never touches. It is correct by test
and **unmeasured by benchmark**: the agent that built it died before running the NAS sweep, and the after-number was
never captured.

What is evidenced:

- The **before** number stands: 3.462 s at window 32 for 500 x 16 KiB into a pre-existing folder (table above).
- The mechanism is arithmetic — 500 serialized `get_metadata` calls at ~4.76 ms each are replaced by a listing the
  driver already performs in Phase 0.6, so the ~2.378 s floor should collapse the way it did for the created-directory
  case.

What is NOT evidenced: that the recovery actually matches the ~74% the fresh-destination case achieved. Nobody has run
`CMDR_BENCH_TARGET=nas CMDR_BENCH_DEST=existing` against the shipped code. ❌ Don't quote a speedup for the merge path
until someone does — the reproduce command is at the top of this note and the harness is committed.
