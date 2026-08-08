# Cross-volume transfer (copy + move)

The engine that copies and moves across backends (Local ↔ MTP ↔ SMB ↔ archive): the phase runner (`copy.rs` driving ONE
of `copy_concurrent.rs` / `copy_serial.rs`), the move paths (`move.rs` cross-volume, `move_same.rs` + `rename_merge.rs`
same-volume), the merge/staging engine (`strategy.rs`, `sequential_extract.rs`), and `conflict.rs`, `cleanup.rs`,
`preflight.rs`, `transfer_error.rs`. Shared scaffolding, local-FS copy, and the drivers: `../CLAUDE.md`.

- **This directory is a facade: outside code reaches it only as `transfer::volume::<item>`.** Every module here is
  private to `volume`; a new outside caller adds a re-export to `mod.rs`, ❌ never widens a submodule's visibility. That
  boundary is what lets these internals move without a repo-wide rename.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow, under every
  policy, backend, and cancel/rollback/retry mid-merge (`merge_tests.rs`). Assert it through `safety_oracle.rs`'s three
  clauses, ❌ never fresh inline asserts. A new coverage cell goes in `safety_grid_tests.rs`.
- **Dir-vs-dir is NEVER a conflict**; only files prompt. **Overwrite means merge for dirs, replace for files**, enforced
  at the `apply_volume_conflict_resolution` call site, ❌ not by `Volume::delete`, and NOT reversible.
- **A MOVE's source sweep spares every child the merge skipped** (`remove_tree`'s `preserve` set): a skipped child's
  source is the ONLY copy.
- **❌ Never fabricate a destination size for the conflict dialog**; report `None`. A fabricated `0` makes every dest
  look smaller, silently turning "Overwrite all smaller" into an unconditional overwrite.
- **Skip the top-level dest pre-check ONLY for a dest dir THIS op created** (`DirectoryCreation::Created`), ❌ never
  one that merely looks empty.

## Staging and cleanup

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, taking its final name only after its last byte. Ask
  `strategy.rs::resolve_staging`; ❌ single-shot-ness earns an exemption, NEVER smallness.
- **Only `cleanup.rs::remove_tree` recurses, and its `TreeRemoval` argument names who authorized it.** Cleanup and
  rollback call `delete_written_file` / `prune_created_dir_if_empty` (which lists before it deletes), so a wrong belief
  can't reach a recursive delete: there isn't one in scope. A fourth sweep adds a variant, or it doesn't happen.
- **An unknown "is this a directory?" is ❌ never guessed.** A missing `source_hints` entry means UNKNOWN, ❌ never
  "file": ask `strategy.rs::resolve_source_is_directory`, and the RESOLVED answer drives the cleanup/ledger branch.
  `NotFound` is an ANSWER; anything else fails the item. ❌ No `.is_directory(…).await.unwrap_or(false)`, a guessed
  `false` picks the destructive branch (`desktop-rust-probe-unwrap-justified` enforces it). **`SourceHint` has no
  `Default`; ❌ never add one.** ❌ Don't probe where a hint EXISTS (15k MTP sources = 15k listings). `DETAILS.md`
  § "A missing source hint means unknown" and § "Every belief-default in this directory, decided".
- **Cross-FS move deletes sources AFTER `flush_created_destinations`, preserving Skipped ones.** Same-volume move is a
  rename-merge with top-level hints only, never a subtree walk.

## Concurrency and failures

- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`copy.rs::transfer_concurrency`, ❌ never a
  `min()`; a remote cap always binds, which keeps MTP serial). The concurrent driver watches cancel/rollback ON ITS
  AWAIT.
- **A failure carries the path it happened ON** (`transfer_error.rs::PathedVolumeError`): ❌ never re-label with the
  top-level source, ❌ never `.at()` above the frame that knows the item; a directory sweep names the first child that
  refused. `DETAILS.md` § "Naming the item that failed".
- **A `*_tests.rs` here is a `#[path]` CHILD of the module it pins**, so `super::` is one level shallower than at file
  scope; a wrongly-deepened chain still compiles. `DETAILS.md` § Files.
- **Inject a wrong or missing answer with `FaultyVolume`** (`faulty_volume_test_support.rs`), ❌ never a fresh
  forwarder; `forward_volume_methods!` writes the boilerplate, so a double's diff is only its lie. **Assert
  `fault_fired(op)`**: a preflight hint can route the code past your fault, and the cell then asserts the UNFAULTED
  behavior while reading as though it covered the faulted one. `DETAILS.md` § test-support files.

Semantics, flows, decisions, and the rollback ledger: `DETAILS.md`. Read it before any non-trivial work here.
