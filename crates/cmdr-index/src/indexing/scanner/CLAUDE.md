# Local guarded scanner

The LOCAL fresh-scan directory walker (boot disk + `LocalExternal`), built to survive a hung `readdir` on a disconnected
File Provider mount, plus the scope-aware exclusion policy every local code path shares.

## Module map

- **mod.rs** — the scan driver: `scan_volume` / `scan_subtree` / `cover_subtree` entry points, `run_scan`, the `Scan*`
  types, and `LOCAL_LIST_TIMEOUT` (15 s).
- **insert_visitor.rs** — the per-directory half: `InsertVisitor` turns each read's children into rows, attributed to
  their parent via the carried `dir.id` (no path→id map). Runs on worker threads, so its state is behind mutexes.
- **walker/** — the hang-tolerant engine (`walk`, the watchdog, the progress-timeout verdict, the subtree give-up
  budget) + `bulk_read` (`getattrlistbulk` batch reads on macOS), whose `bulk_read_dir_unwatched` + `RawFileType` are
  re-exported at `scanner` level for the serial reconcile walk, macOS-only like the reader itself. The engine stays
  private, and `RawDirEntry` with it.
- **exclusions.rs** — the two-tier `should_exclude(path, &ExclusionScope)` policy (the single exclusion gate for
  scanner, reconcile, watch verification, and the verifier).

## Must-knows

- **Never rayon.** Workers are dedicated 8 MB-stack OS threads: File Provider reads descend deep XPC override chains
  that overflow rayon's 2 MB stack.
- **The walker abandons a read that STOPPED PRODUCING (stalled 15 s, judged by `ReadProgress`), never a merely long
  one.** ❌ Never re-cap total duration: elapsed time can't tell a 200,000-entry dir from a dead mount (a total cap
  dropped 661,411 rows once). A read that can't report progress falls back to the plain total cap.
- **Subtree give-up after `DEFAULT_GIVE_UP_AFTER` (32) consecutive failed reads** (sticky, shared by one dir's children;
  a successful sibling resets it). Throttle, not exclude: a healthy provider is fully indexed, no path denylist.
- **Honest-stale, never false-complete.** An abandoned or give-up-pruned dir is NEVER marked listed, so it stays
  `listed_epoch = 0` (unknown size, its `EntryRow` still exists); it's never zeroed and never
  `scan_completed_at`-marked. PERMISSION DENIED also gets `known_unreadable` (so the frontier stops re-offering it); a
  TIMEOUT doesn't, since dead mounts heal. `mark_dirs_listed` clears it.
- **Marks ride WITH their rows, inside `Pending`'s lock.** A mark is a PK `UPDATE` and a dir's row is written by its
  PARENT, so an overtaking mark means `listed_epoch = 0` forever. ❌ Don't split that mutex, ❌ don't send outside it,
  ❌ don't stop `finish()` flushing marks on CANCEL. DETAILS § "Marks ride".
- **`ScanRoot::Virgin` (`cover_subtree`, the search walk) DELETES NOTHING and refuses a root with children**
  (`ScanError::NotVirgin` → serial reconcile repairs it). A frontier node can sit above covered ground; add-only over
  existing rows is worse still (`INSERT OR IGNORE` drops the collider, orphaning its subtree). DETAILS § "Three roots".
- **`bulk_read` degrades, it doesn't drop.** A missing attribute (or a type carrying no sizes: fifo, socket, device)
  yields `stat: None` and the caller stats that child. Only a record with no recoverable name is dropped, counted by
  `BulkDirRead::unusable` — ❌ never report a size the parser didn't read.
- **`should_exclude` derives scope from the volume KIND, never `is_volume_root`** (the boot `/` scan is also a volume
  root). Tier (a) boot-disk absolute prefixes apply ONLY under `BootDisk`; applying them to a mount-rooted scan
  false-completes it (every `/Volumes/X/...` child excluded → zero rows → falsely Fresh).
- **The pseudo-fs trio (`proc`, `sys`, `dev`) is skipped only at a corroborated volume root**: root POSITION AND all
  three present as siblings. A name-only rule would drop a user's `.../Dropbox/dev`. The File Provider domain-root probe
  is an OPTIMIZATION, never the cost backstop (that's `reconcile/`).

Architecture, the two progress-timeout rules, the give-up budget, exclusion tiers, and the domain-root detection:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
