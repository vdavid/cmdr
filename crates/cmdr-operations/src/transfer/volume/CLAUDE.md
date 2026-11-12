# Cross-volume transfer (copy + move)

Copy and move across every volume backend. File map: `DETAILS.md` § Files. Shared: `../CLAUDE.md`.

- **A facade: outside code reaches it only as `transfer::volume::<item>`**; a new caller re-exports from `mod.rs`, ❌
  never widens a submodule.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow, under every
  policy, backend, and mid-merge cancel/rollback/retry. Assert it through `safety_oracle.rs`, ❌ never inline; new
  cells: `safety_grid_tests.rs`.
- **Dir-vs-dir is NEVER a conflict**, and only for REAL dirs: ask `rename_merge::merges_as_a_directory` (an entry) or
  `Volume::entry_kind` (a path), ❌ never `is_directory` (it follows links). `transfer/DETAILS.md` § "Symlinks are
  opaque to a move".
- **Overwrite merges dirs and replaces files**, enforced at `apply_volume_conflict_resolution`, ❌ not `Volume::delete`;
  NOT reversible. A BLANKET one ❌ never crosses types (`../../../CLAUDE.md`); an answered one sets the dest ASIDE.
- **A MOVE's source sweep deletes the walk's LEDGER, ❌ never the tree** (`source_sweep.rs`): Skips, newcomers, and
  changes stay. A drop re-runs it WHOLE, ❌ no blind retry. Barrier first: `held_sweeps.rs`.
- **❌ Never fabricate a destination size for the conflict dialog**: `None` (a fake `0` makes "Overwrite all smaller"
  unconditional).
- **Skip the dest pre-check ONLY for a dir THIS op created** (`DirectoryCreation::Created`), ❌ never one that looks
  empty. Every name asks `landing.rs`, and an unanswerable probe fails the item. § "Look-alike names and new-name
  spelling".
- **A listed name joins a destination ONLY as a `ChildName`**, ❌ never a raw `join` (`../x` escapes). § "Listed names
  are untrusted".

## Staging and cleanup

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, renamed in after its last byte. Ask
  `../staged_write.rs::resolve_staging`; ❌ only single-shot or whole-publish earns an exemption, NEVER smallness.
- **What a file carries rides its stream** (`VolumeReadStream::metadata`); the DESTINATION puts it on the temp. ❌ No
  engine-side `chmod` or xattr write; wrappers forward `metadata`. Extract quarantine: a stream decorator, and folders
  at creation, ❌ never in `folder_metadata.rs`. § "Copies keep the source's metadata".
- **Every file copy asks `may_copy_on_server_from`, then `copy_on_server`**: the BACKEND picks, ❌ never by path, ❌
  nothing stages before a yes. All but a cancel or a taken name falls back to streaming.
- **A same-volume move whose `rename_work` says copy takes the copy-then-delete engine**, ❌ never `rename`.
- **A stop's cleanup ❌ never waits on the share** (`../leftover_cleanup.rs`); leftovers are REPORTED by fate.
- **Only `cleanup.rs::remove_tree` recurses** (its `TreeRemoval` names who authorized it); cleanup and rollback list
  before deleting.
- **An unknown "is this a directory?" is ❌ never guessed**: a missing `source_hints` entry is UNKNOWN;
  `strategy.rs::resolve_source_is_directory` answers. ❌ No `.unwrap_or(false)`, no probing where a hint EXISTS.
- **A same-volume move that renames is a rename-merge with top-level hints only**, ❌ never a subtree walk.

## Concurrency and failures

- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`copy.rs::transfer_concurrency`, ❌ never a
  `min()`). The concurrent driver SPAWNS its tasks (ERR-8MEX6), watches `stop_work`, and DRAINS on a failure, ❌ never
  aborts first. EVERY driver sets its `DriverPhase`.
- **ONE `FileWindow` per operation** (`merge_ctx.rs::MergeCtx`), taken by every merge leaf and top-level FILE task
  (width 1 keeps MTP serial), ❌ never per level or source. A walker ❌ never recurses holding a permit, awaits without
  polling its leaves (`LeafPool::drive`: one may hold the share's reconnect), or returns undrained.
- **A failure carries its path AND side** (`transfer_error.rs::PathedVolumeError`), set where the item is known. Source
  streams go through `source_reads::watch`, ❌ never bare. § "Naming the item that failed".
- **Two test traps**: a `*_tests.rs` here is a `#[path]` CHILD (`super::` one level shallower), and a `FaultyVolume`
  cell must **assert `fault_fired(op)`**.

Flows and decisions: `DETAILS.md`.
