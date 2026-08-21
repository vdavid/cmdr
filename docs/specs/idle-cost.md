# Cmdr should cost almost nothing while you're not using it

**Problem**: an idle prod build burned 110 minutes of CPU over 9.1 hours (about 20% of a core, sustained) at a 1.78 GB
footprint. A file manager sitting in the background is competing with the work the user actually opened their laptop
for, which is principle 5 ("respect the user's resources") failing in the most visible way there is: a fan.

**Size**: about a day of build work that starts the moment one command confirms the first item, plus a second item that
can't be ranked until a week of passive observation has run. Neither is a start-coding-tomorrow item, and the reason is
measurement rather than design.

**Read first**: `docs/notes/idle-cpu-attribution-2026-08-03.md`. Four successive hypotheses about where this CPU goes
were refuted by measurement, and each refutation is recorded there along with the rules it leaves behind: never rank
work off one `sample` window, never report a share as CPU when the leaf frame is a syscall, never infer CPU from log
volume. ❗ Don't re-derive a number from a stack profile here without reading it first.

⚠️ The 110-minute figure is a v0.37.0 measurement taken on 2026-08-03 and hasn't been re-taken since, on a machine that
was also running six cargo builds. Take a fresh baseline on a quiet machine before ranking anything below against it.

## The work

Two items, independent of each other, and both currently waiting on a measurement rather than on a decision or on
effort.

### 1. Stop paying for a CLIP text tower that background work never calls

The largest single number left in the profile. Core ML holds Cmdr's two CLIP towers through the SYSTEM allocator, which
is why three investigations with mimalloc-based tooling were blind to it. Per-tower and per-compute-unit measurements,
the method that found them, and the reusable diagnostic surface it left behind:
`docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`.

What sizes the work:

- The image tower alone costs 64.6 MB of `MALLOC_LARGE`; the text tower alone costs 251.5 MB, because it ships fp32
  while the image tower ships 8-bit palettized.
- `load_towers` (`crates/cmdr-index/src/media_index/clip/macos.rs`) loads BOTH whichever one the caller wants, and
  `WORKER` there is a `OnceLock`. So the first encode of a session pins both towers for the process lifetime, whether or
  not the user switches semantic search off afterwards.
- **An enrichment pass therefore pays 251.5 MB for a text tower it never calls.** That gap is the win.

⚠️ **This is gated on a confirmation, and the confirmation is one command.** Nobody has shown the towers were loaded in
the specific prod run that produced the 643 MB, and 230–340 MB of that number stays unattributed either way. The
discriminator is under § "One command settles it" in the note, it needs no app support so a shipped release build
answers it, and it takes seconds on David's laptop. Until it comes back positive this is a strong lead rather than a
settled cause. ❌ Don't build a fix aimed at a number nobody has confirmed: that is the exact failure
`idle-cpu-attribution-2026-08-03.md` exists to prevent.

The fixes, in the order to take them:

1. **Load each tower on demand, separately.** About a day. They're independently loadable today, so this is plumbing
   with no quality question and no inference-speed question attached, which is why it goes first. A user who never types
   a semantic search stops holding 251.5 MB for one.
2. **Unload after idle.** A tower nobody has asked anything of in N minutes could drop and reload in the one to two
   seconds a cold Core ML load costs. Whether that reload can land inside a user's typing latency is a product call, so
   ask before building.
3. **Reconsider `MLComputeUnits`.** Worth roughly 400 MB: the GPU path is what makes Core ML copy every weight matrix
   instead of reading them from the mmap'd `weight.bin`. ❌ Don't touch it on the memory number alone. The enrichment
   throughput cost of dropping the GPU is unmeasured, enrichment speed is a real user-facing property, and
   `crates/cmdr-index/src/media_index/clip/CLAUDE.md` now carries that as an invariant. Measuring both sides is its own
   task.
4. **Convert the text tower to fp16.** A spike with a quality question attached. `install.rs` records why 8-bit was
   rejected (the text tower's 8-bit Core ML inference comes out all-NaN); fp16 sits between the two and was never tried.

### 2. Bound the reconcile drain's arrival rate

**The next step here is a measurement, not a build**, and the item is ranked lower than it used to be.

The problem is real: nothing rate-limits rescan-anchor arrivals, and the per-anchor throttle contributes nothing to
bounding them, because cargo's anchors are one-shot and `is_eligible`
(`crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/throttle.rs`) returns `true` unconditionally the first time
it sees a path. The cost therefore scales with the user's workload rather than with anything Cmdr controls. Three things
say it still isn't ready to rank:

- **It has been demoted once already.** "The reconcile drain is the one that moves the CPU number" was wrong answer one
  in `docs/notes/idle-cpu-attribution-2026-08-03.md`, refuted by measurement: roughly 466 s of reported walking over
  eight hours, largely IO wait, at the 16–23% CPU that class of walk actually costs.
- **The recommended shape bounds the cost at roughly the same cost.** Shape (d) below routes to the visible scanner,
  whose sweep is measured at 1,309 s and runs at most once a day
  (`crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/route.rs`, on `SHALLOW_RESCAN_MIN_INTERVAL`). What it
  buys is predictability and a bounded worst case, paid for with up to 24 hours of whole-volume staleness. That's a real
  thing to want, and it isn't the CPU win this item was once ranked as.
- **The 3,704-anchors figure that motivated it appears nowhere except this spec.** No note backs it, so nobody can
  re-derive it or say what the machine was doing, and the attribution note's standing rule is to never rank work off one
  window. 93% of those anchors sat under `.claude/worktrees/*/target` on a machine running six cargo builds, and that
  same note asks for a quiet-machine sanity check on anything measured there.

**The signal that settles it already exists and reaches nothing.**
`crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/churn.rs` accumulates per-anchor walks and cost over a
15-minute window, with a walk budget, a row budget, and a 64-anchor cap. It's pure and clock-injected, it emits at most
one INFO line per window and only when a budget is crossed, and a quiet machine stays silent forever. It feeds a reader
today and nothing else.

So the next step is: **run an ordinary week on a quiet machine, collect the churn lines, write the numbers into a
`docs/notes/` note, then choose a shape.** Half a day of work once the week has passed. The week is the cost.

## Choosing a shape, once there are numbers

Four mutually exclusive candidates. The analysis holds; what changed is that it's a menu to pick from after measuring
rather than a decision to take now.

- **(a) A volume-wide duty-cycle budget** (about 3% of wall clock). Makes the hourglass flicker volume-wide, because
  `crates/cmdr-index/src/indexing/reconcile/reconciler/rescan/hold.rs` re-derives every queued anchor's hold on the
  roughly one-second sweep and a held root drags its chain to `/`. Also a blind window on external volumes, the one kind
  with no verifier cover.
- **(b) A per-subtree budget at a fixed depth.**
  `crates/cmdr-index/src/indexing/reconcile/local_reconcile/cost_budget.rs` argues explicitly against charging a read up
  its whole ancestor chain, and its `ANCHOR_DEPTH` of 5 anchors at `~/projects-git/vdavid/cmdr`, which is exactly the
  subtree that `a_subtree_with_a_low_slow_read_fraction_is_never_refused_however_large_it_grows` exists to protect.
- **(c) Spike B's churn-share plus content-ratio climb.** The spike's own authors wanted a `~/Library/Containers` and
  `~/Library/Caches` hard-stop list to make it safe, so the over-climb risk is unmitigated until the exclusion question
  below is answered.
- **(d) Treat high anchor cardinality the way `route.rs` already treats `MustScanSubDirs` on `/`**: route to the visible
  scanner with a once-a-day window and a green badge.

**(d) still looks best.** It reuses a shipped mechanism and a shipped user-facing story (coalesced signals get counted
and surfaced in the volume tooltip, and the badge deliberately stays green because once-a-day sweeping is the designed
operating state), and it's the only one of the four that fights neither `hold.rs` nor `cost_budget.rs`. Read that
alongside what it does and doesn't buy, above.

### The exclusion question, stated correctly

Three of the four shapes used to be pruned as colliding with "no denylists and no path-shaped exclusions". That framing
is wrong about what's in the tree, and it prunes an option it shouldn't:

- `SYSTEM_DIR_EXCLUDES` (`crates/cmdr-index/src/indexing/scanner/exclusions.rs`) is a shipped name denylist. It contains
  `target`, `node_modules`, and `Caches`, and its own docstring calls it "the indexer's policy, read by three consumers
  so they can't drift": search, the importance scorer, and the folder-size tooltip.
- `importance::classify::floors_by_path` already applies it, plus dotfile and system classification, to drop exactly
  this churn out of the importance path.
- So the question to put to David is **"may the rescan walk be a fourth consumer?"**, and not "may we have a denylist".
- The carve-out in that same docstring says the SCANNER must never be one, because skipping at walk time stamps coverage
  on parents whose `dir_stats` then come out short. ⚠️ That reason doesn't obviously transfer to RE-walking an
  already-indexed subtree, where the aggregates already exist and the cost of skipping is staleness rather than a wrong
  count. Which makes this a real question rather than a settled no.

### Two riders

- **Whether a budget-refused subtree's staleness is visible to the user or silent.** Still open, and it only starts
  mattering once a shape is chosen.
- **How this relates to `later/indexing/sealed-subtrees-plan.md`.** Likely a non-decision: `index.md` records that
  plan's M2–M5 as not started and probably never needed, gated behind measured residual pain, so there's nothing to
  reconcile until one of them exists.

## Smaller open calls this effort produced

- **Two `invariant-density` warns, waiting on a yes or no.** `apps/desktop/src-tauri` is at 343 against an allowlist of
  342, which is pre-existing on `main` and not from this work; `crates/cmdr-index` is at 372 against 371, which is the
  one ❌ in `crates/cmdr-index/src/media_index/clip/CLAUDE.md` saying not to change `MLComputeUnits` on the memory
  number alone. Neither allowlist was touched. The options are bumping them or trimming an invariant elsewhere, and both
  are David's call.
- **`ImportanceIndex::above_threshold(0.0)`'s `ORDER BY` may be dead weight.** Its cached consumer
  (`crates/cmdr-index/src/media_index/coverage/scores.rs`) builds a `HashMap` and never reads the order, but the public
  API's ordering is asserted by a test and other callers exist, so this is a decision rather than a cleanup. It matters
  slightly more than it looks, because `cache_size` also sets SQLite's sorter budget
  (`crates/cmdr-fs/src/sqlite_util.rs`).
- **`derive-default-justified` scans only the filesystem trees**
  (`scripts/check/checks/desktop-rust-derive-default-justified.go`), so a `Default` derive on an IPC DTO goes
  unchallenged while its `cmdr-fs` twin needs a `DEFAULT-OK` line. Possibly unintentional; worth one look.
- **What needs David's laptop.** The CLIP discriminator command above, plus the SMB-Docker-backed Rust lanes and the E2E
  and website lanes. The headless agent box can't run any of them, because Docker is deliberately absent there.
