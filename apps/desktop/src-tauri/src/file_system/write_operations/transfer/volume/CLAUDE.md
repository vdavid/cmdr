# Cross-volume transfer (copy + move)

Copy and move across backends (Local ↔ MTP ↔ SMB ↔ archive): the phase runner (`copy.rs`), the moves, and the
merge/staging engine (`strategy.rs`, `merge.rs`). File map: `DETAILS.md` § Files. Shared scaffolding: `../CLAUDE.md`.

- **A facade: outside code reaches it only as `transfer::volume::<item>`**; a new caller re-exports from `mod.rs`,
  ❌ never widens a submodule.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow, under every
  policy, backend, and mid-merge cancel/rollback/retry. Assert it through `safety_oracle.rs`, ❌ never inline; new cells:
  `safety_grid_tests.rs`.
- **Dir-vs-dir is NEVER a conflict**, and only for REAL dirs: ask `rename_merge::merges_as_a_directory` (an entry)
  or `Volume::entry_kind` (a path), ❌ never `is_directory` (it follows links). `transfer/DETAILS.md` § "Symlinks are
  opaque to a move".
- **Overwrite merges dirs and replaces files**, enforced at `apply_volume_conflict_resolution`, ❌ not `Volume::delete`;
  NOT reversible. A BLANKET one ❌ never crosses types (`../../CLAUDE.md`); an answered one sets the dest ASIDE.
- **A MOVE's source sweep deletes the walk's LEDGER, ❌ never the tree** (`source_sweep.rs`): Skips, newcomers, and
  changed files stay.
- **❌ Never fabricate a destination size for the conflict dialog**: `None` (a fake `0` makes "Overwrite all smaller"
  unconditional).
- **Skip the dest pre-check ONLY for a dir THIS op created** (`DirectoryCreation::Created`), ❌ never one that looks
  empty. Every name asks `landing.rs`, and an unanswerable probe fails the item. § "Look-alike names and new-name
  spelling".
- **A listed name joins a destination ONLY as a `ChildName`**, ❌ never a raw `join` (`../x` escapes). § "Listed
  names are untrusted".

## Staging and cleanup

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, renamed in after its last byte. Ask
  `../staged_write.rs::resolve_staging`; ❌ only single-shot or whole-publish earns an exemption, NEVER smallness.
- **The SOURCE's mode goes on the temp BEFORE that rename** (`landed_mode.rs`), local destinations only, never wider
  than created; `0` means none, ❌ never guessed. A new write path owes the call.
- **Every file copy asks `Volume::copy_on_server` first**; the BACKEND picks its sources, ❌ never a path. Staged unless
  whole-publish; anything but a cancel or a taken name falls back to streaming.
- **A same-volume move whose `rename_work` says copy takes the copy-then-delete engine**, ❌ never `rename`.
- **A staged temp the destination won't release is REPORTED** (`CancelRollback::staged_leftovers`), ❌ never only
  logged.
- **Only `cleanup.rs::remove_tree` recurses** (its `TreeRemoval` names who authorized it); cleanup and rollback list
  before deleting.
- **An unknown "is this a directory?" is ❌ never guessed**: a missing `source_hints` entry is UNKNOWN;
  `strategy.rs::resolve_source_is_directory` answers. ❌ No `.unwrap_or(false)`, no probing where a hint EXISTS.
- **A same-volume move that renames is a rename-merge with top-level hints only**, ❌ never a subtree walk.

## Concurrency and failures

- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`copy.rs::transfer_concurrency`, ❌ never a
  `min()`). The concurrent driver watches cancel/rollback ON ITS AWAIT; EVERY driver sets its `DriverPhase`.
- **ONE `FileWindow` per operation** (`merge_ctx.rs`, on `MergeCtx`), taken by every merge leaf and top-level FILE task
  (width 1 keeps MTP serial). ❌ Never per level or per source. A walker ❌ never holds a permit while it recurses
  (deadlock at width 1) and ❌ never returns before draining its leaves.
- **A failure carries the path it happened ON** (`transfer_error.rs::PathedVolumeError`): ❌ never re-labelled with the
  top-level source or `.at()` above the frame that knows the item. § "Naming the item that failed".
- **Two test traps**: a `*_tests.rs` here is a `#[path]` CHILD (`super::` one level shallower), and a
  `FaultyVolume` cell must **assert `fault_fired(op)`**.

Flows and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing,
or advising.
