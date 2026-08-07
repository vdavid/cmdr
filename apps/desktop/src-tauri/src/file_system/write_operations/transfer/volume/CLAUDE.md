# Cross-volume transfer (copy + move)

The engine that copies and moves across backends (Local ↔ MTP ↔ SMB ↔ archive): the phase runner (`copy.rs` driving ONE
of `copy_concurrent.rs` / `copy_serial.rs`), the move paths (`move.rs` cross-volume, `move_same.rs` + `rename_merge.rs`
same-volume), the merge/staging engine (`strategy.rs`, `sequential_extract.rs`), and `conflict.rs`, `cleanup.rs`,
`preflight.rs`, `transfer_error.rs`. Shared scaffolding, local-FS copy, and the drivers: `../CLAUDE.md`.

- **This directory is a facade: outside code reaches it only as `transfer::volume::<item>`.** Every module here is
  private to `volume`; a new outside caller adds a re-export to `mod.rs`, ❌ never widens a submodule's visibility. That
  boundary is what lets these internals move without a repo-wide rename, and it is why `move.rs`'s `r#move` keyword
  escape never appears outside this directory.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy,
  backend, and cancel/rollback/retry mid-merge (`merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**; only files prompt. **Overwrite means merge for dirs, replace for files**, enforced
  at the `apply_volume_conflict_resolution` call site, ❌ not by `Volume::delete`. ❌ It is NOT reversible.
- **A MOVE's source sweep spares every child the merge skipped** (`delete_volume_path_recursive_preserving`): a skipped
  child never reached the dest, so its source is the ONLY copy.
- **❌ Never fabricate a destination size for the conflict dialog**; report `None`. A fabricated `0` makes every dest
  look smaller, silently turning "Overwrite all smaller" into an unconditional overwrite.
- **Skip the top-level dest pre-check ONLY for a dest dir THIS op created** (`DirectoryCreation::Created`), ❌ never one
  that merely looks empty.

## Staging and cleanup

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, taking its final name only after its last byte. Ask
  `strategy.rs::resolve_staging`; ❌ single-shot-ness earns an exemption, NEVER smallness.
- **Cleanup and rollback for a DIRECTORY source are per-FILE, never the dir root**: a merge holds pre-existing dest
  files, so a recursive root delete is silent data loss.
- **A missing `source_hints` entry means UNKNOWN, ❌ never "file"**, and the RESOLVED answer (not the raw hint) drives
  the cleanup/ledger branch. Ask `strategy.rs::resolve_source_is_directory`. A defaulted `false` streams a directory as
  a file AND lets a failed copy recursively delete the merged dest ROOT. ❌ Don't probe where a hint EXISTS (15k MTP
  sources = 15k listings). `DETAILS.md` § "A missing source hint means unknown".
- **Cross-FS move deletes sources AFTER `flush_created_destinations`, preserving Skipped ones.** Same-volume move is a
  rename-merge with top-level hints only, never a subtree walk.

## Concurrency and failures

- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`copy.rs::transfer_concurrency`, ❌ never a `min()`;
  a remote cap always binds, which keeps MTP serial). The concurrent driver watches cancel/rollback ON ITS AWAIT,
  draining under `CANCEL_DRAIN_DEADLINE`.
- **A failure carries the path it happened ON** (`transfer_error.rs::PathedVolumeError`): ❌ never re-label with the
  top-level source, ❌ never `.at()` above the frame that knows the item, and a directory sweep names the first child
  that refused, ❌ never the parent's own `ENOTEMPTY`. `DETAILS.md` § "Naming the item that failed".
- **A `*_tests.rs` file here is a `#[path]` CHILD of the module it pins**, so inside one `super::` is that parent and
  `super::super::` is `volume` — one level shallower than the same text at file scope. `conflict.rs` carries an INLINE
  `mod tests { … }`, holding both scopes in one file. Check which scope you're in before touching a `super::` chain; a
  wrongly-deepened one can still compile against a same-named module at the other level.

Semantics, flows, decisions, the rollback ledger, and the destination pre-check: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
