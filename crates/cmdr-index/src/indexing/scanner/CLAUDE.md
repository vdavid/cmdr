# Local guarded scanner

The LOCAL fresh-scan directory walker (boot disk + `LocalExternal`), built to survive a hung `readdir` on a disconnected
File Provider mount, plus the exclusion policy every local path shares.

## Module map

- **mod.rs** — the scan driver: `scan_volume` / `scan_subtree` / `cover_subtree`, `ScanRoot` + `WalkPolicy`, the `Scan*`
  types, `LOCAL_LIST_TIMEOUT` (15 s).
- **insert_visitor.rs** — the per-directory half: each read's children become rows, attributed via the carried `dir.id`
  (no path→id map), on worker threads, so its state is behind mutexes.
- **walker/** — the hang-tolerant engine (the watchdog, the progress-timeout verdict, the give-up budget) + `bulk_read`
  (`getattrlistbulk`, macOS). Only `bulk_read_dir_unwatched` + `RawFileType` are re-exported, for the serial reconcile
  walk.
- **exclusions.rs** — the two-tier `should_exclude(path, &ExclusionScope)` policy, the single gate for scanner,
  reconcile, watch verification, and the verifier.

## Must-knows

- **Never rayon.** Workers are dedicated 8 MB-stack OS threads: File Provider reads descend XPC chains that overflow
  rayon's 2 MB stack.
- **The walker abandons a read that STOPPED PRODUCING (stalled 15 s), never a merely long one.** ❌ Never re-cap total
  duration: elapsed time can't tell a 200,000-entry dir from a dead mount (that cap once dropped 661,411 rows). A read
  that can't report progress falls back to it.
- **Subtree give-up after 32 consecutive failed reads** (sticky per dir, reset by a successful sibling). Throttle, not
  exclude: no path denylist.
- **A COVER walk carries a `WalkHeartbeat`**, stamped as each read STARTS (batches fill at 2 000 entries, so progress
  derived from them reads as zero) and totalling the give-ups. Partial batches go out after 100 ms (`live_emit.rs`),
  from the push path AND the watchdog tick — the only thing still moving when a walk parks. ❌ Don't drop the tick, ❌
  don't shrink the batch, ❌ no third cadence.
- **Honest-stale, never false-complete.** An abandoned or give-up-pruned dir is NEVER marked listed, so it stays
  `listed_epoch = 0` (unknown size, row intact). PERMISSION DENIED also gets `unreadable_cause = Denied`; a TIMEOUT
  doesn't, since dead mounts heal. `mark_dirs_listed` clears it.
- **Marks ride WITH their rows, inside `Pending`'s lock.** A mark is a PK `UPDATE` and a dir's row is written by its
  PARENT, so an overtaking mark means `listed_epoch = 0` forever. ❌ Don't split that mutex, ❌ don't send outside it,
  ❌ don't stop `finish()` flushing marks on CANCEL.
- **`ScanRoot::Virgin` (the search walk) DELETES NOTHING and refuses a root with children** (`ScanError::NotVirgin` →
  serial reconcile repairs it). A frontier node can sit above covered ground, and add-only over existing rows is worse:
  `INSERT OR IGNORE` drops the collider and orphans its subtree. DETAILS § "Three roots".
- **`bulk_read` degrades, it doesn't drop**: a missing attribute yields `stat: None` and the caller stats that child. ❌
  Never report a size the parser didn't read.
- **`should_exclude` derives scope from the volume KIND, never `is_volume_root`.** Tier (a) absolute prefixes apply ONLY
  under `BootDisk`; on a mount-rooted scan they'd exclude every child → zero rows → falsely Fresh.
- **`WalkPolicy` = what a walk won't descend into**: that scope, an on/off switch (`Volume`/`Virgin` apply, `Rebuild`
  doesn't), and the device `Virgin` pins. Either cut writes NO ROW — an unlisted row sits in the frontier forever. ⚠️
  Pin = the WALK root's device; ❌ File Provider domains are NOT a boundary (Decision 16).
- **The pseudo-fs trio (`proc`, `sys`, `dev`) is skipped only at a corroborated volume root**: root POSITION AND all
  three present as siblings, since a name-only rule would drop a user's `.../Dropbox/dev`.

The progress-timeout rules, the give-up budget, `WalkPolicy`, and the exclusion tiers: `DETAILS.md`. Read it before any
non-trivial work here.
