# Phased cover walks against one bulk build, 2026-08-14

The measurement gate in `docs/specs/phased-indexing-plan.md` before the phase machine gets written: what does covering a
real `/` as a sequence of stitched cover walks cost against today's truncate-and-bulk-build, and does it buy the thing
it is for, which is `~/Downloads` being searchable in seconds.

**The gate is 1.5×. Phased as the plan describes it came in at 4.8×, so by the letter of the gate the answer is stop and
re-decide.** But 69% of that is one thing the plan doesn't mention and nothing about phasing requires: a directory whose
`readdir` fails is left unlisted with no cause, so **every later phase's frontier offers it again and pays the same
failing reads again**. Record it once and the same walk drops to 2.2×; move the writer drain from every frontier root to
every phase as well and it lands at **1.84×**.

Time-to-value is exactly what the plan hoped for and the reason the decision isn't obvious: every priority root is
covered in **under 120 ms** against **1.0–26.6 s** for the bulk build. The cost is on the other side of the ledger,
`$HOME`: **88 s phased at its best against 39 s bulk**, which is the signal that gates the early media kick.

## Method

`indexing::lifecycle::phased_bench`, `#[ignore]`d, one arm per process so the memory high-water mark belongs to that arm
alone:

```sh
CMDR_PHASE_BENCH_NO_PROBE=1 cargo test -p cmdr-index --release --lib -- \
  --ignored --nocapture --exact indexing::lifecycle::phased_bench::bulk_build
```

Every arm runs **twice**: once with `CMDR_PHASE_BENCH_NO_PROBE=1` for a wall clock with no instrument in it, once
without for the coverage timestamps. That split is not fussiness — the coverage probe is a recursive query over a
subtree that grows to millions of rows, and running it once a second adds 10–17% to an arm's wall clock (bulk 38.1 s →
43.9 s, depth 2 184.5 s → 215.0 s). **Every wall clock in the tables below is from the probe-free pass; every coverage
timestamp is from the probed one.**

Each arm gets a fresh index in a temp dir, prepared the way `prepare_database_for_a_walk` prepares one but through
writer messages: seed the epoch, then stamp `EXCLUSION_POLICY_KEY` while the index is empty. The harness asserts the
stamp landed, because without it `index_predates_exclusion_policy` answers yes and every coverage query short-circuits
to "walk the whole scope" — the frontier would never shrink and the arm would measure nothing.

**The stitch is in the harness**, which is what makes arm (b) meaningful rather than a measurement of the `NotVirgin`
serial repair. Before each phase, every ancestor of the phase root is read once, its children upserted (files included),
flushed, and that one directory marked listed at the current epoch. Zero frontier roots were refused as non-virgin in
any arm, on any run, which is the stitch working.

**What the harness leaves out**, so these numbers are lower bounds on both sides and the RATIO is the deliverable: no
`IndexManager`, no `ScanProgressReporter` or its 500 ms partial aggregation, no event sink, no watcher, no freshness. A
real bulk scan and a real phase machine both carry all of it.

Machine: Apple M3 Max, 16 cores, 64 GB, internal SSD, macOS 26.5.2 build 25F84, otherwise idle. Branch
`worktree-david+phased-indexing`. **Full Disk Access: granted** (verified by reading
`~/Library/Application Support/com.apple.TCC`), so nothing here is a fast number bought by not being allowed to look.
12 directories came back permission-denied in every arm.

The boot volume held **6,060,889 entries** at bulk-build time. Two SMB shares and a mounted DMG were present under
`/Volumes` throughout; the boot-disk exclusion tier keeps every walk off them.

⚠️ **The plan's reference numbers are stale.** It quotes 5,191,189 entries and a 193 s full walk measured 2026-08-14 on
this machine; the same walk measures **38.1 s over 6.06M entries** here. That matters more than a footnote: the whole
gate is a ratio against a baseline, and a bulk build that finishes in 38 s leaves phasing far less room than one that
takes three minutes. Whatever produced 193 s (the full app, a contended machine, aggregation counted differently) should
be re-derived before the 193 s figure is used for anything else.

## Wall clock to full coverage

Probe-free pass. Every arm indexed 6,067,5xx entries (the phased arms sit ~6,700 above the bulk build, which is the
stitch's own upserts of directories the bulk walk reaches by descent).

- **bulk build** (`scan_volume` from `/`) — **38.1 s**, 1.00×. Peak resident 586 MB, peak phys footprint 457 MB.
- **phased, stitch depth 1** — **182.5 s**, 4.79×. 405 MB / 240 MB.
- **phased, stitch depth 2** — **184.5 s**, 4.84×. 404 MB / 248 MB.
- **phased, depth 2, while browsing and searching** — **180.7 s**, 4.74×. 408 MB / 230 MB.
- **phased, depth 2, writer drained once per phase** — **155.1 s**, 4.07×. 531 MB / 264 MB.
- **phased, depth 2, four frontier roots at once** — **103.9 s**, 2.73×. 403 MB / 240 MB.
- **phased, depth 2, unreadable ground recorded** — **82.5 s**, 2.17×. 412 MB / 253 MB.
- **phased, depth 2, unreadable ground recorded + drained once per phase** — **70.0 s**, **1.84×**. 808 MB / 621 MB.

**Peak memory is the one number that favours phasing outright.** The bulk build carries the whole aggregation
accumulator and finishes with one `ComputeAllAggregates` over 6M rows; the phased arms aggregate per subtree and hold
405–412 MB against 586 MB, roughly half the phys footprint. The exception is the last arm, whose per-phase drains let a
6M-row backlog build up in the writer queue (808 MB) — a real cost of batching drains, not a free lunch.

## Where the 4.8× actually goes

Depth 2, probe-free, the arm as the plan describes it (184.5 s):

- **146.7 s walking** 1,496 frontier roots,
- of which **101.2 s went to 224 frontier roots that yielded NOTHING**,
- **37.5 s draining the writer** (once per frontier root),
- 0.2 s stitching 107 directories, 5 ms asking for frontiers.

So the stitch and the coverage queries — the two things the plan spends its design effort on — cost **0.2 s combined,
0.1% of the arm**. They are free. The cost is entirely in the other two lines.

### The re-offer problem: 69% of the walk time

`insert_visitor.rs:406-428` records a cause only for permission-denied. Every other `readdir` failure is left "plain
unlisted and retried", deliberately, because a transient error should heal. Under one whole-volume scan that policy is
right and costs one retry. Under phases it means **a directory that fails is handed back by every later phase's
frontier, immediately, at full cost**.

On this machine 76 directories fail that way, all inside `~/Library/CloudStorage` (a MacDroid MTP mount for a phone, and
File Provider domains for Dropbox and Google Drive). They are walked once as a priority root, then re-offered by the
`$HOME` phase and again by the `/` phase: 224 barren frontier roots, 101.2 s of nothing.

Recording them once — one `MarkDirsUnreadable` per phase for whatever that phase left unlisted under its root — takes
the barren roots from 224 to 72 and their cost from **101.2 s to 42 ms**, and the arm from 184.5 s to 82.5 s. Entry
counts are identical (6,067,569 against 6,067,556), so nothing was skipped to get there.

⚠️ **The signal is NOT `heartbeat.abandoned_count()`.** That counts stall timeouts and consecutive-failure pruning, and
it was **zero for every walk in every arm** — the first version of this measurement gated on it, marked nothing, and
reported no improvement. The honest signal is what a finished walk left unlisted under its own root.

⚠️ **And the mark has to come after the phase's drain.** A walk sends its `MarkDirsListed` last, so a mark computed
before that commits condemns thousands of perfectly good directories. That bug made the arm look 2× faster while
silently indexing 1.28M fewer entries — it reads exactly like a win, and the only evidence in the output was an entry
count 21% low. `marking_unreadable_ground_costs_no_coverage` pins it.

### The drain: the walk and the writer stopped overlapping

The bulk build walks and writes at the same time for its whole run. One `cover()` call per frontier root ends with a
blocking flush, so with 1,496 roots the two never overlap: 37.5 s of the arm is the walker standing still. Draining once
per phase instead recovers ~30 s of it (155.1 s against 184.5 s on its own; 70.0 s against 82.5 s combined with the
mark), at the cost of a much larger writer backlog.

This is the knob the plan already names ("batch roots into small groups, ❌ never into one"). It is worth ~16% on its
own and it is the whole remaining gap once the re-offer problem is fixed.

### Not parallelism, mostly

Walking four frontier roots at once brings the unfixed arm to 103.9 s (2.73×), which looks like parallelism starvation.
It mostly isn't: the win comes from the barren roots' stall timeouts overlapping each other. Once the barren roots are
gone the real walking is **41 s against the bulk build's whole 38.1 s run**, so one root at a time keeps the machine
about as busy as one whole-volume walk does. **The plan's join rule costs nothing measurable and does not need
revisiting.**

## Time to value, which is what the plan is for

Probed pass, coverage timestamps at ±1 s. "Covered" means the count of unlisted directories under that folder stopped
moving; where a folder settles above zero, that is ground no walk in that arm could read.

- **bulk build**: Downloads 1.0 s, Movies 1.0 s, Dropbox 1.0 s, Documents 2.1 s, Music 6.4 s, **Desktop 19.0 s**,
  **Pictures 26.6 s**, `~/Library` 38.9 s, `$HOME` 38.9 s.
- **phased, depth 1 or 2**: Downloads, Documents, Desktop, Pictures, Movies, Music, Dropbox **all ≤ 1.0 s** (the first
  sample), `~/Library/CloudStorage` 4.2 s, `~/Library` 149–154 s, `$HOME` 149–154 s.
- **phased, depth 2, unreadable ground recorded + drained once per phase**: same ≤ 1.0 s for every priority root,
  `~/Library/CloudStorage` 5.6 s, **`~/Library` 72.8 s**, **`$HOME` 88.4 s**.

The phased arms also carry an exact, instrument-free version of the same answer, derived from when each frontier root's
walk ended rather than probed. At depth 2 the priority roots land at **19.9 ms, 21.9 ms, 92.2 ms, 105.4 ms, 109.0 ms,
114.4 ms** — the ±1 s probe simply can't see how early they are.

**Read both halves.** Phasing turns the worst priority root from 26.6 s into 0.1 s, a 250× improvement on exactly the
thing the plan exists for. It also pushes `home_covered_at` from 39 s to 88 s at best, and 154 s as specified. The plan's
own open question was "whether `~/Library`'s size makes `home_covered_at` late enough to want the M1 refinement". It
does: `~/Library` is walked as part of the `$HOME` phase in every arm, and it is what `$HOME` waits for.

## Depth 1 against depth 2 (plan decision 13)

Decision 13 assumes stitching two levels under the `$HOME` and `/` phase roots cuts the worst-case wait for a
newly-queued root by roughly 3×, because `~/Library/Caches` (423k) becomes a frontier root instead of `~/Library`
(1.44M). **Measured, it buys almost nothing, and it is almost free.**

- **Worst-case wait, depth 1**: 14.0 s, `~/projects-git` (2,420,523 entries). Then 5.2 s, 3.7 s, 3.5 s, 2.1 s.
- **Worst-case wait, depth 2**: 13.4 s, `~/projects-git/vdavid` (2,358,641 entries). Then 5.3 s, 3.6 s, 3.2 s, 2.1 s.
- Frontier roots: 365 at depth 1, 1,496 at depth 2. Stitch cost: 4 directories / 28 ms against 107 directories / 217 ms.
- Wall clock: 182.5 s against 184.5 s, a 1.1% difference that is inside run-to-run noise.

**Why the 3× didn't materialise**: on this machine the largest subtree is `~/projects-git`, and 97% of it is a single
child, `~/projects-git/vdavid`. Splitting one level deeper hands the walker a root that is barely smaller. `~/Library`
does split as predicted (its biggest depth-2 child is `Caches` at 423k, walked in 3.6 s), but `~/Library` was never the
worst case.

**So decision 13 is right on cost and wrong on benefit.** Depth 2 costs 190 ms of extra stitching and no wall clock, so
taking it loses nothing; it just shouldn't be counted on to bound the wait. The thing that actually bounds the wait is
one user's one big folder, and no stitch depth short of "walk it in pieces" changes that.

## The browsing arm

Depth 2 with a pane doing a real `readdir` plus an enrichment read every 3 s through six folders ahead of the walker,
and a second, search-shaped `cover_subtree` of `/Applications` (300,687 entries) starting 30 s in — a walker the phase
machine doesn't control, which is what a live search is.

**It cost nothing: 180.7 s against 184.5 s, inside noise, and slightly faster.** The second walker did real work
concurrently and removed `/Applications` from the `/` phase's serial list. Peak memory was unchanged (408 MB). Nothing
was corrupted: the arm indexed the same 6,067,558 entries, and zero frontier roots were refused as non-virgin.

## What I'd stake a decision on, and what I wouldn't

**Solid**: the ratios, the cost decomposition, the depth-1-against-depth-2 answer, and the time-to-value numbers. Each
arm was run 3–5 times across the afternoon; wall clocks repeated within ~5% and the decomposition within ~2%.

**Weaker**: the absolute wall clocks drift with page-cache warmth across the afternoon (the same depth-1 arm read
176.3 s, 176.9 s, 182.5 s, and 188.2 s). The `~/projects-git` walk in particular ranged 13.4–34.3 s between a cold and a
warm afternoon. Always compare arms from the same pass, never across passes.

**The 76 unreadable directories are this machine's.** A machine with no MTP mount and no stalled File Provider domain
would see a much smaller re-offer cost, and the unfixed phased arm would come in well below 4.8×. It would not see
*zero*: any `readdir` failure anywhere reproduces it, and the fix costs one message per phase either way.

## Conditions that would change the answer

- **A slower disk or a spinning drive.** Everything here is one internal SSD; the walk is the shared cost and the drain
  is the phased-only one, and their ratio moves with the storage.
- **Fewer cores.** The bulk build saturates 16 of them. On a 4-core Mac one whole-volume walk has less of an advantage
  and the gap narrows.
- **A volume with one dominant subtree, or many even ones.** The depth-1-against-depth-2 answer is entirely a property
  of the tree's shape, and this tree has one 2.4M-entry child that is itself 97% one folder.
- **Anything that makes the bulk build slower again.** If the plan's 193 s is real under the full app, the same phased
  overhead against a 193 s baseline is a completely different ratio, and the gate would pass without any fix at all.
  **This is the single biggest lever on the decision and it is unresolved.**

## Recommendation (David's call, not mine)

**Against the gate as written, phased fails: 4.8× against a 1.5× bar.** But the gate's two prepared answers were written
for a phased shape that is inherently slower, and that is not what the measurement found. Three of the four costs the
plan worried about are free (stitch 0.2 s, coverage queries 5 ms, the one-walk-at-a-time join rule ~0 s), and the two
that aren't are both fixable without changing the design:

1. **Record ground a walk could not read**, so no later phase re-offers it. 184.5 s → 82.5 s. This is a bug worth fixing
   on its own merits, like M0's four: it costs a scan nothing today and a phased machine 101 s. It needs its own
   `UnreadableCause` variant, since "the walk could not read it" is neither a permission the user can grant nor a
   refusal Cmdr chose, and it must be cleared by a later successful listing exactly as `Denied` is.
2. **Drain the writer once per phase rather than once per frontier root** (the plan's own "batch into small groups").
   82.5 s → 70.0 s.

That is **1.84×**, still above the 1.5× bar, with the remaining ~25 s being writer commit work the bulk build overlaps with
walking and the phased shape currently does not. I did not measure a variant that overlaps it, because letting the next
root walk while the previous root's rows commit changes when a root may be reported covered, which is a design question
rather than a knob.

So my recommendation is the gate's **first prepared answer, accept the slower full coverage**, conditional on fix 1
landing first — with the honest caveat that the trade is sharper than the plan assumed. Phasing buys the worst priority
root going from 26.6 s to 0.1 s and costs `home_covered_at` going from 39 s to 88 s. If the early media kick matters
more than sub-second priority coverage, that trade is bad and the second prepared answer is the better one. **Resolve
the 193 s-against-38 s baseline discrepancy before deciding**: if the real in-app bulk build is anywhere near 193 s, the
whole question dissolves.

❌ I have not acted on any of this.
