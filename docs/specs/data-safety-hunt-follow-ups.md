# Data-safety hunt follow-ups

What the 2026-09-01 adversarial hunt over the transfer engines left open after its 15 findings were fixed on 2026-09-06.
Each fix surfaced a narrower gap next to it, or scoped one out deliberately; this is the list of those, ranked, in
problem / impact / solution / size form. The severity scale is the hunt's: **critical** = silent permanent loss of user
data in a realistic flow; **high** = loss or corruption needing an unusual but plausible condition, or loss the user is
told about but cannot recover; **medium** = wrong result, wedged state, or recoverable loss; **low** = hygiene.

Nothing here is fixed yet. When one lands, its durable intent goes beside the code (the nearest `DETAILS.md`) and the
entry comes off this list.

The hunt's method (one reading agent per subsystem, a fixed finding schema with `file:line` and a verbatim excerpt,
verification by one reader following the cited lines) covered only the two transfer engines and the write-ops umbrella.
The nine subsystems it never reached (archive edits; delete, trash, and clipboard; the `cmdr-fs` `Volume` trait and
`LocalPosixVolume`; SMB, SFTP, and MTP; the operation log; secrets and settings persistence; the file viewer; the four
slices of `cmdr-index`; git, downloads, listing, cloud actions, tags, and `open_with`) are a second hunt, not an entry
below.

## High

### 1. A cross-FS move loses the bytes written to a file after its copy finished

- **Problem:** `move_with_staging` copies each file in Phase 2 and deletes it from the ledger in Phase 4
  (`transfer/move_op/source_sweep.rs`). A file rewritten in between (a log, a database, an app saving) is in the ledger
  with its old bytes at the destination, so the sweep removes the source and the newer bytes are gone. Nothing compares
  the source at delete time against what was copied. Scoped out of the #1 fix on purpose.
- **Impact:** high. Silent loss of the newest writes to a file that lives in a folder being moved to another disk, share,
  or device, under the same condition as the fixed #1 (something writing into the folder mid-move), which the hunt rated
  high rather than critical because the move has to overlap the write.
- **Solution:** snapshot size and mtime per file at copy time (the copy already stats the source; keep the pair beside
  the `landed_files` entry, not on the reversal ledger, whose identity rule is deliberately mtime-free). In Phase 4 a
  file whose size or mtime no longer matches is kept, its source directory survives, and it rides out on the same
  `AppearedDuringMove` channel the leftover count uses, so the toast can say "2 items changed during the move and stay
  in Work". Red test: seed the cached scan, copy, rewrite one file, sweep; assert the source file survives with its new
  bytes and the completion event counts it.
- **Size:** half a day. One struct field, one comparison in the sweep, one more branch in the existing toast copy, the
  test.

### 2. A top-level folder symlink on a volume still merges through the link

- **Problem:** the #3 and #4 fixes made every recursive merge level treat a symlink as an opaque leaf, but the volume
  engine's top-level source-vs-destination type decision (`transfer/volume/move_same.rs`, via `Volume::is_directory` and
  the preflight `source_hints`) follows links: a symlink-to-dir the user selected, meeting a real directory of that name
  at the destination, is read as dir-vs-dir and merged, which lists the link's TARGET and renames its entries out.
- **Impact:** high. A folder outside the selection is emptied into the destination, on an external volume (its own
  volume id routes it here) or a share exposing directory symlinks. A top-level selection is the more common shape of
  the fixed #3, since users select what they see.
- **Solution:** give the `Volume` trait a symlink-aware type answer (an `entry_kind` or an `is_symlink` beside
  `is_directory`, defaulting to "not a link" for backends with no link concept), populate it in `LocalPosixVolume`, SMB,
  and SFTP from the metadata they already fetch, thread it through `resolve_source_is_directory` and the preflight hint,
  and route a link-vs-dir top-level clash through `resolve_volume_conflict` as the type mismatch it is. Red test: the
  `rename_merge_symlink_tests.rs` rig (a `LocalPosixVolume` over a `TempDir`) with the link as the top-level source.
- **Size:** one to two days. The trait change touches every backend and every test double; the engine change is small.

## Medium

### 3. The SMB single-shot small-file write has no "I expected this name free" guard

- **Problem:** a staged write refuses to clear a destination name nobody resolved a conflict for
  (`staged_write::LandingName::ExpectedFree`, from the #2 fix), but a file under the single-shot threshold on SMB writes
  at its final name with `FileOverwriteIf`, which truncates whatever is there. The fold-only collision that motivated the
  guard is now settled before any write (`merge_level` shares `DestNameIndex`), and a failed destination probe no longer
  reaches a write (#9), so what remains is a file arriving at that name between the level listing and the write.
- **Impact:** medium. A silent replacement, but only inside a listing-to-write race window (seconds on a large merge),
  and only on the single-shot path.
- **Solution:** an exclusive-create disposition through `Volume::write_from_stream` (a `WriteDisposition` beside the
  path, `CreateNew` vs `Overwrite`), so a single-shot write under `ExpectedFree` asks SMB for `FILE_CREATE` and gets
  `STATUS_OBJECT_NAME_COLLISION` back as `AlreadyExists`; a backend that can't express exclusivity falls back to staging
  for that item (the round trip the single-shot exemption exists to avoid, paid only there). Red test: the SMB fixture
  stack with a file created between the listing and the write.
- **Size:** one day. Roughly eight production backends plus every test double take the new parameter; the SMB
  disposition itself is a few lines.

### 4. The volume engine's folder-over-file Overwrite deletes the file before the folder lands

- **Problem:** `apply_volume_conflict_resolution`'s Overwrite arm (`transfer/volume/conflict.rs`) deletes the
  destination file, then the recursive copy creates the directory and fills it. The local engine got an aside for this
  (#13, `DisplacedEntry`, kept until commit and restored on rollback); the volume side didn't. Since the #5 fix only an
  explicitly answered Stop prompt reaches this arm.
- **Impact:** medium. A failure or cancel mid-subtree leaves neither the file nor a complete folder. The user chose
  Overwrite for that pair, and the source still exists, so the loss is the file they agreed to replace plus the wasted
  work.
- **Solution:** rename the file aside through `StagingTemp::mint_aside` (the `.cmdr-temp-<uuid>` sibling every other
  aside uses), record it in `CreatedPaths` as a displaced entry, discard it when the operation commits, and restore it
  on rollback or failure the way the local `commit_keeping_displaced_aside` does (a ` (recovered)` sibling when the
  directory has already taken the name). The `ResolvedConflict` type grows an aside field that `merge.rs`, `strategy.rs`,
  and the three write sites thread through.
- **Size:** one day. Mostly plumbing through the volume-side ledger; the mechanism exists locally.

### 5. The local folder-over-file Stop prompt describes the clash as file-vs-file

- **Problem:** `transfer/copy/single_item.rs` hands the blocking file to `resolve_conflict` as both source and
  destination, so the dialog renders `source_is_directory: false, destination_is_directory: false`: a file-vs-file prompt
  for a folder-replacing-a-file decision. Verified live during the #5 work.
- **Impact:** medium. The blanket-Overwrite refusal (#5) rests on "an explicit Stop answer is informed because the prompt
  shows both types"; this is the one prompt where it doesn't. Safe on failure now (the aside is kept), but the consent
  it collects is uninformed.
- **Solution:** pass the real source directory as the prompt's source (its size from the drive index, as the volume
  prompt already does), so the dialog says folder-over-file and shows the folder's size; keep the destination as the
  blocking file. The `IncomingItem` the #5 fix added is the carrier.
- **Size:** two hours, plus a gallery fixture so the prompt can be reviewed.

## Low

### 6. A merge leaf whose future is dropped keeps its 0-byte placeholder

- **Problem:** the #14 fix takes back a deep-merge Rename placeholder (`name (1).ext`, reserved with `O_EXCL`) when the
  leaf abandons. A leaf whose FUTURE is dropped by the concurrent driver's cancel-drain deadline runs no abandon path, so
  its placeholder stays.
- **Impact:** low. One 0-byte `file (1).ext` per unresolved clash in that window, indistinguishable from a real file.
- **Solution:** either a `Drop` guard on the reservation (take it back on drop unless committed, the same shape
  `StagedWrite` uses for its temp) or have the driver's drain sweep placeholders the way it sweeps partials
  (`in_flight_partials`).
- **Size:** half a day, with a test that forces the drain deadline.

### 7. `sequential_extract`'s plan mode reserves placeholders before it streams

- **Problem:** the sequential extractor reserves every clashing member's `name (1).ext` at plan time and streams them in
  a later pass, so a cancel or a read error between the two leaves every not-yet-streamed placeholder behind. The #14
  fix covers the streaming pass only.
- **Impact:** low. Same 0-byte leftovers as above, on the solid-archive extract path.
- **Solution:** reserve at stream time instead (the plan only needs to know the name is free, which `ClaimedNames`
  answers in memory), or take back every unstreamed reservation on the abandon path the plan runner already has.
- **Size:** half a day.

### 8. A late-detected collision in the streaming deep merge fails the item instead of prompting

- **Problem:** `rename_merge.rs::late_detected_collision` re-lists and runs the resolver (prompting under Stop) when a
  child's name turns out taken after all; the streaming `merge_level` reports the same case as that item's failure
  (`DestinationExists`) because a merge leaf runs inside a `FuturesUnordered` with no `MergeCtx` to prompt from. Since
  the #2 fix the case is reachable only by a file arriving between the listing and the landing.
- **Impact:** low. The safe side (nothing is replaced), but the two engines answer the same event differently, and the
  user gets a failed item where the same-volume path would have asked.
- **Solution:** hand the leaf a prompting seam (the `MergeCtx` already carries the `FileWindow`; add the resolver) so a
  late collision goes through `resolve_merge_child` like a listed one.
- **Size:** half a day; a modest refactor of the leaf's context.

### 9. The concurrent driver's post-loop is pinned at the driver seam, not end to end

- **Problem:** the #12 fix made `drive_transfer_concurrent` return a `ConcurrentOutcome` rather than a `Result`, so the
  post-loop (partial sweep, rollback, cancelled event) runs by construction; the test asserts the outcome at the driver
  seam. No cell observes the post-loop's effects after a resolver refusal deterministically.
- **Impact:** low. A coverage gap, not a defect; the type makes the skip unrepresentable.
- **Solution:** one cell in `copy_cancel_tests.rs` using `copy_wedge_test_support.rs` (a gated task parked mid-write, a
  `ConflictResponderSink` refusing the prompt) asserting the partial is swept and the cancelled event fires.
- **Size:** two hours.
