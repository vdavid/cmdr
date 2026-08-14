# Phased cover walks against one bulk build, 2026-08-14

The measurement gate in `docs/specs/phased-indexing-plan.md` before the phase machine gets written: what does covering a
real `/` as a sequence of stitched cover walks cost against today's truncate-and-bulk-build, and does it buy the thing
it is for, which is `~/Downloads` being searchable in seconds.

**The baseline is 39.1 s, not the 145–193 s two in-repo sources claim.** That was checked the only way worth trusting:
by running the real app. A release Cmdr on a throwaway data dir indexed this `/` in **39.1 s (6,072,728 entries, 603,559
dirs)**, and the harness arm carrying the same post-scan sequence measured **39.1 s** for the same tree. The harness is
an accurate model of a real first scan; the older figures do not reproduce. Details and the one confounder nobody has
excluded: "The baseline, resolved" below.

**Against that baseline, phased as the plan describes it is 4.7×**, so by the letter of the 1.5× gate the answer is stop
and re-decide. But 69% of that is one thing the plan doesn't mention and nothing about phasing requires: a directory
whose `readdir` fails with a non-permission errno is left unlisted with no cause, so **every later phase's frontier
offers it again and pays the same failing reads again**. On this machine that is 1,497 directories inside a wedged
MacDroid MTP mount returning `ETIMEDOUT`. Record it once and the same walk drops to **2.10×**; batch the writer drain as
well and it lands at **1.79×**.

**So the representative number is ~2.1×, and 4.7× is this machine's dead mount.** A machine without one has no re-offer
cost, which is exactly what the marking arm measures.

Time-to-value is what the plan hoped for and the reason the decision isn't obvious: every priority root is covered in
**under 120 ms** against **1.0–26.6 s** for the bulk build. The cost is on the other side of the ledger, `$HOME`: **88 s
phased at its best against 39 s bulk**, which is the signal that gates the early media kick.

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
`~/Library/Application Support/com.apple.TCC`), so nothing here is a fast number bought by not being allowed to look. 12
directories came back permission-denied in every arm.

The boot volume held **6,060,889 entries** at bulk-build time. Two SMB shares and a mounted DMG were present under
`/Volumes` throughout; the boot-disk exclusion tier keeps every walk off them.

## The baseline, resolved

Two in-repo sources put a real first scan at 145–193 s, four to five times what the harness measured, and a gate that is
a ratio can't be decided on a disputed denominator. Three hypotheses, checked:

**1. Missing work in the harness's bulk arm — refuted, and now proven rather than argued.**
`bulk_build_with_the_full_post_scan_sequence` adds everything `start_scan` and the completion handler put on the same
writer: `set_expected_total_entries`, `BackfillMissingDirStats`, and a `ScanProgressReporter`-shaped 500 ms tick firing
`ComputePartialAggregates { source: Maps }` every tenth pass. Both bulk arms already ran `ComputeAllAggregates` (that's
inside `scan_volume`) and both waited for it through `flush_blocking`. Every arm now reports what it left behind, so
"the same work" is shown:

- bulk build: 37.2 s, **603,559 `dir_stats` rows for 603,559 dirs**, 603,433 covered, 995,334,483,968 bytes at the root.
- bulk build + full post-scan sequence: **39.1 s**, 603,559 rows for 603,559 dirs, 603,439 covered, 995,329,966,080
  bytes.
- phased + mark + per-phase drain: 70.1 s, 604,191 rows for 604,191 dirs, 604,038 covered, 995,391,729,664 bytes.

Same row count, same coverage, same volume total to within 0.006% (ordinary churn between runs). **Neither side is
skipping aggregation**, and the full sequence costs the baseline 1.9 s, not two minutes.

**2. Throttling — refuted.** The local walker never consults the `clearance` seam. The only production callers are the
media-index scheduler and the network scanner's pacing (`grep -rn '\.clearance('`). A local `/` scan runs unpaced in the
app exactly as it does here.

**3. The app really is slower — refuted, by running it.** A release build on a throwaway `CMDR_DATA_DIR`, launched from
this FDA-granted shell, logged:

```
Scan: complete (6072728 entries, 603559 dirs, 39.1s)
ComputeAllAggregates: done, 603560 directories in 3.9s
```

**39.1 s, to the tenth of a second the same as the harness arm carrying the same composition.** It fired 7 partial
passes, the last 1 573 ms over 588K dirs.

**So the harness is a faithful model and 39.1 s is the baseline.** What it does NOT carry: the FSEvents watcher and its
buffered-event reconcile, the event sink and its IPC, freshness, and the frontend. Those run alongside the scan rather
than inside it, and the app's own scan timer says they cost it nothing measurable.

⚠️ **What changed since the older measurements was not established.** `partial_agg.rs` recorded 5.94M entries / 558K
dirs in ~2m25s with 28 passes each ≤ 397 ms on 2026-08-03; today the same code path over a slightly bigger tree gives
39.1 s and 7 passes, the last one 4× slower per pass than any of theirs. That shape (faster overall, slower per pass) is
not what a pure page-cache difference produces, but **page-cache warmth is the confounder I could not exclude**:
`sudo purge` needs a password this session doesn't have, and no unprivileged way to evict 6M inodes from 64 GB of RAM is
practical. The best bound available is that the **first** bulk measurement of the session, taken before any arm had
traversed the tree, was **38.5 s** — if a cold cache cost 4×, it would have shown there. Both stale claims have been
re-anchored in place rather than left to mislead.

## Wall clock to full coverage

Probe-free pass, ratios against the **39.1 s** full-composition baseline. Every arm indexed 6.06–6.08M entries (the
phased arms sit ~6,700 above the bulk build, which is the stitch's own upserts of directories the bulk walk reaches by
descent).

- **bulk build + full post-scan sequence** — **39.1 s**, 1.00×. Peak resident 772 MB, peak phys footprint 634 MB. _The
  baseline; matches the real app exactly._
- **bulk build** (`scan_volume` alone) — 37.2 s, 0.95×. 574 MB / 443 MB.
- **phased, stitch depth 1** — **183.7 s**, **4.70×**. 401 MB / 241 MB.
- **phased, stitch depth 2** — 184.5 s, 4.72×. 404 MB / 248 MB.
- **phased, depth 2, while browsing and searching** — 180.7 s, 4.62×. 408 MB / 230 MB.
- **phased, depth 2, writer drained once per phase** — 155.1 s, 3.97×. 531 MB / 264 MB.
- **phased, depth 2, four frontier roots at once** — 103.9 s, 2.66×. 403 MB / 240 MB.
- **phased, depth 2, unreadable ground recorded** — **82.1 s**, **2.10×**. 411 MB / 254 MB.
- **phased, depth 2, unreadable ground recorded + drained once per phase** — **70.1 s**, **1.79×**. 773 MB / 613 MB.

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

**What actually fails here, measured rather than assumed**: a recursive `scandir` of `~/Library` found **1,497
directories returning `ETIMEDOUT` (errno 60)**, every one of them inside
`~/Library/CloudStorage/MacDroid-googlePixel9ProXL` — a FUSE mount for an Android phone, exposing the phone's `/proc`
(`.../proc/1069/fd`, `.../proc/1069/task/1069/net/can`, …). The phone is not connected, so each read blocks and then
times out. That single-threaded probe took over 13 minutes to enumerate them.

The walker never visits all 1,497: its consecutive-failure budget prunes whole subtrees, so a walk leaves **76**
directories unlisted. Those 76 are walked once as a priority root, then re-offered by the `$HOME` phase and again by the
`/` phase: 186–226 barren frontier roots, ~101 s of nothing.

⚠️ **Why the bulk build doesn't pay this and the phased shape does.** Both traverse the wedged mount, but the bulk build
does it as 1,497 tasks inside one 16-thread pool overlapped with six million other entries, so the timeouts hide behind
real work. The phased shape hands each failed directory back as **its own frontier root, walked serially**, where
nothing hides it. That is structural, not a quirk: any dead mount converts into serial dead time in proportion to the
number of phases above it.

**This is a live bug today, not only under phases.** `coverage_for_scope` puts those 76 directories in the frontier, so
every search scoped at or above `~/Library` hands them to a cover walk that pays the timeouts again — today, in the
shipped build, on any machine with a disconnected File Provider or FUSE mount.

Recording them once — one `MarkDirsUnreadable` per phase for whatever that phase left unlisted under its root — takes
the barren roots from 226 to 72 and their cost from **101.9 s to 34 ms**, and the arm from 183.7 s to 82.1 s. Entry
counts are identical (6,079,410 against 6,079,395) and both leave 604,19x `dir_stats` rows, so nothing was skipped to
get there.

### Does one `UnreadableCause::Abandoned` cover every case? Yes.

The plan already designs `Abandoned` for two producers. There is a third, and it is the one that fires here:

1. **The watchdog timeout** — `WalkReadError::TimedOut`, the read produced nothing for 15 s. Reaches `visit_read_error`
   with the directory's id. The plan has this one.
2. **The consecutive-failure budget** — `engine.rs:341-347` drops a queued sibling task unread, so it never reaches the
   visitor at all; the id is on `scheduled.task`. The plan has this one.
3. **`readdir` returning a non-permission errno** — `WalkReadError::Io(e)` with `e.kind() != PermissionDenied`. Reaches
   `visit_read_error` with the id, and is deliberately left unmarked ("any other I/O error might be transient, so those
   stay plain unlisted and get retried"). **The plan does not have this one, and it is 100% of what fires on this
   machine.**

**One variant covers all three**, because nothing downstream branches on which of them happened. The cause is consumed
in three places, and all three want the same answer from each case: the coverage verdict (not the user's to fix, so ❌
never the `permission_denied` bucket that says "grant Full Disk Access"), completion (must not hold the frontier open),
and healing (`MarkDirsListed` already clears any cause on the next successful listing). ❌ Don't split it by errno:
`ETIMEDOUT` on a wedged mount and `ENOENT` on a directory that vanished mid-walk both want "stop offering this", and the
`ENOENT` row is the watcher's to delete anyway.

⚠️ **Two things the implementer has to carry across with it.** First, if the mark is applied by today's bulk scan too —
and it should be, since case 3 is a live bug there — it **changes shipped behavior**: a transient failure stops being
retried by the next walk. The exposure is narrow (a truncating fresh scan wipes every row and cause; `MarkDirsListed`
clears on any success), but it makes the plan's `ClearUnreadableCause { cause: Abandoned }` backoff **not optional**.
Second, the mark must be computed **after the walk's own `MarkDirsListed` has committed**, or it condemns everything the
walk listed but hasn't stamped yet — see the trap below.

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
thing the plan exists for. It also pushes `home_covered_at` from 39 s to 88 s at best, and 154 s as specified. The
plan's own open question was "whether `~/Library`'s size makes `home_covered_at` late enough to want the M1 refinement".
It does: `~/Library` is walked as part of the `$HOME` phase in every arm, and it is what `$HOME` waits for.

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

**Solid**: the ratios, the cost decomposition, the depth-1-against-depth-2 answer, the time-to-value numbers, and the
baseline. Each arm ran 3–6 times across the afternoon; wall clocks repeated within ~5% and the decomposition within ~2%.
The baseline is the strongest of them: an independent release-app run agreed with the harness arm to 0.1 s, and both
arms are shown producing the same 603,559 `dir_stats` rows and the same volume total.

**Weaker**: the absolute wall clocks drift a few percent across the afternoon (the same depth-1 arm read 176.3 s, 176.9
s, 182.5 s, 183.7 s, and 188.2 s). The `~/projects-git` walk in particular ranged 13.4–34.3 s. Always compare arms from
the same pass, never across passes.

**Unresolved, and the one thing I would not stake the decision on alone**: whether a genuinely cold page cache moves the
baseline. Everything ran on a machine that had been walking this tree all afternoon; `sudo purge` needed a password this
session didn't have, and no unprivileged way to evict 6M inodes from 64 GB is practical. The bound is that the first
bulk reading of the session, before any arm had traversed the tree, was 38.5 s. **One reboot-fresh measurement would
close it**, and it is worth taking before the gate call, because the ratio is only as good as its denominator.

**The 76 unreadable directories are this machine's**, so 4.7× is not the number a typical machine would show. ~2.1× is —
that is what the marking arm measures, and marking is precisely "behave as if the dead mount weren't there". It would
not be _zero_ on any machine: one `readdir` failure anywhere reproduces the mechanism, and the fix costs one message per
phase either way.

## Conditions that would change the answer

- **A slower disk or a spinning drive.** Everything here is one internal SSD; the walk is the shared cost and the drain
  is the phased-only one, and their ratio moves with the storage.
- **Fewer cores.** The bulk build saturates 16 of them. On a 4-core Mac one whole-volume walk has less of an advantage
  and the gap narrows.
- **A volume with one dominant subtree, or many even ones.** The depth-1-against-depth-2 answer is entirely a property
  of the tree's shape, and this tree has one 2.4M-entry child that is itself 97% one folder.
- **A machine with no wedged mount.** Then the plan's shape costs what the marking arm costs, ~2.1×, and the 4.7× here
  never appears. Conversely a machine with several dead mounts is worse than 4.7×.
- **A cold page cache**, the one confounder left standing. See above.

## Recommendation (David's call, not mine)

**The baseline is settled at 39.1 s, and against it the plan's shape as written is 4.70× — over the 1.5× bar.** But the
gate's two prepared answers assume a phased shape that is inherently slower, and that is not what the measurement found.
Three of the four costs the plan worried about are free (stitch 0.2 s, coverage queries 5 ms, the one-walk-at-a-time
join rule ~0 s), and the two that aren't are both fixable without changing the design:

1. **Record ground a walk could not read**, so no later phase re-offers it. 183.7 s → 82.1 s (**2.10×**). This is a bug
   worth fixing on its own merits, alongside M0's four, and it is not phasing-specific: it makes every search scoped
   above a dead mount pay the timeouts again, today, in the shipped build. It wants one `UnreadableCause::Abandoned`
   covering all three producers, and it obliges the plan's `ClearUnreadableCause` backoff to ship with it.
2. **Drain the writer once per phase rather than once per frontier root** (the plan's own "batch into small groups").
   82.1 s → 70.1 s (**1.79×**).

**1.79× is the best measured configuration**, still above the bar. The remaining ~21 s is writer commit work the bulk
build overlaps with walking and the phased shape does not. I did not measure a variant that overlaps it: letting the
next root walk while the previous root's rows commit changes when a root may be reported covered, which is a design
question rather than a knob, and I was asked not to build the machine.

So my recommendation is the gate's **first prepared answer — accept the slower full coverage — conditional on fix 1
landing first**, with two honest caveats. The trade is sharper than the plan assumed: phasing buys the worst priority
root going from 26.6 s to 0.1 s and costs `home_covered_at` going from 39 s to 88 s. And 1.79× is a real overshoot of a
bar David set, not a rounding error; taking it means deciding the bar was measuring the wrong thing, since 30 extra
seconds of politely-throttled background walking is invisible unless someone is watching the badge, while the
minutes-earlier priority coverage is not.

If the early media kick outranks sub-second priority coverage, the second prepared answer (`$HOME` and the priority
roots only, no whole-drive phase) is the better product, and the numbers here don't argue against it.

**Before deciding, take one reboot-fresh baseline.** It is the only input to this that is still a bound rather than a
measurement.

## What has been acted on since

**Fix 1 landed on 2026-08-14, as a shipped-build bug fix rather than as part of the phased plan.** A local walk records
every directory it couldn't read as `UnreadableCause::Abandoned` (all three producers, including the `readdir`-errno one
above), and a persisted per-volume 1 h → 4 h → 24 h backoff reopens that ground. Canonical write-ups:
`crates/cmdr-index/src/indexing/store/DETAILS.md` § "What coverage needs" and
`crates/cmdr-index/src/indexing/writer/DETAILS.md` § "Retrying ground a walk gave up on".

So the arms above are no longer comparable to a re-measurement on this branch: the marking arm's behavior is now the
BASELINE walk's behavior, and the "unfixed" 4.70× shape can't be reproduced without reverting the fix. A re-run should
compare the drain knob against ~2.10×.

❌ Nothing else here has been acted on: the drain batching, the phase machine, and the gate call itself are all still
open.
