# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), through `transfer_driver/`, emitting via
`OperationEventSink`. State, intent, cancel/rollback, ETA, the conflict mutex, settle: `../CLAUDE.md`. Frontend:
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

## Module map

- Local-FS: `copy/` (+ `CopyTransaction` rollback), `move_op.rs`, `copy_strategy.rs` + `{macos,linux,chunked}_copy.rs`.
- Volume: `volume_{copy,move,preflight,rename_merge,conflict,strategy}.rs`; `volume_copy.rs` runs the phases + post-loop
  and drives ONE of `volume_copy_{concurrent,serial}.rs`. Plus `checkpoint_stream.rs`, `staged_write.rs`,
  `retry.rs` (per-file retry policy), `transfer_probe.rs` (in-flight table + stall watchdog + the abort it trips).

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy,
  every backend, cancel/rollback/retry mid-merge (`volume_merge_tests.rs`).
- **Top-level dest pre-check: skip it outright ONLY for a dest dir THIS op created** (`DirectoryCreation::Created`), ❌
  never one that merely looks empty. A MERGE answers it from the ONE listing Phase 0.6's temp-reap already pays for
  (`dest_name_index.rs`), ❌ not a probe per file. That listing is a SNAPSHOT: a file arriving mid-batch is overwritten
  unprompted — David's call, ❌ no re-listing or freshness window. `DestNameIndex` says `Absent` only when NO backend
  could route the name onto an entry it holds (case, NFC/NFD, trailing dot, `~` ⇒ `Unknown` ⇒ the real probe); ❌ never
  a byte-exact map, and a FAILED listing ⇒ probe all. Local dests keep their `stat`s; MTP never reaches this.
- **Dir-vs-dir is NEVER a conflict**: `resolve_volume_conflict` short-circuits to merge before any policy lookup or
  emit. Stop/Skip/Rename all merge the folder; only files prompt.
- **Overwrite means merge for dirs, replace for files**, enforced at the `apply_volume_conflict_resolution` call site,
  NOT by `Volume::delete`'s contract — else a recursive-delete backend flips merge into replace.
- **Overwrite is NOT reversible**: rollback can't restore a replaced original. ❌ No unbounded backup.
- **Cross-type Rename reserves the name with a 0-byte `O_EXCL` placeholder** (TOCTOU guard), returning
  `needs_safe_overwrite: true`.

## Staging and durability

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>` and takes its final name only after its last byte**: a
  force-quit must never leave a truncated file at a real name. Exempt: `AlreadyStaged` and `SingleShot` — ❌
  single-shot-ness buys that, NEVER smallness; ask `resolve_staging`. A temp is in `in_flight_temps` only while partial,
  carrying `state.liveness_token()` so it hides.
- **Cross-volume file→file Overwrite is a safe-replace** (sibling temp + `finalize_safe_replace`); that temp is
  committed data, ❌ not a partial. Cross-type: delete-first.
- **Cleanup/rollback for a DIRECTORY source is per-FILE, never the dir root** (a merge holds pre-existing dest files,
  so a recursive root delete is silent data loss): `last_dest_path` is cleared for a dir source, and a dir root never
  enters `in_flight_partials`.

- **Cross-FS move source-delete preserves Skipped sources, AFTER `flush_created_destinations`.** Same-volume move is a
  rename-merge with top-level hints only (`bytes_total = 0`), never a subtree walk.

## Streaming, cancel, and diagnosis

- **Cross-volume copy parks/yields between chunks** (`CheckpointStream`): park in place, NO release/reopen; auto-yield
  keeps the op **Running**. TWO opt-ins, ❌ don't merge: SOURCE read-yield (MTP + SMB) is UNBOUNDED; DESTINATION
  write-yield (SMB only) is HARD-CAPPED — it holds a handle the server reaps.
- **A LOCAL `max_concurrent_ops` must NOT bound a REMOTE peer** (`transfer_concurrency`); ❌ don't restore the `min()`.
  A remote cap always binds — that's what keeps MTP serial.
- **The concurrent driver observes cancel/rollback ON ITS AWAIT**, not only in the spawn loop: it races
  `in_flight.next()` against `state.backend_cancel`, drains under `CANCEL_DRAIN_DEADLINE`, then ABANDONS the rest. ❌
  Never unbounded.
- **Every phase must announce itself** (`transfer_probe.rs`): a parked transfer is invisible to a stack sample. ❌ No
  `.await` on a transfer path without a phase. The probe is ALSO the UI's stall signal, so register on BOTH paths; ❌
  never derive a stall from FE timing — a wedge emits NOTHING.
- **A transport blip retries the FILE** (`retry.rs`), ONLY inside `stream_pipe_file`: 3 attempts, bounded cancel-aware
  backoff, restarting at byte zero on a fresh temp. ❌ Never a `Cancelled`; ❌ never higher — conflicts, the ledger, the
  journal, and the milestone all sit above it and must happen once. Retryability: an exhaustive typed match.
- **The stall watchdog's teeth are GATED and inert**: it ends a task's wait only on
  `Volume::connection_liveness() == Dead` **AND** `STALL_ABORT_AFTER`. Nothing answers that, 0.16.0's keepalive
  included: a missed ECHO isn't death. ❌ Never collapse the AND — a keepalive false-`Dead`s under write load. Why, and
  what `smb2` must expose: `DETAILS.md`.
- **Both progress paths report a file's HIGH-WATER bytes** (concurrent `fetch_max`, ❌ not `swap`; serial
  `leaf_high_water`), so a retry's restart neither double-counts nor reverses the bar.

Semantics, flows, decisions, the retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any non-trivial work
here.
