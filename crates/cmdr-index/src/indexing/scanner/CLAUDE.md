# Local guarded scanner

The LOCAL fresh-scan directory walker (boot disk + `LocalExternal`), built to survive a hung `readdir` on a disconnected
File Provider mount, plus the scope-aware exclusion policy every local path shares.

## Module map

- **mod.rs** — the scan driver: `scan_volume` / `scan_subtree` / `cover_subtree`, `run_scan`, `ScanRoot` + `WalkPolicy`,
  the `Scan*` types, `LOCAL_LIST_TIMEOUT` (15 s).
- **insert_visitor.rs** — the per-directory half: `InsertVisitor` turns each read's children into rows, attributed via
  the carried `dir.id` (no path→id map). Runs on worker threads, so its state is behind mutexes.
- **walker/** — the hang-tolerant engine (`walk`, the watchdog, the progress-timeout verdict, the give-up budget) +
  `bulk_read` (`getattrlistbulk`, macOS). `bulk_read_dir_unwatched` + `RawFileType` are re-exported for the serial
  reconcile walk; the engine and `RawDirEntry` stay private.
- **exclusions.rs** — the two-tier `should_exclude(path, &ExclusionScope)` policy, the single gate for scanner,
  reconcile, watch verification, and the verifier, plus `ExclusionMode`.

## Must-knows

- **Never rayon.** Workers are dedicated 8 MB-stack OS threads: File Provider reads descend XPC override chains that
  overflow rayon's 2 MB stack.
- **The walker abandons a read that STOPPED PRODUCING (stalled 15 s, judged by `ReadProgress`), never a merely long
  one.** ❌ Never re-cap total duration: elapsed time can't tell a 200,000-entry dir from a dead mount (a total cap
  dropped 661,411 rows once). A read that can't report progress falls back to the plain total cap.
- **Subtree give-up after `DEFAULT_GIVE_UP_AFTER` (32) consecutive failed reads** (sticky per dir; a successful sibling
  resets it). Throttle, not exclude: a healthy provider is fully indexed, no path denylist.
- **Honest-stale, never false-complete.** An abandoned or give-up-pruned dir is NEVER marked listed, so it stays
  `listed_epoch = 0` (unknown size, `EntryRow` intact); never zeroed, never `scan_completed_at`-marked. PERMISSION
  DENIED also gets `known_unreadable`; a TIMEOUT doesn't, since dead mounts heal. `mark_dirs_listed` clears it.
- **Marks ride WITH their rows, inside `Pending`'s lock.** A mark is a PK `UPDATE` and a dir's row is written by its
  PARENT, so an overtaking mark means `listed_epoch = 0` forever. ❌ Don't split that mutex, ❌ don't send outside it,
  ❌ don't stop `finish()` flushing marks on CANCEL. DETAILS § "Marks ride".
- **`ScanRoot::Virgin` (`cover_subtree`, the search walk) DELETES NOTHING and refuses a root with children**
  (`ScanError::NotVirgin` → serial reconcile repairs it). A frontier node can sit above covered ground; add-only over
  existing rows is worse (`INSERT OR IGNORE` drops the collider, orphaning its subtree). DETAILS § "Three roots".
- **`bulk_read` degrades, it doesn't drop.** A missing attribute (or a sizeless type: fifo, socket, device) yields
  `stat: None` and the caller stats that child; only a nameless record is dropped, counted by `BulkDirRead::unusable`.
  ❌ Never report a size the parser didn't read.
- **`should_exclude` derives scope from the volume KIND, never `is_volume_root`.** Tier (a) absolute prefixes apply ONLY
  under `BootDisk`; on a mount-rooted scan they'd exclude every child → zero rows → falsely Fresh.
- **`WalkPolicy` = what a walk won't descend into**: that scope, an on/off switch (`Volume`/`Virgin` apply, `Rebuild`
  doesn't), and the device `Virgin` pins. Either cut writes NO ROW — an unlisted row sits in the frontier forever. ⚠️
  Pin = the WALK root's device (else a walk rooted in a mount false-completes); ❌ File Provider domains are NOT a
  boundary (Decision 16).
- **The pseudo-fs trio (`proc`, `sys`, `dev`) is skipped only at a corroborated volume root**: root POSITION AND all
  three present as siblings. A name-only rule would drop a user's `.../Dropbox/dev`. The domain-root probe is an
  OPTIMIZATION, never the cost backstop (that's `reconcile/`).

Architecture, the progress-timeout rules, the give-up budget, `WalkPolicy`, exclusion tiers, and domain-root detection:
`DETAILS.md`. Read it before any non-trivial work here.
