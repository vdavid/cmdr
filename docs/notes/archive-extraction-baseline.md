# Archive-extraction: before and after

The measured before and after for moving `backends/archive/**` (8,352 lines, 2.5% of `src-tauri/src`) into the
standalone `cmdr-archive` crate. This is the gate the per-filesystem-backend plan (`docs/specs/backend-crates-plan.md`)
ends at, so the answer it produces matters more than the numbers themselves.

Scenarios and commands match `docs/notes/index-extraction-baseline.md`, which is the only way the two extractions are
comparable. Read that one first if you're checking whether this one drew the right conclusion.

## The short version

- **The scoped inner loop got 12–18× cheaper**, and by more than the index extraction managed: type-checking the archive
  after an archive edit went **4.67 → 0.34 CPU seconds (−93%)** and building its unit tests went **27.6 → 2.7 CPU
  seconds (−90%)**. The index's equivalents were −83% and −85%.
- **A full app build after an archive edit is FLAT**, not the +11% the index extraction measured. Two `after` runs
  straddle the `before` figure (16.3 and 19.2 CPU seconds against 18.1), so the honest reading is "no change", and the
  reason is size: archive is 2.5% of the tree where the index was 28%, so what the app crate stops compiling roughly
  cancels what it now has to relink.
- **Release builds got 13% faster** (262.8 → 228.3 s wall) for +0.4% binary size, and this run shows the mechanism
  rather than inferring it: CPU utilization rose from **695% to 812%**. Total CPU went UP 1.5%. More crates didn't
  reduce the work, they gave cargo more of it to schedule at once.
- **`pnpm check` did not get faster and cannot**, for the reason `docs/specs/backend-crates-plan.md` § "Reason 2" states
  up front: every Rust check shares one `rustInputs` set and runs `--workspace`. Nothing here changes that.

## Read this before comparing

- **Both sides were measured on a BUSY machine**, and this is the note's main methodological caveat. Load average ranged
  27–125 across the runs (several other agents were building the same workspace concurrently, plus a `nilaway` pass,
  `eslint`, and Spotlight indexing). The index baseline was taken at load ~8 at worst.
- **So every scenario reports CPU seconds (user + sys, children included) alongside wall clock, and the CPU column is
  the one to trust.** Wall clock under a swinging load says more about the other agents than about cargo; CPU seconds
  measure the work actually done. Where the two disagree, this note follows CPU and says so.
- **The `before` side is `152d3fe79`, NOT `main`.** That's the commit immediately preceding the move, where the archive
  backend already talks to its host through the seams but still lives in the app crate. Comparing against `main` would
  have charged the seam work (P0, P1, and the seam rewiring) to the extraction, which measures the wrong thing: the
  question is what the CRATE BOUNDARY costs and buys.
- **Thin LTO is on** (`[profile.release] lto = "thin"` at the workspace root), on both sides, as it was for the index
  measurement.
- Measured 2026-08-03 on an Apple M3 Max (16 cores, 64 GB), macOS 26.5.2, rustc 1.97.1 (`8bab26f4f`, 2026-07-14), in the
  `david-backend-crates` worktree.

## Build times

### The inner loop, and the full build

Every edit is real (a changed log-message string in the archive content watch), not a `touch`, so incremental
compilation has to redo codegen for the touched unit. Medians of five consecutive runs per side, after a warm-up build.

The scoped commands are what an agent working on the archive actually runs: `cargo check --lib` and
`cargo test --lib --no-run` on the thing being edited. Before the split that meant the 332k-line app crate, because
there was no way to ask for less; after, it's `cargo check -p cmdr-archive --lib`.

| scenario                                                      | before (wall / CPU s) | after (wall / CPU s) | CPU delta |
| ------------------------------------------------------------- | --------------------- | -------------------- | --------- |
| Archive edit, then `cargo check --lib` on what you're editing | 6.86 / 4.67           | **0.38 / 0.34**      | **−93%**  |
| Archive edit, then `cargo test --lib --no-run` on it          | 48.93 / 27.61         | **1.55 / 2.74**      | **−90%**  |
| Archive edit, then `cargo build` (the whole app)              | 46.93 / 18.07         | 14.42 / 16.27        | flat      |

**The first two rows are the answer**, and they're the same answer the index extraction gave, only larger. They come
from not compiling the app at all — which is why the win doesn't scale with the extracted subsystem's size, and why it
transfers to any backend regardless of how small it is.

**The third row's wall-clock figures are not comparable** and are shown only for completeness: the `before` run's load
was rising (29 → 65) while the `after` run's was falling (102 → 27). On CPU, two independent `after` runs gave 19.21 and
16.27 seconds against `before`'s 18.07, so the honest reading is that a full app build after an archive edit didn't
move.

**Reproducibility.** The `after` side was measured twice, 30 minutes apart under different loads. The scoped test build
reproduced within 4% on CPU (2.84, then 2.74 seconds); the scoped type-check within 0.13 seconds absolute (0.47, then
0.34) — small enough that the percentage is noisy but the ORDER OF MAGNITUDE is not, which is all a −90% claim needs.
The `before` side's first sample of each scenario is a cold outlier (the warm-up doesn't cover `cargo check`'s own
fingerprint set) and the median discards it, exactly as it would on either side.

**Running the tests is fast too, though it isn't in the table** because there's no `before` equivalent to compare
against: `cargo test -p cmdr-archive` runs all 146 of the crate's tests in 1.7 seconds.

### Release

Full clean workspace build (`cargo clean --release && cargo build --release` from the repo root, so every member
builds), one run per side, taken back to back so they saw the most similar load of any pair here.

| measure         | before       | after        | delta   |
| --------------- | ------------ | ------------ | ------- |
| wall clock      | 262.8 s      | **228.3 s**  | −13.1%  |
| total CPU       | 1,827.1 s    | 1,854.9 s    | +1.5%   |
| CPU utilization | 695%         | **812%**     | +117 pp |
| `Cmdr` binary   | 79,792,240 B | 80,100,336 B | +0.4%   |

**The release build got faster without doing less work**, and the utilization column is the proof. Four crates give
cargo four independent codegen units to schedule where there were three, so the long pole shortened while total CPU went
slightly UP. That's the same effect the index extraction saw (214 → 188 s, −12%) at nearly the same magnitude, despite
archive being a tenth of the index's size — which suggests the release win comes from parallelism structure rather than
from how much code moved, and shouldn't be expected to stack linearly with more extractions.

The extra 308 KB of binary is more cross-crate inlining, the same direction thin LTO already moved it (+2.2 MB when it
landed).

## What this means for extracting SMB

**The measurement gate passes, and it passes on the metric the plan said mattered.** But "the numbers justify P3" and
"P3 is the best use of the next stretch of effort" are different questions, and the honest answer differs between them.

**The benefit is proven and it transfers.** The inner-loop win comes from not compiling the app, not from the size of
what moved, so `cargo check -p cmdr-smb` would land in the same sub-second range. Archive is 2.5% of the tree and got
−93%; there's no size threshold below which this stops working.

**The benefit does NOT grow with the extraction's difficulty, and SMB's is several times archive's.** Archive had three
coupling points and no `cfg(test)` behavior gates; SMB has 23 sites across all seven seams, an `AppHandle` in a
`OnceLock` feeding `tauri_specta` emits, two registry reach-backs, a `pub(in crate::…)` visibility with no cross-crate
spelling, 5,343 lines of Docker-gated tests reached through a `use super::*` prelude glob, and a
`smb2 = { features = ["testing"] }` forward. The payoff per unit of effort is therefore much lower than the pilot's,
while the payoff for a backend written as a crate from day one (FTP, S3, SFTP) is the same as archive's and costs nearly
nothing.

**One measured argument in P3's favor that the plan didn't anticipate.** The extraction surfaced two latent defects the
app crate had been hiding: seven `.unwrap()`s that were legal only because their file was `cfg(test)`, and a rustdoc
intra-doc link to a function that no longer exists. Neither was found by any check while the code lived in the app.
SMB's test surface is 1.6× archive's, so a similar or larger crop should be expected — that's real quality, not just
build time.

**The recommendation, and it's a judgment call rather than a number:** do P4 unconditionally (a new backend written as a
crate is nearly free and gets the full win), and treat P3 as optional — worth doing when someone is about to spend
sustained time inside SMB, not worth doing for its own sake. Nothing measured here argues against P3; what argues
against it is that the same effort spent on P4 buys more.

## Redoing this

Re-run it if a backend's build cost is ever questioned. The harness is three scenarios and a release build, all
reproducible from the table above; the only thing that needs care is the pairing. Take both sides in the same worktree
minutes apart, record the load average at the start and end of each side, and report CPU seconds if the machine isn't
idle. An unpaired wall-clock number on a shared machine is worth nothing here — the `before` and `after` full-build wall
times in this note differ by 3× purely from other agents' load, in the direction that would have flattered the
extraction.
