# Size-only subtrees

**Status**: SPECCED and signed off, not started. Blocked on `unindexed-search-plan.md` M5 shipping to `main` first, so
the two efforts don't fight over the same descent rule and walk primitive. **Owner**: David. **Date**: 2026-08-04.
**Baseline**: `main` at `5cfa4ea50`.

**Read first**: `crates/cmdr-index/src/indexing/store/CLAUDE.md` (schema + the epoch model),
`crates/cmdr-index/src/indexing/writer/CLAUDE.md` (the `dir_stats` ledger's four hard rules),
`crates/cmdr-index/src/indexing/reconcile/CLAUDE.md`, `docs/notes/importance-treadmill-2026-08-04.md`.

## The idea

**Cmdr indexes folders. Files are an input to a folder's totals, not a thing worth storing per row.** For most of a
volume both are worth keeping, because the user searches files. For a build-output tree neither search nor navigation
wants the files, yet they dominate the index and almost all of its churn.

A **size-only subtree** stores its directories and their totals, and does NOT store a row per file. It is still
enumerated (that is how the totals stay exact) and still watched (a rebuild still moves its size), but a rebuild writes
one row per changed directory instead of one per changed file.

## Why, measured

On David's machine (`index-root.db`, 2026-08-04):

- **`target/` directories indexed**: 137
- **Files under them**: **982,486**
- **Directories under them**: 58,412
- **Bytes under them**: 1.12 TB
- **Share of the whole 7,086,485-row index**: **13.9%**
- **Share of rescan anchors in an 8 h window**: **93%** (3,438 of 3,704)
- **Importance rows for those paths**: **0**

Two things fall out of those numbers and they shape the design:

1. **The drive index is the entire cost. Importance is already free here**, not because `target` is denylisted (it
   isn't; the list is `node_modules`, `.pnpm-store`, `.git`, `.venv`, ...) but because these live under `.claude/`, and
   `is_hidden_or_system` floors a dot-directory's whole subtree. Don't "optimize" importance for this case; there is
   nothing there.
2. **`target/` is 14% of the rows but 93% of the churn.** Dropping the file rows removes ~94% of the rows in those
   subtrees, and with them the per-file diff, write, and `dir_stats` delta propagation the churn actually pays for.

## The problem this design has to solve first

**FSEvents does not carry old sizes.** It says "this path changed", not "it grew by 4 KB". Today the per-file rows ARE
the memory of the old size: a delete or a modify diffs the live entry against its stored row and propagates the
difference. Delete the file rows naively and the aggregate silently drifts, because there is nothing to subtract.

**The fix is to move the memory up one level: store each directory's OWN totals.** `dir_stats` today is entirely
recursive (`recursive_logical_size`, `recursive_physical_size`, `recursive_file_count`, ...) with nothing direct. Add
the direct counterparts, and a directory row remembers "the bytes and count of my immediate file children".

Then every event resolves the same way, without knowing anything about individual files:

1. An event lands in directory `D`. Re-list `D` (which the reconciler already does).
2. Sum its direct file children => `fresh_direct`.
3. `delta = fresh_direct - stored_direct`, read from `D`'s own row.
4. Write `D`'s new direct totals and propagate `delta` up the ancestor chain, exactly as a per-file delta propagates
   today.

Create, modify, delete, and rename-within-`D` all collapse into that one subtraction. **This is the whole trick**, and
it is why the feature is sound rather than approximate: a per-directory total is a lossless summary of the per-file rows
for everything the aggregate needs.

**Cost per rebuild of a directory**, today versus after:

|                                                | today | size-only       |
| ---------------------------------------------- | ----- | --------------- |
| Enumerate the changed directory                | yes   | yes (unchanged) |
| Diff N file rows against stored                | yes   | **no**          |
| Write changed file rows                        | yes   | **no**          |
| Propagate a `dir_stats` delta per changed file | yes   | **no**          |
| Sum N sizes and compare ONE number             | no    | yes             |
| Write the directory's own row                  | yes   | yes             |

The listing cost is identical; everything after it collapses. That matters because the write is what was measured as
expensive: the live write path costs 34.1 us per row even after the batching that just landed (70.2 us before), and
these subtrees are where the rows come from.

**Hardlinks stay a special case.** The index dedups them today (`db.logical_size.is_none() && snap.nlink > 1` => compare
mtime only), and a direct sum has to keep doing so or a hardlinked file counts once per link. Carry the existing rule
into the summing path; don't let "it's just a sum" hide it.

## What the user keeps and loses

**Keeps:**

- **Disk-space attribution, including drill-down.** Directory rows and their `dir_stats` survive, so
  `target/ -> debug/ -> build/` still shows real recursive sizes. The use case David named as non-negotiable.
- **Live, exact size tracking**, per the mechanism above.
- **Directory names in search.** Only file rows go away, and a size-only directory is an ordinary search hit: it has a
  row, a name, and a `dir_stats` size. Someone searching for `incremental` still finds `target/debug/incremental`.
- **Normal navigation.** Listing a folder reads the filesystem, never the index, so the user still sees their files.

**Loses, deliberately:**

- **Files inside are not searchable** and are invisible to index-backed features.
- **Media enrichment finds no images there**, since it walks file rows. Correct for build output, and named here because
  it is a behavior change rather than an oversight.

## The fan-out objection, and why coalescing answers it

A naive "re-list D on every event" turns today's O(1) single-file update into O(children). For a 20k-file directory
under heavy churn that is a REGRESSION, and it is the first thing to get right.

**Measured on `~/projects-git/vdavid/cmdr/target/` (2026-08-04), the distribution is brutally skewed:**

- **Directories with children**: 13,771
- **Mean fan-out**: **16.4**
- **With >100 children**: 182
- **With >1,000 children**: **9**
- **With >5,000 children**: **2**
- **Max fan-out**: **95,143**

So for 99% of directories the recompute is ~16 entries and the objection does not bite. It bites on **nine
directories**, one of them enormous.

**The answer is to coalesce, not to re-list eagerly.** On an event, mark the directory dirty (O(1), no listing) and
recompute its direct total at most once per window. That is strictly better than today exactly where the worry lives.
For a build touching the 95,143-child directory 500 times in a window:

- **today**: 500 x (stat + row diff + row write + ancestor propagation up ~10 levels), on the order of 5,500 row writes
  at the measured 34 us each, plus 500 stats
- **coalesced**: one bulk listing + one direct-total write + one propagation

A `getattrlistbulk` sweep of ~95k entries is tens of milliseconds against ~190 ms of row writes, so coalescing wins most
on the pathological case. It also matches what the feature is for: disk-space attribution tolerates a size that is
seconds stale, where a file listing would not.

**The window is the knob**, and it is the same "recognize and throttle" shape the reconcile work needs anyway. Size it
from measurement in M3, and log the coalescing ratio so a regression is visible.

## What the size distribution says about the data

Same directory, same day, 211,873 files totalling ~213 GB:

| Size bucket             |      Files |  Total |
| ----------------------- | ---------: | -----: |
| null (hardlink-deduped) | **87,049** |      - |
| 0 bytes                 |      1,733 |      0 |
| <1 KB                   |     42,974 |   8 MB |
| 1-10 KB                 |     16,847 |  68 MB |
| 10-100 KB               |     14,226 | 542 MB |
| 100 KB-1 MB             |     21,387 | 9.9 GB |
| 1-10 MB                 |     25,438 |  52 GB |
| 10-100 MB               |      1,844 |  45 GB |
| >100 MB                 |        375 | 106 GB |

**2,219 files (1%) hold 71% of the bytes**, while 42,974 sub-1-KB files hold 8 MB between them. The row count and the
byte count live in completely different places, which is the clearest possible argument for storing folder totals rather
than file rows.

**41% of files are hardlink-deduped** (null `logical_size`). That makes the dedup rule far more load-bearing for a
direct-sum design than a footnote: get it wrong and a hardlinked artifact counts once per link, on 87,049 files in one
directory tree. The summing path MUST carry the existing rule (`db.logical_size.is_none() && snap.nlink > 1` => mtime
only), and M1 should assert it against a fixture with real hardlinks.

## The model

One flag on the subtree ROOT's `entries` row, inherited downward: this directory and everything beneath it is size-only.

- **Directory rows: stored**, as today.
- **File rows: not stored.**
- **`dir_stats` gains direct totals** (`direct_logical_size`, `direct_physical_size`, `direct_file_count`) for EVERY
  directory, not only size-only ones. Uniform is simpler than conditional, they are cheap, and they make a directory's
  own bytes an O(1) read for everyone.
- **Coverage epochs stay honest.** A size-only directory IS listed, so it stamps `listed_epoch` normally. It must never
  read as "skipped" (`cost_budget.rs`: a skipped dir is one we NEVER listed and must not stamp an epoch). Getting this
  backwards makes the unindexed-search work treat these subtrees as gaps and re-walk them forever.

Stored, not derived, plus a `SCHEMA_VERSION` bump. The index is a disposable cache (`indexing/CLAUDE.md` § "Rebuild,
don't migrate"), so a bump costs a rescan and no migration code.

## How a subtree becomes size-only

A single `SizeOnlyPolicy` seam answering one question: given this directory, should its subtree be size-only? v1 ships
exactly one rule behind it.

**v1 rule: `CACHEDIR.TAG`.** A directory holding a `CACHEDIR.TAG` whose first line is
`Signature: 8a477f597d28d172789f06886806bc55` is declaring itself regenerable cache. It is a cross-tool standard
(honored by borg, restic, `tar --exclude-caches`), and **cargo writes one into every `target/`** (verified on David's
tree, 2026-08-04).

Not a denylist, and the distinction is the point: we read a machine-readable declaration published by the tool that owns
the directory, so software nobody here has heard of gets the same treatment for free. That was David's requirement when
he rejected path denylists.

**Later rules, designed for but NOT built now** (David's list): low importance score, subtree is all binary files,
measured churn, large and never read. Each is a new arm behind the same seam. Keeping the seam narrow in v1 is what
stops this becoming a heuristics project.

**Not in v1**: a user-facing setting, a per-folder override, or UI. Ship the mechanism on one unambiguous signal,
measure it, then decide whether it needs a knob.

## Invariants that must hold

- **The `dir_stats` ledger's four hard rules still apply** (`writer/CLAUDE.md`): never clamp a negative delta; a failed
  `dir_stats` read or write is drift and goes to `deferred_repair`; structural rewrites repair ancestors on the writer;
  suppress propagation only inside `BulkReconcileGuard`. This changes what is stored, never these.
- **A negative direct delta is NORMAL here** (a `cargo clean` empties a directory), so "never clamp" matters more than
  usual: the subtraction must be allowed to go down, and a result that would take an ancestor negative is drift to
  repair, not a number to floor.
- **The reconciler must not read a size-only directory's missing file rows as deletions.** The likeliest silent bug:
  `diff_dir_against_db` sees files live and none stored. It must know the mode BEFORE diffing. Same for the verifier and
  `count_children_capped`.
- **Ancestor aggregates stay exact.** The bytes must still roll up to `~` and `/`, or the feature breaks the use case it
  exists for.
- **Turning the flag OFF repopulates.** A removed `CACHEDIR.TAG` or a policy change must invalidate that subtree's
  coverage so a normal rescan refills the file rows.
- **The flag is inherited by the whole subtree**, so a nested `target/` inside a size-only tree is not a second
  decision. A reader asks the question once, at the marked root, and everything below it answers the same way. Making
  each nested root its own decision would mean a policy re-evaluation per directory and a mode that can flip mid-walk,
  which is cost and complexity for a distinction nobody wants.

## Milestones

**M1 - direct totals, everywhere.** Add the three `dir_stats` columns, populate them on every write path, bump
`SCHEMA_VERSION`. No behavior change yet: the normal path keeps its file rows, and the new numbers are asserted to equal
what a child scan computes. **This is the load-bearing milestone** and it is independently useful, because it makes a
directory's own bytes an O(1) read.

**M2 - the flag and the read path.** Schema column, inheritance, and every reader that must respect it (listing, search,
media walk, verifier, reconciler diff). Mark subtrees only from a test hook; no policy yet.

**M3 - the write path, with coalescing.** Scanner and reconciler skip file rows in a size-only subtree, computing direct
totals and propagating deltas from them. TDD, and the differential check is exact: the aggregate for a size-only subtree
must equal the aggregate the normal path produces for the same directory, including after a create, a modify, a delete,
and a `cargo clean`.

**M4 - the `CACHEDIR.TAG` policy.** The seam plus the one rule, applied at scan and on directory creation. One `stat`
per directory considered, directories only.

**M5 - measure it.** Re-run the table above against a rebuilt index: rows under `target/`, rows written per rebuild, and
CPU per hour under active cargo churn (`scratchpad/cmdr-churn-metrics.csv` holds the before-baseline). Don't call this
done on a row count; the claim is about CPU under churn.

## Searching inside a size-only subtree

`unindexed-search-plan.md` builds a walk that finds files on ground the index does not cover, and a size-only subtree is
exactly that ground. The two features compose, and this is where the "loses: files inside are not searchable" line above
gets softened: **the files stay findable, they are just found live rather than from the index.**

**The coverage machinery does not notice on its own, and that is the thing to get right.** A size-only directory stamps
`listed_epoch` normally (§ The model), and `min_subtree_epoch` is a min over child DIRECTORIES only, so the descent rule
reads a size-only subtree as **covered** and never descends. Left alone, that means search quietly returns nothing from
these trees rather than re-walking them, which is the cheap failure but still a wrong answer.

**The fix is the per-dimension coverage the search plan already designed for.** Its § "Forward compatibility with
content search" requires that "M2's coverage API must not assume a single dimension", for exactly this shape of problem.
A size-only subtree is **directory-covered and file-uncovered**, so the descent rule gains one arm returning a node that
is served from the index for directories and handed to the walk for files. No new epoch column is needed: the inherited
flag IS the answer, read once at the marked root.

**The walk is the cheap half of the search plan's walk.** It writes nothing, and nearly every hard problem in that
plan's M3a exists because its walk writes: cancellation leaving durable partial coverage, `DeleteDescendantsById`,
ordering marks after inserts, and the bounded writer channel throttling discovery. A file-matching walk over a size-only
subtree has none of them. It reuses the walker, drops the insert visitor, and emits batches to the matcher over the
channel that plan's Decision 3 already defines.

**Low ranking falls out; it does not need building.** Importance weights come from the index, and these subtrees have
zero importance rows by construction, so live-walked hits already rank by match quality and recency alone (that plan's
Accepted difference 6). Worth a deliberate demotion on top, since the reason a subtree is size-only is that nobody wants
its files, but the default is already the right shape.

**Two things this costs, both to state rather than solve:**

- **It never converges.** That plan's payoff property is that every search durably shrinks the frontier. A size-only
  subtree is a PERMANENT file-frontier by design, so the walk cost recurs on every search that reaches it. That is the
  honest trade for not storing a million rows, and it belongs in that plan's § Accepted differences.
- **Scope decides whether it is reached.** A search of `~` should not pay to enumerate every `target/` beneath it.
  Recommendation: walk a size-only subtree only when the scope is INSIDE one. Scoping a search into `target/` is an
  unambiguous statement that you want those files; searching your home folder is not. Cheap, needs no setting, and
  matches how someone actually reasons about it.

**Sequencing**: this arm depends on that plan's M5 (coverage-pruned streaming search) being shipped, so it lands after
it rather than alongside. Until it does, the honest interim behavior is the "loses" line as written: files in a
size-only subtree are not searchable, and the search UI's existing coverage note is what says so.

## Risks

- **Aggregate drift is the failure mode that matters**, and it is silent. Every milestone touching the delta path needs
  a test that a sequence of create/modify/delete/clean leaves ancestor totals equal to a fresh full recompute. The
  differential oracle already exists in spirit (importance's two-walk harness); mirror it here.
- **A schema bump forces a full rescan** (~10 min on a big NAS). Acceptable per "rebuild, don't migrate", and this ships
  ALONE rather than waiting to ride along with other schema work: there is no other schema change queued, so pairing
  would mean holding the feature for a hypothetical partner.
- **`CACHEDIR.TAG` is not universal.** Plenty of churny directories publish nothing, so this does not remove the need
  for an arrival-rate governor; it removes the biggest single case cheaply.
- **Interaction with `unindexed-search-plan.md`.** A size-only subtree reads as COVERED to that plan's descent rule (see
  § "Searching inside a size-only subtree"), so search silently returns nothing from these trees rather than re-walking
  them. Cheap, but a wrong answer, and invisible unless someone goes looking. Resolve when that plan's M5 ships.
