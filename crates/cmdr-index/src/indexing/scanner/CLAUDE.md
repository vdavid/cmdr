# Local guarded scanner

The LOCAL fresh-scan directory walker (boot disk + `LocalExternal`), built to survive a hung `readdir` on a disconnected
File Provider mount, plus the exclusion policy every local path shares.

`mod.rs` drives a scan (`ScanRoot` + `WalkPolicy`, `LOCAL_LIST_TIMEOUT`); `insert_visitor.rs` turns each read's children
into rows on worker threads, attributed via the carried `dir.id`; `walker/` is the hang-tolerant engine plus `bulk_read`
(`getattrlistbulk`, macOS); `exclusions.rs` is the single `should_exclude` gate for scanner, reconcile, watch
verification, and the verifier.

## Must-knows

- **Never rayon.** Workers are dedicated 8 MB-stack OS threads: File Provider reads descend XPC chains that overflow
  rayon's 2 MB stack.
- **The walker abandons a read that STOPPED PRODUCING, never a merely long one.** ❌ Never re-cap total duration:
  elapsed time can't tell a 200,000-entry dir from a dead mount (that cap once dropped 661,411 rows).
- **Subtree give-up after 32 consecutive failed reads**, sticky per dir, reset by a successful sibling. Throttle, not
  exclude: no path denylist.
- **A COVER walk carries a `WalkHeartbeat`**, stamped as each read STARTS, since batches only fill at 2 000 entries.
  Partial batches go out after 100 ms, from the push path AND the watchdog tick — the only thing still moving when a
  walk parks. ❌ Don't drop the tick, shrink the batch, or add a third cadence.
- **Honest-stale, never false-complete.** An abandoned or give-up-pruned dir is NEVER marked listed, so it stays
  `listed_epoch = 0`. PERMISSION DENIED also gets `unreadable_cause = Denied`; a TIMEOUT doesn't, since mounts heal.
- **Marks ride WITH their rows, inside `Pending`'s lock** — an overtaking mark means `listed_epoch = 0` forever. ❌
  Don't split that mutex, send outside it, or stop `finish()` flushing marks on CANCEL.
- **`ScanRoot::Virgin` (the search walk) DELETES NOTHING and refuses a root with children** (`ScanError::NotVirgin` →
  serial reconcile repairs it). Add-only over existing rows is worse: `INSERT OR IGNORE` drops the collider and orphans
  its subtree. DETAILS § "Three roots".
- **`bulk_read` degrades, it doesn't drop**: a missing attribute yields `stat: None` and the caller stats that child. ❌
  Never report a size the parser didn't read.
- **`should_exclude` derives scope from the volume KIND, ❌ never `is_volume_root`.** Tier (a) absolute prefixes apply
  ONLY under `BootDisk`; on a mount-rooted scan they'd exclude every child → zero rows → falsely Fresh.
- **`WalkPolicy` = what a walk won't descend into**: that scope, applied by EVERY walk, plus the device `Virgin` pins.
  Either cut writes NO ROW, and an unlisted row sits in the frontier forever. ⚠️ Pin = the WALK root's device; ❌ File
  Provider domains are NOT a boundary (Decision 16).
- **The pseudo-fs trio (`proc`, `sys`, `dev`) is skipped only at a corroborated volume root** (root POSITION and all
  three as siblings): a name-only rule would drop a user's `.../Dropbox/dev`.

The progress-timeout rules, the give-up budget, `WalkPolicy`, and the exclusion tiers: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
