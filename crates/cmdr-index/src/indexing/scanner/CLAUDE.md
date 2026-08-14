# Local guarded scanner

The LOCAL fresh-scan directory walker (boot disk + `LocalExternal`), plus the exclusion policy every local path shares.

`mod.rs` drives a scan (`ScanRoot` + `WalkPolicy`, `LOCAL_LIST_TIMEOUT`); `insert_visitor.rs` turns each read's children
into rows on worker threads, attributed via the carried `dir.id`; `walker/` is the hang-tolerant engine and its own
guardrails (`walker/CLAUDE.md`); `exclusions.rs` is the single `should_exclude` gate for scanner, reconcile, watch
verification, and the verifier.

## Must-knows

- **A COVER walk carries a `WalkHeartbeat`**, stamped as each read STARTS, since batches only fill at 2 000 entries.
  Partial batches go out after 100 ms, from the push path AND the watchdog tick — the only thing still moving when a
  walk parks. ❌ Don't drop the tick, shrink the batch, or add a third cadence.
- **Honest-stale, never false-complete.** An abandoned or give-up-pruned dir is NEVER marked listed, so it stays
  `listed_epoch = 0`.
- **Every failed read gets an `unreadable_cause`**: `Denied` for permission denied, `Abandoned` for a timeout, any other
  errno, and a give-up-pruned task (via `DirVisitor::visit_pruned`, a pruned task's only mention anywhere). ❌ Don't
  "let a transient failure heal" by leaving one uncaused: that's a full stall timeout per dir on EVERY search, forever,
  and it never converges. The retry lives in `../writer/abandoned_retry.rs` on a backoff instead.
- **Marks are sent AFTER `visitor.finish()`, and only for ids a read actually failed on.** ❌ Never derive them from
  "whatever is still unlisted": that condemns rows the walk read but hasn't stamped, which looks like a speed-up and
  shows up only as a short entry count (`marking_abandoned_ground_costs_no_coverage`).
- **Marks ride WITH their rows, inside `Pending`'s lock** — an overtaking mark means `listed_epoch = 0` forever. ❌
  Don't split that mutex, send outside it, or stop `finish()` flushing marks on CANCEL.
- **`ScanRoot::Virgin` (the search walk) DELETES NOTHING and refuses a root with children** (`ScanError::NotVirgin` →
  serial reconcile repairs it). Add-only over existing rows is worse: `INSERT OR IGNORE` drops the collider and orphans
  its subtree. DETAILS § "Three scan roots".
- **`should_exclude` derives scope from the volume KIND, ❌ never `is_volume_root`.** Tier (a) absolute prefixes apply
  ONLY under `BootDisk`; on a mount-rooted scan they'd exclude every child → zero rows → falsely Fresh.
- **`WalkPolicy` = what a walk won't descend into**: that scope, applied by EVERY walk, plus the device `Virgin` pins.
  Either cut writes NO ROW, and an unlisted row sits in the frontier forever. ⚠️ Pin = the WALK root's device; ❌ File
  Provider domains are NOT a boundary (Decision 16).
- **The pseudo-fs trio (`proc`, `sys`, `dev`) is skipped only at a corroborated volume root** (root POSITION and all
  three as siblings): a name-only rule would drop a user's `.../Dropbox/dev`.

Three scan roots, `WalkPolicy`, the emit cadence, marks, and the exclusion tiers: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
