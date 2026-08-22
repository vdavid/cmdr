# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, before any model is involved. Change events become per-folder
counters, counters become an interest score, scores become deadlines, and a wake turns whatever is waiting into one
budgeted digest. Depth: `DETAILS.md`.

## Module map

- `coalesce.rs`: events or per-batch rollups into per-folder counters, both through one `Merger` fold.
- `interest.rs`: how much a bundle is worth waking for (`interest`), and how soon (`wake_delay`).
- `compact.rs`: the digest, fitted to a hard token budget, with whatever missed a line rolled up and counted.
- `inbox.rs`: what is waiting, when it comes due, and what a restart does to it.
- `readiness.rs`: whether the agent may watch, and whether it may think.
- `job.rs`: the wake itself — gates, digest, a thread, and one `run_turn`.
- `persist.rs`: the one file here that takes a `Connection` for the inbox. Everything else is values in, values out.

## Must-knows

- **Bundles carry counters, never file names.** Names would grow memory with the EVENT count on exactly the path that
  has to survive five million of them, and would spend digest budget on detail the agent can pull with a `list_dir`
  once it is awake. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Unscored importance is not zero.** `interest` mirrors `WeightLookup`s three-way answer and refuses its `score()`
  collapse: a folder the importance scorer has not reached yet must stay distinguishable from one scored as junk, or a
  project cloned five minutes ago ranks exactly like `node_modules` and the agent silently ignores it.
- **Consent outranks disk access outranks the key**, because each state asks the user for something. Without consent
  the pipeline stores NOTHING; with consent but no key, signal accumulates and waits. None of the three is silence.
- **A merge can only pull a deadline earlier.** Otherwise a folder on a steady trickle has its deadline postponed by
  every new arrival and never comes due, which looks like an agent that is asleep rather than patient.
- **Gotcha: a cold row's deadline is `None`, and no-deadline LOSES every merge.** Merging two `deliver_by`s with
  `Option::min` compiles, reads right, and does the opposite (`None < Some(_)`), so a junk contribution erases a hot
  row's deadline and that folder never wakes. `soonest` is the merge; `next_deadline` and `reconcile` hit the same trap
  from the other side. All three are tested, and `DETAILS.md` says why.
- **Any wake drains the WHOLE inbox.** The expensive part is the model turn, not the row, so cold bundles ride along
  free and a MAX-interest wake policy falls out rather than being written.
- **The tap is a second observer inside `process_live_batch`, placed AFTER rename detection and storm coalescing**, and
  it hands over per-batch rollups. Never a parallel FSEvents subscription, and never per-file messages across the crate
  boundary. Why, and the `downloads/` watcher question: `DETAILS.md`.
- **A wake reuses `run_turn` and opens a real thread** with `ConversationOrigin::Notification`, so the sweep it produces
  points back at reasoning the user can read. Nothing is drained until the turn is certain to run.

What a wake produces: `../suggested_ops/CLAUDE.md`. The store beneath: `../store/proposals/CLAUDE.md`.
