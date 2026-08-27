# What the importance subsystem still owes

The folder-importance subsystem shipped as its own neutral thing: `crates/cmdr-index/src/importance/` scores every
folder on a Local or SMB volume from index rows alone, stores the scalar plus its raw signal vector in a disposable
per-volume `importance.db`, recomputes fully on scan completion and incrementally on live listing changes, and answers
`weight_for` / `top_n` / `above_threshold` / `explain` through one read API, even for an unmounted volume. Its own docs
are the canonical account: `crates/cmdr-index/src/importance/CLAUDE.md` routes to five area doc pairs (`scorer/`,
`store/`, `scheduler/`, `read/`, `evals/`), and the lifecycle bus it rides lives in
`crates/cmdr-index/src/indexing/DETAILS.md`.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## 1. The weights are still a guess, and the instrument that would fix that is built

**The gap**: the scorer's coefficients are defaults nobody has tuned against a real tree. Every consumer (summary
gating, event-bundle interest, media enrichment order) inherits whatever they rank.

**What already exists**: the whole tuning loop. `evals/` holds the scenario format, the hard and soft constraint tiers,
and a corpus importer that derives signals through PRODUCTION code, so a dump scores identically to the live volume.
Three dev bins drive it (`importance-tune` to eyeball a ranking with `explain` breakdowns, `importance-snapshot` to dump
an anonymized scenario, `importance-measure` for the cost side), and `docs/guides/importance-evals.md` is the
David-facing how-to.

**Why it hasn't happened**: real dumps land in a gitignored corpus dir and are never committed, so CI runs with zero
corpus files and the suite proves the shape holds, not that the ranking is good. Closing this needs David's own home
directory on his own machine.

**The guardrail that survives either way**: `SOFT_SCORE_FLOOR` is a FIXED floor. A tuning pass that genuinely improves
quality raises it consciously in the same commit; ❌ never lower it to make a change pass (`evals/CLAUDE.md`).

## 2. `SAMPLE_CAP` has never been measured on a real home

**The gap**: `last_used.rs` samples `kMDItemLastUsedDate` for at most 500 folders per pass, on a dedicated 8 MB-stack OS
thread inside an autoreleasepool. Both the cap and the sample strategy are guesses, and `importance/DETAILS.md` §
"Sampled `kMDItemLastUsedDate`" says so outright.

**Cost**: a measurement, not a fix. `importance-measure` already reports a full pass's phase wall-clock split, so the
missing number is what the sampling phase costs against a real Spotlight index at a few cap values.

**Bounded by construction**: sampling runs only where the volume mask says `last_used_available`, so SMB never pays it
and the cost is confined to the boot disk.

⚠️ **Not the same work as `indexing-loose-ends.md` item 3.** That item wants an app-side `mdfind` at LAUNCH to seed
`priority_roots` on a true first run, before any index exists. This one is about what the in-crate sampler costs once
one does. Don't merge them.

## 3. A recompute can't be stopped

**The gap**: every other long walk in the crate runs under a `CancellationToken` rooted at the volume. An importance
pass runs under nothing and registers no stop hook, so `stop_all_indexing` (the memory watchdog's emergency stop AND the
shutdown path) doesn't reach it: a running pass walks the whole index to the end regardless.

**Why it hasn't hurt**: the full walk is seconds, not minutes (5.5–6.4 s over real 391k- and 611k-folder indexes,
measured 2026-07-29), and an incremental is microseconds. Seconds of unstoppable work inside an emergency stop is
survivable where a scan's minutes would not be.

**The fix shape is already written down**, next to the `TODO(importance)` it belongs to:
`crates/cmdr-index/src/importance/scheduler/DETAILS.md` § "A pass can't be stopped". Thread a child of the volume's
token in from whoever starts the pass and register a stop hook. ❌ Don't introduce a second primitive; the one-token
tree is what makes stopping a volume stop everything under it at once.

**Trigger**: a pass that stops being seconds, or a memory watchdog stop that visibly fails to free anything.
