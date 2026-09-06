# Cross-volume transfer (copy + move)

Copy and move across backends (Local ↔ MTP ↔ SMB ↔ archive): the phase runner (`copy.rs`), the cross-, same-volume, and single-file moves, and the merge/staging engine (`strategy.rs`,
`merge.rs`). File map: `DETAILS.md` § Files. Shared scaffolding: `../CLAUDE.md`.

- **A facade: outside code reaches it only as `transfer::volume::<item>`**; a new caller re-exports from `mod.rs`,
  ❌ never widens a submodule.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow, under every
  policy, backend, and mid-merge cancel/rollback/retry. Assert it through `safety_oracle.rs`, ❌ never inline; new cells:
  `safety_grid_tests.rs`.
- **Dir-vs-dir is NEVER a conflict**, and only for REAL dirs: a link-to-dir lists as `is_directory`,
  so ask `rename_merge::merges_as_a_directory`, ❌ never `Volume::is_directory` (follows links). `transfer/DETAILS.md`
  § "Symlinks are opaque to a move".
- **Overwrite means merge for dirs, replace for files**, enforced at the `apply_volume_conflict_resolution` call site,
  ❌ not by `Volume::delete`; NOT reversible. A BLANKET Overwrite ❌ never crosses types (`../../CLAUDE.md`).
- **A MOVE's source sweep spares every child the merge skipped** (`remove_tree`'s `preserve` set): that source is the
  ONLY copy.
- **❌ Never fabricate a destination size for the conflict dialog**; report `None` (a fabricated `0` makes "Overwrite all
  smaller" unconditional).
- **Skip the dest pre-check ONLY for a dir THIS op created** (`DirectoryCreation::Created`), ❌ never one that looks
  empty. Top level and deep merge share `DestNameIndex`: ❌ a fold-only name is never free, and an unanswerable
  probe fails the item.

## Staging and cleanup

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, taking its final name after its last byte. Ask
  `strategy.rs::resolve_staging`; ❌ single-shot-ness earns an exemption, NEVER smallness.
- **The SOURCE's mode goes on the temp BEFORE that rename** (`landed_mode.rs`), local destinations only, never wider
  than what the destination created. `0` means no mode: ❌ never guess or fail over one. A new write path owes the call.
- **A same-`Arc` copy tries `strategy.rs::try_server_side_copy` (`Volume::copy_within`) first**, staged, ❌ never
  single-shot. Anything short of success streams, ❌ except a cancel.
- **A staged temp the destination won't release is REPORTED**: the sweep RETURNS it on
  `CancelRollback::staged_leftovers`, ❌ never `skips`, ❌ never only a log.
- **Only `cleanup.rs::remove_tree` recurses, and its `TreeRemoval` argument names who authorized it.** Cleanup and
  rollback go through `delete_written_file` / `prune_created_dir_if_empty`, listing before deleting.
- **An unknown "is this a directory?" is ❌ never guessed**: a missing `source_hints` entry means UNKNOWN, ❌ never
  "file"; `strategy.rs::resolve_source_is_directory`'s answer drives the cleanup/ledger branch.
  ❌ No `.unwrap_or(false)`, ❌ no `Default` on `SourceHint`, ❌ no probing where a hint EXISTS.
- **Cross-FS move deletes sources AFTER `flush_created_destinations`, preserving Skipped ones.** Same-volume move is a
  rename-merge with top-level hints only, ❌ never a subtree walk.

## Concurrency and failures

- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`copy.rs::transfer_concurrency`, ❌ never a
  `min()`). The concurrent driver watches cancel/rollback ON ITS AWAIT; EVERY driver sets its `DriverPhase`.
- **ONE `FileWindow` per operation** (`strategy.rs`, on `MergeCtx`), taken by every merge leaf and top-level FILE task
  (width 1 keeps MTP serial). ❌ Never per level or per source. A walker ❌ never holds a permit while it recurses
  (deadlock at width 1) and ❌ never returns before draining its leaves.
- **A failure carries the path it happened ON** (`transfer_error.rs::PathedVolumeError`): ❌ never re-label with the
  top-level source, ❌ never `.at()` above the frame that knows the item. § "Naming the item that failed".
- **Two test traps**: a `*_tests.rs` here is a `#[path]` CHILD (`super::` is one level shallower), and a
  `FaultyVolume` cell must **assert `fault_fired(op)`**.

Semantics, flows, decisions, and the rollback ledger: `DETAILS.md`; read it before non-trivial work.
