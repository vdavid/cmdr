# Cross-volume transfer details

Pull-tier docs for `transfer/volume/`: the copy and move engine that spans Local ↔ MTP ↔ SMB ↔ archive backends, its
merge and conflict semantics, the rollback ledger, the destination pre-check, the single-shot staging exemption, and how
a failure names the item it happened on. Must-know invariants live in `CLAUDE.md`.

The shared scaffolding this engine runs on is documented one level up, next to the code that owns it: staging
(`../DETAILS.md` § "File writes are staged"), the parked-chunk pause (§ "Pause reaches between chunks"), the stall
signal (§ "The stall signal"), per-file retry and its watchdog (§ "Retrying one FILE, and the watchdog that ends a wait
nothing else will"), and cancel/rollback against a parked driver (§ "Cancel and rollback reach a parked driver").

## Files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape and its
invariants: `CLAUDE.md`. Only the layout facts neither of those carries live here:

- **The concurrent copy driver is three files, one per stage of a source's life.** `copy_concurrent.rs` turns the
  window (`ConcurrentDriver`: fill, wait, record, plus the drain-deadline tiers); `copy_concurrent_source.rs` decides
  what a source needs before it can join it (`ConcurrentCopy::prepare_source`: the directory question, the destination
  pre-check, the SYNCHRONOUS conflict resolve, skip accounting); `copy_concurrent_task.rs` streams one source end to
  end (`run_copy_task` over an owned `CopyTask`, plus the two result payloads). The task's inputs are OWNED, which is
  what keeps its future `'static` and the window free of a lifetime — ❌ don't reintroduce a borrow there, it welds the
  window to the borrowed context and blocks every further split. `copy_concurrent_driver_tests.rs` drives
  `drive_transfer_concurrent` and `prepare_source` directly, because the post-loop's delete-capability split (below)
  means an end-to-end assertion can't tell a right rollback ledger from a wrong one.
- **The engine is two files, and they call each other.** `strategy.rs` moves ONE file's bytes (staging,
  `stream_pipe_file`, both cancel tiers) and owns the vocabulary both halves speak (`MergeCtx`, `CreatedPaths`,
  `copy_single_path`). `merge.rs` walks a tree (`copy_directory_streaming`, `resolve_merge_child`): a directory child
  recurses there, a file child goes to `strategy.rs`. `sequential_extract.rs` reuses the same walk in plan mode, which
  is why the merge/conflict/rollback code is not reimplemented for one-pass archives.
- **`move_file.rs` is the per-FILE cross-volume move**, under the operation-level driver in `move.rs`: one
  `stream_pipe_file` plus the source-side removal, no scan, no conflict resolution, no journaling. It exists because
  the operation-log rollback restores one already-decided leaf at a time and needs the staging, retry, stall detection,
  and mid-file cancel that only `stream_pipe_file` has — `move_volumes_with_progress` is the wrong granularity, and
  it's `#[cfg(test)]`-only outside anyway.
- **`strategy_*_tests.rs` are shallow engine tests**; the full merge + policy pipeline is pinned by the copy-side
  merge suite. That suite is three files split by contract, all declared from `copy.rs`: `merge_tests.rs` (per-file
  conflict resolution INSIDE a directory merge, and the fixtures `make_rich_merge` / `make_deep_clash_with_bigger_dest`
  it owns), `merge_dir_vs_dir_tests.rs` (the "always merge, never prompt" contract and its boundary, the type
  mismatches and fresh levels that are NOT dir-vs-dir merges), and `merge_dispatch_mutex_tests.rs` (the
  conflict-dispatch mutex across concurrent and nested merges). A new merge test adds itself to the matching contract
  rather than growing one file. `rename_merge_tests.rs` drives `LocalPosixVolume` over a tempdir because
  `InMemoryVolume` models neither real subtree-rename nor empty-only-delete semantics, plus a `CaseInsensitiveVolume`
  double for the case-fold cases.
- **Naming a destination is `naming.rs`, not `conflict.rs`.** `find_unique_volume_name` (plus `resolve_local_path`, the
  root-anchoring its `O_EXCL` reservation needs) walks the shared `unique_name::NameCandidates` and owns only HOW a pick
  is held on a volume; `conflict.rs` decides POLICY and asks it for a name. Its tests are `naming_tests.rs`.
- **Duplicating in place is pinned once, for both engines**, in `self_collision_tests.rs` (declared from `mod.rs`, so `super::` is `volume`): it drives the real `copy_volumes_with_progress` and `move_within_same_volume_with_progress` over one `InMemoryVolume` used as BOTH sides, and pins the folding cases at the `resolve_volume_conflict` seam, because `InMemoryVolume` is itself case- and normalization-sensitive.
- **The move suite is six files split by subject, all declared from `move.rs`**: `move_tests.rs`
  (cross-volume happy path, conflict matrix, bulk skip, dest auto-create), `move_same_tests.rs` (the
  same-volume rename path), `move_cancel_tests.rs`, `move_failure_tests.rs` (finalize-rename failure and
  error naming), `move_progress_tests.rs` (byte totals, scan tallies, leaf granularity), and
  `move_merge_tests.rs` (the no-byte-lost policy matrix, which owns its own fixture trees). Fixtures every suite
  shares (`make_state`, `make_state_with_interval_ms`, `make_volumes`, `config_default`) plus the
  `CancelAfterFirstSink`, `SampleInFlightTableSink`, and `MoveRenameFailsDestVolume` doubles live in
  `move_test_support.rs`, reached as `super::test_support` — a new move test adds itself to the matching subject rather
  than growing one file. `SampleInFlightTableSink` is the only way to see the probe's in-flight table: it renders the
  table from `emit_progress`, which the destination calls from inside `write_from_stream` and therefore inside the
  `CURRENT_TASK_PROBE` scope. Outside that window the row is already gone.
- **A `*_tests.rs` file is a `#[path]` CHILD of the module it pins**, not a sibling of it: `copy_tests.rs` is
  `volume::copy::tests`, `strategy_pause_tests.rs` is `volume::strategy::pause_tests`. So inside one, `super::` is that
  parent module and `super::super::` is `volume` — one level shallower than the same text at file scope in
  `copy.rs`. Check which scope you are in before touching a `super::` chain here; a wrongly-deepened one can still
  compile against a same-named module at the other level.
- **`copy_bench.rs` is `#[ignore]`d** and needs a QNAP NAS plus `SMB2_TEST_NAS_PASSWORD`, so it never runs in CI.
- **Three test-support files, split by what they fake.** `faulty_volume_test_support.rs` holds `FaultyVolume` (wrap any
  volume, arm the Nth call to a named operation to fail) plus `forward_volume_methods!`, the macro every `Volume` double
  in this directory builds its boilerplate from — a new double lists the methods it forwards and hand-writes only what
  it lies about. `strategy_test_support.rs` holds the doubles whose lie is behavioral (`RecursiveDeleteVolume`,
  `UndeletableSource`, `FlakyDest`, the yield/pause sources); `copy_wedge_test_support.rs` holds the STREAM doubles
  (`GatedChunkStream`, `IncrementalDest`), which control WHEN bytes arrive and are a different axis entirely. ❌
  `FaultyVolume` over an `InMemoryVolume` can't show a half-written destination — the in-memory store buffers a whole
  file and creates it at the end — so partial-destination cells use `LocalPosixVolume`.
- **An armed fault that never fires reads exactly like a passing test.** A cell that arms `FaultyOp::IsDirectory` on the
  source and then lets the operation run a REAL preflight scan gets a confident `is_directory` hint in `source_hints`,
  so `resolve_volume_conflict` takes its `Some(hint)` arm and never probes. The cell then asserts the UNFAULTED
  behavior — for a cross-type Overwrite, the documented recursive clear of the destination — while reading as though it
  covered the unanswerable-probe case. Two halves keep that honest: seed an empty-`per_path` preview
  (`seed_incoherent_scan_result_for_test`) so no hint exists and the probe actually happens, and assert
  `FaultyVolume::fault_fired(op)` so a cell that stops reaching its own fault fails instead of quietly changing subject.

### The safety oracle

`safety_oracle.rs` states ONCE what a finished copy, move, or delete must have left behind, and every suite that asserts
data safety routes through it (`merge_tests.rs`, `move_merge_tests.rs`, `safety_grid_tests.rs`). Three clauses:

1. **No byte the user didn't approve is gone from either side** — every source file's content is readable from the
   source tree or the destination tree. Searched by CONTENT over whole trees, ❌ never by path: Rename relocates a
   clashing item to a `name (1)` sibling, and for a clashing DIRECTORY that shifts every file inside it. Fixture
   contents are unique per file, so presence in the bag is an honest "the data still exists".
2. **Every byte the user did approve is at the destination**, at the path they'd go looking for it. A caller lists only
   the deliveries that hold under EVERY policy it drives (the source-only files); where a CLASHING file lands is a
   policy question, and clause 1 already covers it. An operation with no destination (delete) has no clause 2, and
   `safety_grid_tests.rs` says so per cell rather than passing an empty list and calling it covered.
3. **Every dest-only file the source didn't shadow is untouched**, byte for byte. That's the merge invariant.

**Decision**: the oracle is shared; the two merge FIXTURES stay separate. **Why**: `merge_tests.rs::make_rich_merge`
and `move_merge_tests.rs`'s `build_merge_source_tree` / `build_merge_dest_tree` are different trees, not two spellings
of one. The clash contents differ (`b"SRC-clash-larger"` versus `b"SRC-c"`), which is what decides whether
`OverwriteSmaller` resolves to an overwrite on the copy suite and to a skip on the move suite, and the move fixture
carries a second cross-type clash (`/album/swap2`). Unifying them would quietly weaken a policy assertion, which is the
worst possible outcome for a change whose proof is "both suites stay green". ❌ Don't fold the fixtures together as a
rider on something else; it's a policy-by-policy review of its own.

### The coverage grid

`safety_grid_tests.rs` covers op × cache state × outcome (Tier A, 27 cells) and the shape axis (Tier B, 12), all
asserted through the oracle above. **Its own module doc comment is the canonical statement** of the tier structure, the
per-op item kinds, and — the part that matters most — the explicit "NOT covered, and why" list. That's where an agent
adding a cell looks, so it lives there rather than being restated here and rotting. ❌ Don't add a cell without reading
that list first: several of the combinations it names are excluded because a double genuinely can't stand in for the
thing (no symlinks, no inodes), and adding them would assert the double rather than the code.

## Volume copy + move

**Destination paths arrive ANCHORED, and every re-anchoring here is idempotent.** The engine's callers (the Tauri
commands in `commands/file_system/volume_copy.rs`) pass the destination through `resolve_dest_path`, which anchors the
transfer dialog's volume-relative box at the dest volume's root; `file_system/volume/DETAILS.md` § "Path handling
gotchas" is the canonical account. Inside this directory the same `cmdr_fs::volume::root_anchored` re-derives the local
absolute path in four places that must agree on it: the both-local shortcuts in `copy.rs` / `move.rs`, the O_EXCL
reservation in `conflict.rs`, and the `note_pending_write_for_cmdr` registrations in `strategy.rs` / `move_same.rs`
(register a different path than the writer hits, and the downloads watcher treats Cmdr's own write as somebody else's).
❌ Don't replace one with a raw `root.join(path.strip_prefix("/"))`: that re-roots an already-absolute destination under
itself.

**The engine is reached through the facade.** `mod.rs` re-exports `copy_between_volumes` and `move_between_volumes` for the Tauri commands of the same name; every module under `volume/` is private to it. Both copy and move support conflict detection and resolution (Stop/Skip/Overwrite/Rename/OverwriteSmaller/OverwriteOlder) for all volume combinations (Local↔MTP, MTP↔MTP). Volume copy supports rollback (delete all copied files in reverse order with progress events, matching the local copy's `rollback_with_progress` pattern) and cancel cleanup (delete only the last partial file).

**Decision**: Cross-volume rollback records per-file destinations for a directory source, never the directory root.
**Why**: A directory source merges into an existing dest directory ("Overwrite means merge for dirs"), so dest-only files the user already had legitimately coexist in the merged tree. Recording the top-level dest directory in `copied_paths` and recursively deleting it on Rollback destroyed those untouched files — silent data loss on the one operation advertised as the safe undo. The local-FS path never had this bug because `CopyTransaction` records individual `created_files`. The volume path now mirrors that granularity: `copy_single_path` / `copy_directory_streaming` thread a `CreatedPaths` ledger (`strategy.rs`) that records every destination FILE the copy streamed plus every directory it NEWLY created (the `create_directory` call returned `Ok`, not `AlreadyExists`). On Rollback, `volume_rollback_with_progress` deletes the recorded files individually (reverse order), then prunes the newly-created dirs deepest-first with `prune_created_dir_if_empty`, which lists each one and removes it only if that listing came back empty — a created dir that still holds a pre-existing sibling stays put. A top-level FILE source still records its single landed path (the original after a safe-replace finalize, never the temp), so file→file Overwrite rollback is unchanged. Pinned by `copy_rollback_tests.rs::rollback_of_merged_directory_preserves_preexisting_dest_files`. **Don't** revert to recording the directory root or to a recursive delete for created dirs — either reintroduces the merged-dir data loss. The cleanup path can't reach one any more (§ "Three ways to delete, and who may use each"), which turns that "don't" into a fact about the code.

The same ledger must flow out of the **interrupted-mid-stream** path, not just the completed-copy path. A directory source cancelled/rolled-back/errored while still streaming its children returns `Err` from `copy_single_path`; both the serial transfer closure's `Err` arm and the concurrent task's `CopyTaskFailure` carry the per-file `CreatedPaths` ledger so the post-loop records the individual files (into `copied_paths`) and newly-created subdirs (into `created_dirs`) — and CLEARS `last_dest_path` for a directory source, so the Stopped/error partial-cleanup sweep is never handed the dest directory ROOT. On a merge that root holds pre-existing dest-only files. The sweep itself can no longer recurse into one (§ "Three ways to delete, and who may use each"), so the ledger discipline and the capability split are two independent defenses of the same file. A FILE source still routes its single partial dest/temp through `last_dest_path` (a genuine half-written partial, safe to remove). Pinned by `copy_rollback_tests.rs::{cancel_mid_merge_stream_preserves_preexisting_dest_file, rollback_mid_merge_stream_preserves_preexisting_dest_file, cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file}` (serial Cancel, serial Rollback, concurrent Cancel) and `rollback_after_rename_keeps_preexisting_dest_file` (file→file Rename rollback removes only the `name (1)` it landed). **Don't** drop the `created` ledger in the `Err`/cancel arms or let a directory source's dest root reach `last_dest_path`.

### Three ways to delete, and who may use each

**Decision**: `cleanup.rs` offers three deletes, split by capability, and exactly one of them recurses.

- **`delete_written_file(volume, path)`** — one `Volume::delete`, no listing, no recursion. The rollback loop over
  `copied_paths` and the post-loop `clean_partial_writes` sweep may call nothing else. A path that's already gone
  (`NotFound`) counts as success: the job is "make sure this isn't there", and a partial that never landed is not a
  failure worth a warn. Anything else (a non-empty directory's `ENOTEMPTY`, a permission refusal) comes back as a
  `PathedVolumeError` the caller logs.
- **`prune_created_dir_if_empty(volume, dir)`** — lists the directory and deletes it only when the listing comes back
  empty. A listing that FAILS leaves the directory standing: unknown is not empty.
- **`remove_tree(volume, path, preserve, why: TreeRemoval)`** — the recursive sweep, carrying the `preserve` set and the
  first-child-failure reporting (§ "Naming the item that failed"). `TreeRemoval` is fieldless with **no `Default` and no
  `From<bool>`**, and its three variants are the only authorizations that exist: `UserChoseOverwriteAcrossTypes`
  (`conflict.rs`, a cross-type Overwrite the user picked), `MoveSourceAfterDestinationLanded` (`move.rs`, after
  `flush_created_destinations`, with the skipped children in `preserve`), and `ArchiveMoveSourceAfterCommit`
  (`archive_edit/copy_into.rs`, remote originals after the rewrite durably commits). `remove_tree` logs the variant
  before it starts, so a recursive delete says in the log who authorized it.

**Why**: the ledger decisions above made cleanup per-FILE by DISCIPLINE — the paths that reach the sweep happen to be
files, and the sweep would recurse the day one wasn't. Both feeds into the rollback loop come through a single cell
(`copy_serial.rs`'s `last_dest_cell`, which holds a directory source's dest ROOT from the moment the transfer starts
until its result arrives), and whether a directory can survive that window is a property of the DRIVER, not of the
cleanup code. Splitting by capability makes the claim structural instead: no wrong `is_directory` belief can reach a
recursive delete, because on the cleanup path there is no recursive delete in scope. A fourth sweep has to answer
"authorized by what?" in the type. Pinned by `cleanup_tests.rs::{partial_sweep_leaves_a_directory_and_its_contents_alone,
rollback_leaves_a_directory_in_copied_paths_alone}`.

**Why the prune lists instead of trusting the `delete` contract**: `Volume::delete` promises not to recurse, every
shipping backend keeps that promise, and a conformance assertion holds them to it — but "the user's untouched files
survive rollback" then rests on a promise rather than on this code. `created_dirs` only ever holds a directory this
operation created, so a pre-existing user file gets inside one solely through a TOCTOU race with another writer; the
reason to check anyway is the NEXT backend, not this one. The cost is one listing per created directory, on the
rollback path, bounded by what the operation itself created. Pinned by
`cleanup_tests.rs::created_dir_prune_checks_emptiness_itself_even_on_a_recursive_backend`, which runs against a volume
whose `delete` recurses (`strategy_test_support.rs::RecursiveDeleteVolume`).

**Rejected: minting the recursive capability from a `DirectoryCreation::Created` record.** It guards the wrong thing. A
destination directory THIS operation created is exactly the one that's safe to sweep — nothing the user had can be
inside it — while the dangerous case is the MERGED directory we did not create, which a `Created`-minted token says
nothing about. `DirectoryCreation` is also itself a belief a backend supplies (MTP's `create_folder` can't signal a
collision at all), so keying a destructive branch on it would re-run this whole effort's bug one layer up. ❌ Don't
propose it again.

### A missing source hint means unknown, never "file"

**Decision**: A top-level source with no entry in `preflight.rs`'s `source_hints` map is resolved by probing `Volume::is_directory`, not by falling back to `SourceHint::default()`. `strategy.rs::resolve_source_is_directory` is the one place that decides; `copy_single_path` takes `Option<bool>` and routes through it, and both drivers plus the cross-volume move call it once per source and use the RESOLVED answer everywhere.

**Why**: the hint map can legitimately be empty. A LOCAL scan preview reaches the copy through the same `take_cached_scan_result` branch as a volume preview, and a `SourceHint::default()` claims `is_directory: false`. Two things broke on that lie, and the second is the dangerous one:

- `copy_single_path` trusts the flag absolutely, so a directory went down the FILE branch and the copy died on the backend's "can't read a directory" (`local_posix.rs`). Copying any local folder to SMB or MTP failed outright.
- Both drivers gate partial cleanup on "is this source a directory?" precisely so the post-loop sweep is never handed a destination directory ROOT (see the ledger decisions above). With the flag wrong, the dest dir's path reached `last_dest_path` (serial) / `in_flight_partials` (concurrent), and since dir-vs-dir merges silently, that path is the user's OWN pre-existing folder. A failed copy deleted it.

So the resolved answer, not the raw hint, must drive the ledger/cleanup branch as well as the streaming branch — resolving only for the copy call would leave the guard defeated. `Err` from the probe (the source can't be stat'd at all) fails that source rather than guessing.

**`SourceHint` has no `Default`, and it stays that way.** A `SourceHint::default()` isn't "no information": it's a confident `is_directory: false` that nobody established. `source_hints.get(p).copied().unwrap_or_default()` reads as harmless and is the exact line that shipped the bug above. Without the derive, the compiler refuses to produce that value at all, so absence has to stay an `Option` until something probes or fails. Removing it cost zero production changes (the two call sites were already fixed), which is the point: the derive was a loaded gun with nobody holding it.

### Every belief-default in this directory, decided

The compiler can't see a hand-written `volume.is_directory(p).await.unwrap_or(false)` — it has no type to refuse. Each of those sites was decided one at a time on one question: **can a wrong `false` reach a branch that deletes?**

- **`conflict.rs`'s source probe (no hint) — PROPAGATES.** `is_file_to_folder` is `!source_is_directory && destination_is_directory`, so a guessed `false` on a real FOLDER flips the cross-type latch on, and Overwrite's cross-type arm runs `remove_tree` over the user's destination folder. The old comment claimed the opposite of what the code did ("we'd rather over-prompt than route an unknown clash into the destructive file→folder latch"); `false` is precisely what routes it there. An unanswerable stat now fails the item.
- **`conflict.rs`'s destination probes (two of them, in the resolver and in `apply_volume_conflict_resolution`) — PROPAGATE, via `resolve_dest_is_directory`.** A guessed `false` reaches the same arm's bare `dest_volume.delete(dest_path)`. The helper keeps ONE exception, and it's load-bearing: `VolumeError::NotFound` means the destination raced away between detection and resolution, which is an ANSWER (nothing to protect), so it resolves as "not a directory" and the write proceeds. Failing there would break a write that would simply have succeeded.
- **`rename_merge.rs`'s `write_path` dir check — PROPAGATES.** A guessed `false` falls through to `exists()` and then `ctx.volume.delete(&write_path)`, aimed at a destination directory. Its input is now guarded upstream by the resolver, so this is the transient-fault residue plus defense in depth on the last destructive branch of the family.
- **`move_same.rs`'s source and destination probes — route through `strategy::resolve_source_is_directory` and the same NotFound-is-an-answer shape.** A wrong `false` here picks `resolve_volume_conflict` over `rename_merge_directory` for a folder-onto-folder collision. That degrades to a merge (`conflict.rs`'s `same_type_dir` catches it) rather than destroying anything, so it isn't a loss — but it's a branch chosen on a belief, and it mislabels the journal row's entry type, so it goes through the same helper as everything else.
- **`write_operations/rename.rs`'s probe — LEFT ALONE, with a comment saying why.** It feeds the journal snapshot's entry type only. A wrong value mislabels an undo entry and reaches no destructive branch.
- **`delete/walker.rs`'s no-preview probe** is the same family and is decided in `../../delete/DETAILS.md` § "What each branch does with a missing or wrong fact".
- **The two dest-inside-source guards — LEFT ALONE, deliberately.** `copy.rs` and `move_same.rs` each spell the probe `matches!(volume.is_directory(source).await, Ok(true))`, so an unanswerable stat means the guard doesn't fire. That's the safe direction here: the guard refuses a runaway (§ "Dest-inside-source guard on the same volume"), it deletes nothing, and a transfer whose source can't be stat'd fails on its own a moment later. Note the spelling is the same guess `.unwrap_or(false)` makes, so `desktop-rust-probe-unwrap-justified`'s method-plus-`unwrap_or` predicate doesn't see either one; that's why they're written down here instead.

The rule the list encodes: **a probe whose answer can select a destructive branch may not have a default.** "It isn't there" is an answer and may stay one; "I couldn't find out" fails the item.

**Cost**: exactly zero where hints exist. The probe fires only for a source the preflight didn't describe. ❌ Don't reintroduce a probe on the hinted path: it was removed because 15k MTP sources meant 15k parent listings, roughly two minutes of frozen dialog. Pinned by `copy_source_hint_tests.rs` (serial and concurrent × copies-the-subtree and spares-the-dest-folder, all against an EMPTY `per_path`). The root fix that keeps the map full lives in `../../DETAILS.md` § "A completed preview always carries `per_path`".

**The destination free-space pre-flight treats "can't tell" as permission to proceed.** `Volume::get_space_info` is explicitly allowed to answer `VolumeError::NotSupported` when the protocol has no way to ask, and `SftpVolume` does exactly that (`statvfs@openssh.com` isn't reachable from its crate stack). Both pre-flights — `scan_for_volume_copy`'s, which powers the dialog PREVIEW, and `copy_volumes_with_progress`' Phase 2 — go through `copy.rs::dest_space_if_known`, which maps `NotSupported` to `None` and skips the comparison. ❗ Every OTHER error still propagates: a destination that answered with a dead mount or a permission refusal has told us something, and walking past it would fail later and worse. ❌ Reading the refusal as "no room" makes such a volume a destination nothing can ever be written to; that shipped, and every copy into an SFTP server died in pre-flight after ~500 ms with `IoError { message: "Operation not supported by this volume type" }`, naming the destination path and nothing else. `VolumeCopyScanResult.dest_space` is `Option<SpaceInfo>` for the same reason: a backend that can't measure reports nothing rather than a number somebody invented. Pinned by `copy_tests.rs::{a_destination_that_cant_report_free_space_is_still_copyable_into, a_destination_that_does_report_free_space_still_refuses_what_it_cant_hold, a_preview_of_a_destination_that_cant_report_space_still_scans}` — the second is the one that stops the tolerance growing into ignoring a genuine "no room" — and end to end by `write_operations/sftp_transfer_integration_test.rs`.

**Dest-inside-source guard on the same volume.** `copy_volumes_with_progress` rejects copying a directory into its own descendant when `Arc::ptr_eq(source_volume, dest_volume)` (the command layer hands the same `Arc` for a same-volume-id copy). Without it, `copy_directory_streaming` re-lists each subdir live, so copying `/A` into `/A/sub` re-discovers and re-copies the files it just wrote — unbounded recursion that fills the volume (or overflows the streaming copy's stack). Returns `WriteOperationError::DestinationInsideSource`, mirroring the local-FS path's `validate_destination_not_inside_source`. Cross-DEVICE copies can't hit it (separate path spaces), so the guard is scoped to the same-volume branch and uses a path-prefix check (no `std::fs::canonicalize`, which doesn't apply to MTP/SMB/InMemory paths). Pinned by `copy_rollback_tests.rs::{same_volume_copy_into_own_descendant_is_rejected, same_volume_copy_into_sibling_dir_is_allowed}`.

**Self-collision: a source that would land on ITSELF duplicates, and never prompts.** The rule, why it isn't a conflict, and why no policy answer is acceptable for it: `../DETAILS.md` § "Self-collision (duplicating in place)". What's specific to volumes:

- **Identity without `dev+ino`.** No backend out here offers an inode, so `conflict.rs::is_the_same_item` asks `Arc::ptr_eq` (what everything in this directory means by "the same volume" — the command layer hands one `Arc` for a same-volume-id transfer) plus `is_the_same_volume_path`: the SAME parent directory, and a final component that folds together (`dest_name_index::fold`, NFC + lowercase, the same key `DestNameIndex` buckets destination names under). The leaf fold settles a case-differing name (SMB shares, macOS volumes) and an NFC/NFD-differing one, which is most of what `dev+ino` buys the local engine. A non-UTF-8 leaf can't be folded the way a backend would, so there only a byte-exact match counts. It's answered at the TOP of `resolve_volume_conflict`, before the destination probe and before the dir/dir short-circuit below, because both are wrong for this shape.
- **The PARENTS are compared as they are, never folded** (the rule lives on `is_the_same_volume_path`, and so does the ban on widening it). `fold` answers "would this backend treat these two names as the same, within one listing", and the only component a transfer has a listing for is the leaf. Whether `/DCIM` and `/dcim` are one directory is the backend's call: MTP is case-sensitive and an SMB share can be, so folding the parents makes a genuine cross-folder move (`/DCIM/photo.jpg` into `/dcim/`) look already-in-place — `move_same.rs` writes nothing, emits `Done`, and the user is told an item moved that didn't. The other mistake is much cheaper: a case-insensitive backend reached by a differently-cased route falls into the ordinary conflict path, which is exactly where such a transfer went before this rule existed. `commands/file_system/volume_copy.rs`'s pre-flight draws the same line, because the dialog and the engine have to agree. Pinned by `conflict.rs::tests::{a_case_differing_leaf_names_the_same_item, a_case_differing_parent_names_a_different_item}` and by `self_collision_tests.rs::moving_into_a_case_differing_folder_on_one_volume_really_moves`.
- **A directory's ` (N)` name is never reserved.** `find_unique_volume_name` takes its `exists()`-probe branch for a directory on EVERY backend, local-FS destination included. Its reservation is an `O_CREAT|O_EXCL` FILE, and one sitting where the copy is about to create a directory makes `merge_level`'s `create_directory` report `AlreadyExists`, so the walk would list and merge into it; the walker creating the directory itself is also what records it in `CreatedPaths`, and therefore what lets rollback remove it.
- **The names one operation hands out are remembered** (`state.claimed_names`, `unique_name::ClaimedNames`; the rule and the local twin are in `../DETAILS.md` § "Self-collision (duplicating in place)"). `find_unique_volume_name` claims its pick and walks past anything already claimed, on both branches. Out here that's load-bearing twice over: a directory never reserves at all (bullet above), and three or more top-level sources put the copy on the CONCURRENT driver, so several picks happen at once and the `exists()` probe of a backend with no exclusive create can't see a name whose bytes haven't landed. `photo.jpg` and `photo (1).jpg` duplicated together would otherwise both take `photo (2).jpg`.
- **A pre-known conflict naming the source itself never bulk-skips it.** The pre-flight matches by NAME, so a copy into the folder the sources already live in lists every one of them as conflicting; under `Skip all` `copy_volumes_with_progress`'s bulk-skip prelude would drop them all and the duplicate would silently do nothing. The prelude excludes them (`is_the_same_item`, the resolver's own question), the same exclusion the local engine makes.
- **No `dir_remap` equivalent is needed, and that asymmetry with the local engine is the point.** `merge_level` threads `dest_path` down through its OWN recursion (`child_dest = dest_path.join(&entry.name)`; it's `merge_level` that recurses, not its caller `copy_directory_streaming`, which only sets up the leaf pool), so renaming the top-level destination carries the whole subtree with it for free. The local engine can't: `FileInfo::dest_path()` recomputes `destination.join(relative)` from the original relative path on every call, which is why it needs `dir_remap`'s longest-mapped-ancestor rewrite.

**The move drops the item rather than renaming it**, in `move_same.rs` before the driver runs, so no engine below can represent the case. Only the same-volume path can reach one at all: `move_between_volumes` routes `Arc::ptr_eq` there FIRST, the both-local branch hands off to `move_files_start` (whose own rule lives in `../DETAILS.md`), and a genuine cross-volume move names two different volumes, so no two paths there name one item. Without the drop a folder reaches `rename_merge_directory`, which threads the destination down through recursion and renames every leaf onto itself or shuffles it aside to `name (1)`. Dropped items still emit `write-source-item-done` as `Done` with `source_removed: false`, and they count in `files_total` and `files_processed` so the tally matches what was asked for. Pinned by `self_collision_tests.rs`.

**Dir-vs-dir is NEVER a conflict — it always merges, silently.** `resolve_volume_conflict`'s first check after the self-collision one, and still before any policy lookup or `write-conflict` emit, is "are both sides directories?" — if so it returns the dest path as the merge target with no `replace_after_write`, regardless of `conflict_resolution`. A source folder landing on an existing same-named dest folder always merges into it; the configured **file** policy governs every clash _inside_ the merge. So even Stop / Skip / Rename merge the folder itself; only files ever prompt. The FE never sees a dir-vs-dir `write-conflict`. Cross-type clashes (file↔folder) are NOT merges — they keep the full conflict machinery (red file-over-folder warning, explicit Overwrite/Rename).

**Scan-as-you-merge: deep per-file conflicts resolved inline, one dest listing per merged level.** `merge.rs::copy_directory_streaming` discovers deep clashes as it walks, with no upfront recursive pre-scan. The trigger is `create_directory`'s result: `Ok(())` means WE created the level fresh (nothing can clash — skip the dest listing, stream every child straight in); `AlreadyExists` means we're MERGING into the user's pre-existing dir (list the dest level ONCE, build a `name → FileEntry` map, dispatch each clashing source child through `resolve_volume_conflict`). Dir-vs-dir children recurse unconditionally (no resolver call for the folder); a type mismatch routes through the resolver; a Skip leaves the dest child untouched. No per-child `get_metadata` probes — one listing per level, in-memory lookups after. Context is threaded via a `MergeCtx` struct (sink, op id, config, `state`, the op-wide apply-to-all latch cell, source hints) so `copy_single_path`'s signature doesn't grow per item. The merge engine is shared by all three pipelines: volume copy (serial `copy.rs` AND concurrent `copy_concurrent.rs`), and cross-volume move (`move.rs::move_volumes_with_progress`). `MergeCtx` is `None` only for the cross-volume move's _staging_ writes and tests that never merge.

**The two legs of a level run concurrently, unless a backend is single-transport.** `merge_level` drives the source
`list_directory` and the whole destination chain (`create_directory`, then the conditional dest `list_directory`) under
one `tokio::join!`, so a level costs `max(source, dest)` rather than their sum. On a cross-share merge each leg is a
full network round trip, and they're independent: the dest chain's shape depends only on what `create_directory`
answers, never on the source listing.

❌ **They stay sequential when either side reports `max_concurrent_ops() == 1`** (MTP: one USB bulk transport). The
device lock `MtpConnection` takes is released before the PTP transaction runs, so two overlapping calls would
interleave transactions on one phone. These two listings sit OUTSIDE the `FileWindow` that keeps every leaf serial
there, so they have to honor the same rule themselves.

❌ **Nothing that ORDERS anything may move into either leg.** Every ordering decision still happens in the
`for entry in &entries` loop after both legs land: which child prompts first, the apply-to-all latch, and the order
directories are created in. That is what keeps the serial-discovery guarantee below (§ "One window for the whole
operation", "What did NOT change") true while the two reads overlap.

**A skipped child credits both bars immediately.** `MergeChildDecision::Skip` calls `MergeCtx.on_file_skipped(bytes)`
beside `created.record_skip(...)`: `record_skip` is for the move sweep's preserve set, and `on_file_skipped` is what
moves the progress bars and keeps the skip out of the rate sample. A merge whose children all clash moves no bytes at
all, so without it both bars sit at `0 of N` for the whole run. The netting rule and the throttling behind it:
`../DETAILS.md` § "Skipped work moves the bars, and stays out of the rate".

**MTP can't signal collisions via `create_directory` — the merge walker pre-checks existence there.** Every backend except MTP returns `VolumeError::AlreadyExists` for an existing same-name dir (LocalPosix: `std::fs::create_dir`; SMB: smb2 typed STATUS_OBJECT_NAME_COLLISION; InMemory: explicit check). MTP's `create_folder` happily makes a same-name sibling object (the protocol allows duplicates), which would make the merge target the wrong dir. `Volume::create_directory_errors_on_existing_dir()` (default `true`, `false` for MTP) gates this: on MTP the walker pre-checks `exists()` with the one listing the merge level pays anyway, before creating.

### One window for the whole operation

**Decision**: a directory subtree's file copies run concurrently, through a SINGLE `strategy.rs::FileWindow` that the
whole operation shares — sized by the same `copy.rs::transfer_concurrency` that sizes the top-level driver's window, and
carried on `MergeCtx` so the three production pipelines (`copy_serial.rs`, `copy_concurrent.rs`, `move.rs`) each hand it
in where they already build one.

**Why**: `merge.rs` walked a tree with a plain serial `for entry in &entries { … .await }`, and the concurrent driver
fans out only across TOP-LEVEL sources. Select one folder — what a user actually does — and it is one source, so it
takes the serial driver and nothing inside it ever overlapped. `network.smbConcurrency` (advertised 1-32, default 10)
did nothing for the commonest copy there is. Measured on the Docker Samba stack, ~2.8 MB files inside one folder:
`docs/notes/transfer-subtree-concurrency-bench-2026-08-13.md`.

**The N² trap, and why the window is not per-level.** The driver already keeps `W` sources in flight. If each walker
opened a `W`-wide window of its own, one connection would carry `W²` files — 100 at the shipped default, far past where
throughput starts falling. So there is exactly ONE semaphore per operation and everything that writes a FILE takes a
permit from it: every walker's leaves at every depth, and the concurrent driver's top-level FILE tasks (a top-level file
IS a leaf). A top-level DIRECTORY task takes none — it holds no permit while it walks, which is what keeps a width-1
window from deadlocking on itself. ❌ Don't add a second width for the subtree; ❌ don't let a walker hold a permit
across a recursive descent.

**What did NOT change, deliberately.** Discovery stays serial: ONE walker descends in listing order, creates each
directory, and resolves every conflict on itself before that child's bytes join the window — the same rule
`copy_concurrent_source.rs` follows at the top level ("conflict resolution runs synchronously on the driver BEFORE a
task is spawned"). So prompt order, the `apply_to_all` latch, `conflict_dispatch_lock`'s role, and the order `created_dirs` is
recorded in are all exactly what they were. Rollback's "creation order, shallowest first" therefore survives untouched
within a source; across sources the concurrent driver already interleaved it, and the property that matters there is
only ancestor-before-descendant, which holds because a level can't start until its parent's `create_directory` returned.

**PLAN MODE stays serial.** `sequential_extract.rs` runs `copy_directory_streaming` with `plan: Some(_)`, which records
destinations and streams nothing, so there are no bytes to overlap. It builds the pool and never submits to it.
`merge: None` (the archive scratch pull, and tests that never merge) has no `MergeCtx` to carry a window, so it keeps a
strictly serial walk too.

**The walk drains before it returns, on every exit path.** A leaf still writing when `copy_directory_streaming` returned
would keep touching the destination after the driver moved on to cleanup, and its `created` record would land too late
for rollback to see. So the pool is drained after the walk finishes, fails, or hits a cancel. Under CANCEL that is
prompt: `stream_pipe_file` checks the intent per chunk, and the concurrent driver's cancel-drain deadline is the backstop
for a leaf that won't wind down. Under an ERROR it means the walk waits out the ≤ `W-1` leaves already streaming rather
than abandoning them — they complete, get recorded, and the FIRST failure is still the one reported (`cleanup.rs`'s
`remove_tree` follows the same first-failure rule). Pinned by
`merge_window_tests.rs::{a_cancel_mid_subtree_leaves_no_leaf_still_writing,
a_rolled_back_wide_window_copy_leaves_no_directory_it_created}`.

**One in-flight-table row per leaf.** `TaskProbe` is built on "one row, one write attempt": `arm_stall_abort` REPLACES
the row's token per attempt and `set_bytes` STORES (never adds) that attempt's count. Two overlapping leaves sharing one
row would clobber each other's stall-abort signal and keep resetting the watchdog's stillness clock, so each leaf gets
its own `begin_task` row and runs inside its own `CURRENT_TASK_PROBE` scope. That also makes the dump name the leaf that
wedged instead of the folder, and makes `WriteProgressEvent::activity.in_flight` a real measurement of how full the
window got. The top-level source's own row stays alongside them at `Spawned` (a phase the watchdog never acts on).

**Whoever builds a window owes the probe two things**, and both are one line each: `MergeCtx.op_probe`, so the leaves can
open rows; and the SAME width to `register_operation`, so the dump's `in_flight=<open>/<width>` names the fan-out rather
than the driver's own source loop. Miss the first and every leaf reports into the enclosing SOURCE's row through the
outer task-local — the clobbering above, silently, plus a dump that names the folder. Miss the second and the table
declares a width nothing uses. The cross-volume move missed both while running a `transfer_concurrency`-wide walk, and a
user's bundle shows what that costs: a 282-file folder to a NAS dumped `in_flight=1/1` with ten writes open, its one row
the top-level directory carrying a leaf's byte count. Pinned for both drivers side by side in `probe_row_tests.rs`,
which photographs the live table from inside a write.

**The live progress bar under-reports transiently, by design.** Both per-file progress callbacks were written for one
in-flight leaf: `SerialLeafProgress`'s `leaf_high_water` and `make_concurrent_per_file_progress`'s `last_file_bytes` are
each one watermark shared by every file in a subtree. With `W` leaves reporting into one watermark the bar lags by up to
`(W-1)` files' worth mid-flight. It never over-reports (both use `fetch_max`, so a lower report contributes nothing),
and on the SERIAL driver — the one a single-folder copy takes — it is exact at every completion boundary, because
`on_leaf_complete` adds each finished leaf's exact size to `byte_base`. ❌ Reshaping `progress.rs` for this is not worth
it; the bound is the contract.

**Known, pre-existing, and NOT caused by this**: the CONCURRENT driver's `make_concurrent_per_file_progress` never
resets `last_file_bytes` between the files of one subtree (`on_file_complete` there only bumps the file counter), so a
multi-file directory copied through that driver credits roughly `max(file sizes)` rather than their sum — its final byte
total is under-reported, not just its live bar. `SerialLeafProgress` does it correctly. Out of scope by David's decision
(2026-08-13); fixing it means resetting the watermark per leaf and compensating on completion, the way the serial type
already does.

### Answering the pre-check from one listing

Before spawning each top-level source, the CONCURRENT driver has to know whether something already sits at that name; if so, conflict resolution runs. Asked as a `dest_volume.get_metadata(dest_item_path)` per source it is one round trip PER FILE, serialized on the driver, and no window width can overlap it — a batch of N files carries a hard floor of `N × RTT`. Measured against David's QNAP at 3.7 ms RTT: **2.378 s of a 3.224 s best run for 500 files, 74%** (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).

Three answers, cheapest first, in `copy.rs`'s spawn loop:

1. **Nothing to ask.** THIS operation created the destination directory (Phase 0.5's `create_directory_all` answered `DirectoryCreation::Created`): nothing the user already had can be inside a folder that didn't exist a moment ago, so there is neither a probe nor an index. Same rule the deep-merge walker one level down has always used (see "Scan-as-you-merge" above).
2. **The listing Phase 0.6 already paid for.** `reap_stale_transfer_temps` does one `list_directory` of `dest_path` on every copy, merges included, immediately before the spawn loop. It now RETURNS that listing (minus the temps it reaped), and the driver indexes it into a `DestNameIndex` (`dest_name_index.rs`) the loop consults in memory. This is the ordinary F5 copy's case: `TransferDialog` seeds the destination with the opposite pane's current folder, which exists, so a merge is what most copies are.
3. **The per-file probe**, for anything the index won't answer.

**Decision (2)**: answer the merge case from the one listing, and accept that it is a snapshot.
**Why**: the round trip is spent either way, so the cost side is zero; the alternative is `N × RTT` of pure serialized latency that no other change can remove.

#### The staleness trade, stated plainly

The listing is taken once, at operation start. By file 400 of a large batch it can be MINUTES old. **A file that arrives at the destination mid-batch is missed: an Overwrite replaces it with no prompt, a Skip doesn't skip it.** That is a real narrowing of the guarantee, not a free win, and it is wider than the created-directory case's window (which needs someone to target a folder Cmdr made seconds ago).

David weighed exactly this and chose it (2026-08-02), with the alternative on the table. ❌ **Do NOT "fix" it with re-listing, polling, a freshness window, or a re-probe before Overwrite.** Each buys back part of the latency this removes, and the simple version is the decision. If the trade ever needs revisiting it's a product call, not a cleanup.

#### Why a name lookup is not a `get_metadata`, and how the gap is closed

The two are NOT equivalent, and every gap is a conflict that becomes a silent overwrite. `DestNameIndex` therefore answers `Absent` only for a name no backend could route onto an entry it holds; everything else is `Unknown` and falls through to the probe, which stays authoritative. Wrong-way-round costs one round trip; wrong-way-forward costs the user's file.

- **Case.** SMB shares and macOS volumes are typically case-INsensitive, so `get_metadata("foo.txt")` finds a stored `Foo.txt`. Entries are bucketed under a folded key, and a fold-only match is `Unknown`, not a hit — whether two spellings are one file is the destination filesystem's call, and a case-SENSITIVE destination legitimately holds both.
- **Unicode normalization.** macOS and SMB move paths between NFC and NFD (`SmbVolume::to_smb_path` NFC-normalizes everything it sends), so one user-visible name is two byte strings. The fold is NFC + lowercase, so both spellings share a bucket. An ASCII fast path skips the normalizer without changing the answer.
- **Trailing dots and spaces.** Win32 path canonicalization strips them from the request, so a Windows-hosted share resolves `report.` onto a stored `report`. The trimmed form is checked too, and a hit concedes the probe.
- **8.3 short names.** `PROGRA~1` is a generated second name for an entry the listing reports under its real one — an alias namespace a listing cannot enumerate, so a miss can't be proven. Any name containing `~` concedes the probe. Cheap: such names are rare.
- **A name we can't read as UTF-8**, and a source path with no final component (the destination is the directory itself), are both `Unknown`.

❌ **A listing that failed is not an answer of "nothing is there".** `reap_stale_transfer_temps` returns `None` when its `list_directory` errors or is cancelled, and the driver then probes every source. Fail safe, never fast. (The API returns `Result<Vec<FileEntry>>` with no truncation signal, so a partial listing isn't representable; an error is the failure mode there is.)

#### Where it deliberately does NOT apply

- **A LOCAL destination keeps every per-file probe** (`!dest_volume.operations_are_local()` gates the index). `LocalPosixVolume::get_metadata` is a microsecond `stat`; folding every name in a folder that might hold 200k entries to copy three files into it is the worse trade, and local→local behavior is unchanged bit for bit.
- **Scoped to the concurrent loop, so MTP is untouched by construction.** `MtpVolume::max_concurrent_ops()` is 1, so a phone always takes the serial driver, which keeps its own per-file probe. That matters: an MTP `get_metadata` lists the entire parent directory (~18 s for 1046 photos on a cold cache), so MTP wants its own decision about this, not this one. The serial path pays at most a couple of probes anyway (it runs for `< 3` sources or a window of 1).
- ❌ **"Created by us" is not "exists and is empty".** A directory that already existed can gain an entry from another process between any two instants; one we created cannot have held anything BEFORE we made it. Only the second claim licenses skipping the question outright. ❌ Never relax this into an emptiness check.
- ❌ **Losing the create race is not creating.** If `create_directory` answers `AlreadyExists` because another process won, `create_directory_all` reports `AlreadyExisted`: it is somebody else's directory and may already hold something.
- **Conflict DETECTION changed; resolution did not.** A hit carries the same `size` / `is_directory` the probe supplied (a `FileEntry` either way), and `resolve_volume_conflict` and everything under it are untouched.

Pinned by `copy_precheck_tests.rs` (end to end, against a destination that resolves names case- and normalization-insensitively like a real share: an exact-match map turns those cases green while the user's data is gone) and `dest_name_index_tests.rs` (the matching rule alone).

**The conflict-dispatch mutex serializes the human across concurrent / nested merges.** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next to `conflict_slot` — same concern: one human, one oneshot slot) guards the whole Stop-mode dispatch inside `resolve_volume_conflict`: acquire → check `is_cancelled` (bail with `Cancelled` if so — load-bearing: a dropped sender on cancel unblocks only the ONE awaiting task, so a task parked on the mutex must not then emit a prompt nobody will answer, a hang) → re-check the latch (a prior "…all" answer collapses this queued prompt) → emit + await → store latch → release. Released on every exit path, NEVER held across the subsequent file write — serialize the human, not the I/O. The concurrent spawn loop's top-level dispatch and every deep merge acquire the SAME lock. Known acceptable residual: a prompt already emitted before another task latched "…all" isn't retroactively resolved — a rare extra prompt, never a data risk. Pinned by `merge_dispatch_mutex_tests.rs` (concurrent-two-deep-clashes, top-level-vs-deep race, cancel-while-queued no-hang).

**The merge invariant.** A merge never deletes or overwrites a dest file the source doesn't shadow — under every file policy, on every backend, including cancel and rollback mid-merge. Pinned by `merge_tests.rs::merge_never_deletes_unshadowed_dest_files_under_every_policy` (the property test) and the SMB integration pin `smb_integration_merge_deep_clash_skip_all_preserves_dest_only_files`.

**The move invariant: no byte is ever lost.** A move is copy-then-delete-source, so every source file must end up readable from the destination (it moved) or the source (it didn't). The hazard is a folder merge: the deep walker can resolve individual children to Skip, and a skipped child never reached the destination, so its source copy is the only one in existence. `move_volumes_with_progress` therefore sweeps a directory source with `remove_tree(…, MoveSourceAfterDestinationLanded)`, passing `CreatedPaths::skipped_source_paths()` as `preserve` — the source path the merge walker records for each skipped child. The sweep deletes a directory only once its whole subtree is gone, so preserving one leaf keeps its ancestor spine; a child that FAILS to delete counts as preserved too, so the parent isn't attempted (an `ENOTEMPTY` on top of the real leaf error tells the user nothing). This mirrors the local path's `delete_dir_preserving_skipped` (§ "Cross-FS move source-delete preserves Skipped sources" above), which had it first. It matters on ordinary use, not just an explicit Skip: `OverwriteSmaller` / `OverwriteOlder` reduce to Skip **per file**, so "Overwrite all smaller" on a folder move hits this for every non-qualifying child. The **same-volume** move is inherently correct — its rename-merge leaves a skipped child in place and each level's cleanup is an empty-only `Volume::delete`, so a surviving child keeps its directory. Both are pinned by `move_merge_tests.rs::{move_folder_merge_never_loses_a_byte_under_every_policy, same_volume_move_folder_merge_never_loses_a_byte_under_every_policy}`, which drive every file policy (including the Stop-mode answers) and assert each source file is readable from one side or the other. **Don't** sweep a merged source folder unconditionally.

**The conflict dialog never fabricates a destination size.** `resolve_volume_conflict` reports `destination_size` from the caller's hint, else from the `get_metadata` it already does for the mtime annotation, else `None` ("unknown" in the dialog); a folder destination is always `None` (the volume layer never walks a remote tree for a size). A deep-merge child carries no top-level hint, so the old `Some(dest_size_hint.unwrap_or(0))` reported "Existing: 0 bytes" for files that had content. That number is not cosmetic: the dialog's answer feeds `reduce_volume_conditional_resolution`, so a fabricated `0` made every destination look strictly smaller and silently degraded "Overwrite all smaller" into an unconditional overwrite (and, on a file→folder clash, into a recursive delete of the destination folder). `merge.rs::resolve_merge_child` now passes the dest `FileEntry` the walker already listed for `dest_by_name`, so the common case costs no extra round trip. Pinned by `merge_tests.rs::{deep_merge_clash_reports_the_real_destination_size, overwrite_all_smaller_keeps_a_larger_destination_on_the_first_deep_clash}`.

**Overwrite means merge for dirs, replace for files, enforced architecturally, not by trait contract.** `apply_volume_conflict_resolution` stats the dest first; for directories it skips the delete entirely (the recursive copy merges into the existing tree). This is enforced at the call site rather than relying on `Volume::delete`'s "file or empty directory only" contract. A future backend with recursive delete semantics, or a refactor that consolidates `delete` + `delete_recursive`, would otherwise silently flip the UX from merge to wholesale replace and delete files unique to dest. The `dir_overwrite_must_merge_not_replace_even_with_recursive_delete` test in `conflict.rs` pins this with a wrapper Volume that violates the trait contract.

**Cross-volume file→file Overwrite is a safe-replace, NOT a delete-then-write.** A cross-volume file Overwrite (Local↔SMB↔MTP↔USB) must never destroy the existing destination before the new bytes are fully written — otherwise a mid-stream failure (network drop, USB yank, cancel) leaves the user with neither the old file nor a complete new one. So `apply_volume_conflict_resolution`'s file→file branch does NOT delete the dest. It returns a `ResolvedConflict { write_path: <temp sibling>, replace_after_write: Some(orig) }`: the streaming writer lands bytes in a `<name>.cmdr-tmp-<uuid>` sibling on the dest volume, and only after the temp is fully written does the caller call `finalize_safe_replace(dest_volume, temp, orig)`, which deletes `orig` (which survived the whole write) then `rename(temp, orig, force=false)`. On any failure the original is untouched and the existing partial-cleanup sweep removes the temp.
- **Why explicit delete-then-rename, not `rename(force=true)`:** MTP's `rename(force=true)` does NOT delete an existing destination — it can create a duplicate. SMB(force=true) deletes-then-renames internally and Local replaces atomically, but the finalize must be uniform across all backends, so it always deletes `orig` first then renames into the now-absent slot. There is a tiny window between the delete and the rename where neither name resolves, but the complete new data lives in the temp throughout, so a crash there leaves a recoverable `.cmdr-tmp-*` sibling rather than data loss. If the `delete(orig)` fails, `finalize_safe_replace` returns the error WITHOUT deleting the temp (the new data must survive).
- **Threading:** `resolve_volume_conflict` / `apply_volume_conflict_resolution` return `Option<ResolvedConflict>`. The three streaming write sites (`volume::copy` serial + concurrent, `volume::r#move` cross-volume) carry `replace_after_write` through to their `transfer_one` work, track the TEMP as the in-flight partial (so cancel/error cleanup removes the temp, never the original), and after a successful `copy_single_path` call `finalize_safe_replace` and record the ORIGINAL (not the temp) in `copied_paths` / the milestone for rollback bookkeeping. The cross-volume move finalizes BEFORE deleting the source (a move must never delete the source if the dest isn't fully in place). When `replace_after_write` is `None`, behavior is byte-for-byte identical to before.
- **The post-write temp is committed data, NOT a cleanable partial.** `finalize_safe_replace` deletes the original first, then renames the temp in. If the rename fails after the delete succeeded (disconnect at that instant), the temp holds the ONLY complete copy of the new data. The partial-cleanup contract ("delete partials on error") must NOT touch it — leaving a recoverable `.cmdr-tmp-*` artifact is the correct outcome. Each write site stops treating the temp as a partial the moment `copy_single_path` returns `Ok`, BEFORE finalize runs: the **serial** closure clears `last_dest_cell` to `None` up front; the **concurrent** task returns its Err as `(path, error, cleanup_temp)` where a finalize failure sets `cleanup_temp = false` so the result handler skips adding the temp to `last_dest_path` (a stream failure sets `true` and cleans as before); the **cross-volume move** has no dest partial-cleanup at all, so its temp survives a finalize failure unconditionally. Pinned by `copy_crashsafe_tests.rs::{cross_volume_overwrite_serial_preserves_new_data_on_finalize_failure, cross_volume_overwrite_concurrent_preserves_new_data_on_finalize_failure}` and `move_failure_tests.rs::cross_volume_move_preserves_new_data_on_finalize_failure` (a `MoveRenameFailsDestVolume` double whose `rename` always errors).
- **What's still delete-first:** cross-type Overwrite (file→folder recursive-delete, folder→file delete) keeps the delete-first behavior — a type swap is rare and already a wholesale content replacement, and there's no volume-level temp+rename atomicity for a type change. Same-type dir→dir still merges (no delete). **Same-volume move** (`move_within_same_volume`, the `volume.rename` path) keeps the legacy delete-first overwrite shape: its resolver collapses a `ResolvedConflict` with `replace_after_write: Some(orig)` back to "delete `orig`, rename source straight onto it" — rename is atomic-ish and not a stream, so the safe-replace temp dance buys nothing there.
- Pinned by `conflict.rs::{file_overwrite_keeps_original_until_temp_is_written, finalize_safe_replace_swaps_temp_over_original}` and `copy_crashsafe_tests.rs::{cross_volume_overwrite_preserves_dest_on_midstream_failure, cross_volume_overwrite_success_replaces_and_cleans_temp, cross_volume_overwrite_concurrent_replaces_and_cleans_temp}`.

**Cross-volume move source-delete is recursive.** `move_between_volumes` in `move.rs` deletes the source via `cleanup.rs::remove_tree`, authorized as `MoveSourceAfterDestinationLanded`, when the source is a directory. The `Volume::delete` contract is "file or *empty* directory": `LocalPosixVolume::delete` calls `std::fs::remove_dir` which fails ENOTEMPTY, so deleting a populated source directory after a cross-volume copy must walk the tree. Regression coverage: the `remove_tree_*` tests in `copy_tests.rs`. The original failure mode (data at both source and dest, FE shows generic `io_error`) traced back to this; the SMB collision that surfaced on retry was just the second-order symptom.

**`write-error` carries a typed, word-free `WriteOperationError` for both move and copy.** Both `move_between_volumes` and `copy_volumes_with_progress` funnel every `?`-propagated failure through the shared `WriteFailure` struct (in `transfer_error.rs`). `WriteFailure::from_volume(path, e)` maps an originating `VolumeError + path` to a `WriteOperationError` (one spot to map, via `map_volume_error`); `WriteFailure::synthetic(write_err)` wraps an already-typed error (cancellation, validation, synthetic IoError). The shared `write_error_event_from(...)` helper builds the `WriteErrorEvent` via `WriteErrorEvent::new` from any `WriteFailure`. The FE renders all copy and classification (including provider-specific suggestions) from the typed `error` via `transfer-error-messages.ts`; no prose crosses IPC. Both move and copy paths land at the same FE quality.

**Volume copy/move must skip `write-error` emit on `Cancelled`.** `copy_volumes_with_progress` / `move_*` inner handlers already emit `write-cancelled` before returning `Err(Cancelled)`, so the outer `copy_between_volumes` / `move_between_volumes` wrapper must match on `WriteOperationError::Cancelled { .. }` and NOT also emit `write-error`, otherwise the frontend logs a user-initiated cancel as an error. This mirrors `../mod.rs`'s `Ok(Err(Cancelled)) ⇒ no-op` branch for the generic `start_write_operation` path; the volume paths don't go through `../mod.rs`, so they carry their own version of the check. Related: cancellation must propagate as `VolumeError::Cancelled(msg)`, not `VolumeError::IoError { message: "Operation cancelled" }`; the `matches!(WriteOperationError::Cancelled)` check at the outer layer relies on the typed variant. `SmbVolume`'s streaming reader and `map_smb_error`'s `ErrorKind::Cancelled` arm both return `VolumeError::Cancelled` to stay consistent.

## One-pass sequential extract (compressed tar / solid 7z sources)

A directory source on a SEQUENTIAL archive (a compressed tar or solid 7z) can't be read entry-by-entry without
re-decoding the prefix in front of each file, so the normal per-entry walk would extract a subtree in O(n²). The copy
engine routes it to a one-pass path instead. `copy_single_path`'s directory branch checks
`source_volume.extraction_is_sequential(source_path)`: when `true` it calls `extract_sequential_subtree`; otherwise
(any real FS, a plain `.tar`, a zip) it keeps `copy_directory_streaming` unchanged — **zero regression for random-access
sources**.

`extract_sequential_subtree` runs two phases:

1. **Plan** — it calls `copy_directory_streaming` in PLAN MODE (`plan: Some(&ExtractPlan)`). Plan mode reuses that
   function's entire merge machinery — it creates the whole destination directory structure (walking the tree, so empty
   and synthetic dirs land too), resolves every file's conflict (policy, Stop-prompt, apply-to-all latch, type
   mismatches, safe-replace, Rename reservation), and records newly-created dirs in `created` for rollback — but instead
   of streaming each file's bytes it records the resolved destination (`PlannedWrite { dest_path, replace_after_write }`)
   in the plan, keyed by the file's full source path, and streams nothing.
2. **Data** — it opens `source_volume.open_sequential_extract(source_path)` (the archive's one-pass extractor, decode
   ONCE; mechanism in `crates/cmdr-archive/src/read/DETAILS.md` § "One-pass subtree
   extract") and walks the files in ARCHIVE order. Each file the plan kept is streamed through the destination's
   `write_from_stream` (same safe-overwrite temp+rename, downloads-watcher registration, fsync, and
   `finalize_safe_replace` safe-replace as `stream_pipe_file`), recorded in `created`, and reported via `on_file_complete`
   / `on_file_progress`. A file the plan SKIPPED (conflict resolution said skip) is drained and dropped.

Why split plan from data: the merge decisions are naturally TREE-ordered (list each dest level once) while the one-pass
decode is ARCHIVE-ordered; precomputing the plan lets the data pass be a simple archive-order lookup-and-write, and reuses
the data-safety-critical merge/conflict/rollback code in `copy_directory_streaming` verbatim rather than reimplementing
it. **Progress** is honest: the plan pass touches no bytes (a fast tree walk over the cached index), and the data pass
emits real per-file byte progress as each member lands. **Cancellation** is checked between members in the data pass (and
between entries in the plan pass, by `copy_directory_streaming`'s existing check), so a cancel stops cleanly between files
— the in-flight partial is cleaned by `write_from_stream`'s abort, exactly as on the per-entry path. Archive sources
report `max_concurrent_ops() == 1`, so this always runs on the serial copy path. Pinned by
`strategy_sequential_tests.rs` (nested-subtree correctness, the random-vs-sequential routing gate, empty
dirs + symlinks + out-of-order entries, and cancel-between-members).

## Letting the server do a same-volume copy

Duplicating a file inside one remote volume used to pull every byte down the link and push it straight back up. A
protocol that can copy for itself (SFTP's `copy-data`) skips both halves, so `stream_pipe_file` asks
`Volume::copy_within` before it opens a stream. MOVE has had a same-volume fast path since forever
(`move.rs` → `move_within_same_volume`, a server-side rename); this is COPY's.

**Eligibility is `Arc::ptr_eq` on the two volumes**, which is exact rather than approximate: `routing.rs` resolves both
sides through the volume registry, so one volume id yields one `Arc`. ❗ Asking a DIFFERENT volume to copy `from`
inside itself is not a failure — on a server that happens to hold a same-named file it is the wrong file, copied
silently.

**`Ok(None)` means "do it the ordinary way"**, and that is the answer for everything except a clean success and a
cancel:

- Two different volumes.
- `NotSupported`, from a backend with no server-side copy or from one whose SERVER lacks the extension.
- ⚠️ **Any other failure.** The streaming loop below carries the retry policy, the stall watchdog, and the pause
  checkpoints, so a fast path that failed for a real reason fails there too with better handling and a better report.
  The cost is one doomed extra attempt on a genuinely broken destination, which is the cheaper side of the trade.
- ❌ **A cancel is never one of them.** The intent is consulted as well as the error variant, so a backend that labels
  its own stop something other than `Cancelled` can't turn a Cancel click into a second, full-speed attempt.

**It stages exactly like a streamed write**, with the REQUESTED staging rather than `resolve_staging`'s: the destination
genuinely holds a byte-incomplete file while a server-side copy runs, so the single-shot exemption can't apply however
small the file is. The quit deadline (`state.backend_abort`) rides the same `select!` it rides for a streamed write, and
cleans up nothing for the same reason.

⚠️ **A pause doesn't land mid-file here**, only at the next file boundary. There is no stream to park between chunks,
and pausing frees nothing anyway — no bytes are crossing the link. Same limitation, and the same reasoning, as the
local-FS chunk loop's.

Cells: `strategy_server_side_copy_tests.rs` (eligibility, the fallback, staging, cancel) with a counting double;
`crates/cmdr-sftp/src/volume/copy_test.rs` for what a real server does, including a fixture that lacks the extension.

## The single-shot exemption

**Decision**: a write the DESTINATION performs as one indivisible operation skips the staging and goes straight to the
file's final name (`WriteStaging::SingleShot`). The destination answers `Volume::write_is_single_shot(size)`;
`volume::strategy::resolve_staging` is the only place that upgrades a `Stage` to it, and only ever a `Stage` (a caller's
safe-replace temp keeps the ORIGINAL alive until the new bytes are complete, which is strictly stronger). Today SMB is
the only backend that answers `true`; MTP, local FS, archives, and in-memory keep the trait default of `false`.

**Why it's safe**: staging buys exactly one property — no window in which the final name holds a byte-incomplete file.
A single SMB2 compound frame has no such window. The client sends one length-prefixed frame carrying
CREATE+WRITE+FLUSH+CLOSE; the server either receives it whole and runs all four ops or discards it and creates nothing,
and it needs nothing further from the client to finish. So the force-quit that started all this (kill the process
mid-transfer, no error path, no `Drop`, no cleanup) cannot produce a truncated file on this path.

**❌ Why it is NOT "small files are fine"**: smallness merely correlates with single-shot-ness today, through
`max_write_size`. A caller-side size threshold would go on claiming the guarantee the day a backend retuned its
fast-path condition, and the failure is silent: truncated files at real names, discovered months later. So the condition
is asked of the destination, and the SMB backend answers with the SAME function its fast path branches on
(`smb/streams.rs::fits_one_compound_write`, on the negotiated `max_write_size`, with `size > 0` because an empty file
has no WRITE to compound with and takes the streaming writer). Two copies of that threshold IS the bug; don't
introduce one.

**Backend obligations** taken on with a `true` answer, both in `crates/cmdr-smb/src/volume/streams.rs`:

- The drained buffer, not the promised size, decides the final branch. A source that yields SHORT still goes out as one
  compound frame rather than dropping into the multi-round-trip streaming writer, which would be a broken promise at an
  unstaged final name.
- A compound that fails AFTER the server's CREATE (out of space, over quota) leaves a 0-byte file at that name, so the
  backend deletes it (`create_succeeded_but_write_failed`, which reads smb2's typed per-command status). A CREATE
  failure is NOT cleaned up: nothing was created, and any pre-existing file there is untouched, so deleting would be
  data loss. `StagedWrite::abandon` is a no-op for a single-shot write for the same reason — only the backend can tell
  those two apart.

**Residual risk, accepted (transport)**: `create_succeeded_but_write_failed` reads a typed `smb2::Error::Protocol`, so
it only fires when the SERVER answered and named the failing command. A TRANSPORT failure mid-frame (the connection
drops before any response) is not a `Protocol` error, so nothing is cleaned up, and the server may still have processed
the CREATE — leaving a 0-byte file at the real name. This is not fixable from here rather than merely unfixed: with the
connection gone there is no session to delete through, and the client cannot know whether the server got the frame at
all. It is also the narrowest window on this path (one frame, no client round trip inside it), which is exactly why the
exemption is scoped to single-shot writes and nothing wider.

**The hard-abort tier IS armed for a single-shot write**, unlike the stall watchdog (which never is — `../DETAILS.md`
§ "The guards, each load-bearing"). The difference is when each fires. An abort only ever fires with the process seconds
from exiting, so dropping the compound frame mid-send produces exactly what the process dying produces, which is the
transport residual risk above and nothing new. A stall abort fires on a live app, where it WOULD be a new,
client-initiated instance of it. `../DETAILS.md` § "Two tiers of cancel".

**Residual risk, accepted**: a source stream that reports a `total_size` smaller than the bytes it then yields, past the
compound limit, falls back to the streaming writer at an unstaged final name. That needs a source lying about its own
length (a file being appended to under us) AND a force-quit inside a 2–3 round-trip window, and what would be left at
that name is what the source actually gave us. The alternative (failing such a copy outright) is worse.

Pinned by `strategy_single_shot_tests.rs` (both directions: single-shot writes at the final name with no rename;
too big, or a backend that makes no promise, still stages; a caller temp is never converted),
`staged_write::tests::a_single_shot_write_targets_the_final_name_and_needs_no_landing`, `cmdr-smb`'s `streams_test.rs` (the boundary of
`fits_one_compound_write`, and no promise without a live session), and — against real Samba —
`cmdr-smb`'s `streaming_integration_test.rs::smb_integration_a_single_shot_write_leaves_as_one_compound_frame`, which counts wire
frames to prove the promised write really is one compound frame.

## Pause and the concurrent copy path

The serial drivers (`drive_transfer_serial_{sync,async}`) call `wait_while_paused_{sync,async}` at each per-source loop top, right after the `is_cancelled` check, so local copy/move, the cross-volume *serial* path, and delete all honor pause between files; the cross-volume serial path additionally parks between chunks (see above).

**The concurrent copy path is deliberately NOT gated for mid-batch pause.** `copy_volumes_with_progress`'s `FuturesUnordered` path (several files in flight at once) has no single "between files" boundary to park at, so it does **not** honor mid-batch pause: its per-file progress callback (`make_concurrent_per_file_progress`) stays **cancel-only** (it breaks on `is_cancelled`, ignores `paused`), like the serial per-file callback. A pause on a concurrent-path op takes effect once the in-flight batch drains to the next admission point. (Threading the `CheckpointStream` checkpoint into the concurrent path too is possible — each in-flight file already streams through `stream_pipe_file` — but isn't wired yet; the admission-point framing is the current contract.) Pinned by `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`.

## Overwrite isn't reversible

**Decision**: Overwrite does NOT keep a backup of the replaced original. Rollback removes the files the operation created, but it can't restore an original that an Overwrite (or Overwrite-with-rename) replaced.

**Why**: The obvious "make it reversible" fix is to retain a `.cmdr-backup-<uuid>` of every overwritten file for the operation's duration and delete the backups on commit. But that backup consumes drive space the user doesn't expect: a large multi-file Overwrite would briefly hold a full second copy of everything it overwrites, and on a near-full disk that can fail the operation — or fill the drive — exactly when the user is trying to free space. We judge "rollback can't undo an overwrite" to be the lesser surprise than "Overwrite filled my disk," so we accept the current behavior until users actually ask for reversible overwrites. The mechanics today: `safe_overwrite_file` uses temp+rename-aside+rename (the original is intact until the new content is fully in place), then **deletes** the aside in step 4 rather than retaining it. `CopyTransaction::rollback` and `MoveTransaction::rollback` therefore only un-create new files / reverse new renames.

**If you revisit this**: the three sites that would need backups are `overwrite::safe_overwrite_file` (step 4, the aside deletion), `state::CopyTransaction::rollback`, and `transfer/move_op.rs::MoveTransaction::rollback`. Each carries a pointer comment back here. Any future "retain backup" design must bound the extra disk footprint (for example, a size cap that falls back to no-backup, or an explicit pre-flight space check that reserves 2× the overwrite footprint) — don't reintroduce the unbounded-backup footgun this decision exists to avoid.

## Naming the item that failed

**Decision**: a transfer failure travels as `PathedVolumeError { path, error }` (`transfer_error.rs`), not a
bare `VolumeError`, from `copy_single_path` / `copy_directory_streaming` / `extract_sequential_subtree` /
`remove_tree` out to the three drivers. The `AtPath::at()` helper attaches the path at the frame that
knows it. Both phases of a cross-volume move are covered: the copy AND the source delete.

**Why**: one `copy_single_path` call can walk an entire subtree, so the error a driver receives may come from a file
thousands of entries below the top-level item the user selected. With a bare `VolumeError` the driver's only available
path was that top-level item, and it used it: a folder move that tripped on one unwritable file reported nothing but
the folder's own name. That is undiagnosable — the folder is fine, one leaf is not, and the leaf's name is the entire
content of the report. The originating path exists only inside the walker; once the error leaves without it, it cannot
be reconstructed.

**Which SIDE the path came from travels with it too** (`PathRole`, same module). A `VolumeError::NotFound` says nothing
about which volume answered, and the two readings are opposites for the user: `SourceNotFound` means "your file is
gone", `DestinationNotFound` means "there was nowhere to put it". `map_volume_error` takes the role as an argument, so
every call site has to decide; inferring it downstream is impossible anyway, since the only signal available is the
path's shape and a volume-relative destination (`/photos`) looks exactly like a source path. The destination-side sites
are the three `create_directory_all(dest_path)`
calls (`copy.rs` Phase 0.5, `move.rs`, `move_same.rs`), the dest space query, and the dest `is_directory` probes;
everything else is source-side.

**Why it matters**: with one shared `SourceNotFound`, a share that couldn't address the destination reported the user's
intact source file as missing. A NAS user hit exactly that (`v0.38.1`): the dialog claimed the destination folder
didn't exist, then the transfer claimed the source file didn't either, naming a file they had never touched. Two
opposite diagnoses for one destination fault, and the alarming one was the wrong one.

**`From<PathedVolumeError>` is `PathRole::Source` by construction, not by default**: every `at()` site labels the error
with the SOURCE item the walker was on (that IS the type's purpose above), so the path it carries is a source path even
when the failing call wrote to the destination. Naming that a destination would attach a destination verdict to a
source path. A dest-side caller that needs the other verdict maps explicitly through
`WriteFailure::from_volume(path, PathRole::Destination, e)`.

**Where each driver attaches it**:

- Cross-volume move (`move.rs`) and serial copy (`copy_serial.rs`) map with `e.path`, never the loop's
  `source_path`.
- The concurrent driver carries TWO paths on `CopyTaskFailure` (`copy_concurrent_task.rs`) and they are not
  interchangeable: `failed_path` is the DESTINATION entry to drop from `in_flight_partials` and possibly clean, while
  `reported_path` is the SOURCE item the user is told about. Merging them would either clean the wrong path or report
  the dest dir root.
- `pull_path_to_local` deliberately drops back to a bare `VolumeError`: it materializes into a scratch dir that is
  discarded wholesale on failure, so no consumer reads a per-item path.

**The source-delete phase, and what a directory sweep reports**: `remove_tree` (`cleanup.rs`)
keeps sweeping after a child fails, so it clears everything it can, and it remembers the FIRST child failure with that
child's own path. When the directory's own `delete` then fails, that remembered child comes out instead of the
directory's `ENOTEMPTY` — the surviving child is the diagnosis and the parent's refusal is only its symptom, named
after the folder the user selected. When the directory DOES go, the sweep returns `Ok`: nothing survived to report, and
promoting a child failure there (a race with another deleter, say) would turn a finished move into a reported failure.
Both `remove_tree` callers (cross-type Overwrite in `conflict.rs`, and `archive_edit/copy_into.rs`) only log, and they
log `e.path` alongside the root they asked for, so the log names the leaf too. Rollback and partial cleanup don't reach
this walker at all — they delete one node each, so their failure already names the only path they asked about.

**That `Ok` rests entirely on `Volume::delete` REFUSING a non-empty directory**, which is the trait's contract but was
not what SMB did until smb2 0.18.0: `delete_directory` used `FILE_DELETE_ON_CLOSE`, and Samba answers that with
`STATUS_SUCCESS` on a non-empty directory and then deletes nothing. Under the old behavior this sweep would have
returned `Ok` with the whole subtree still on disk, and a cross-volume move would have reported success on a source it
never touched. ❌ A backend whose `delete` can silently succeed on a non-empty directory must not use this walker until
it can't. Verify the contract holds when adding a backend (`volume/DETAILS.md` § "Trait capability model"), and treat
an smb2 downgrade below 0.18.0 as breaking this specific carve-out.

**Don't** re-collapse this to `VolumeError` for tidiness, and don't `.at()` one frame up from the failure — a path
attached by the parent names the parent, which is exactly the bug.

Pinned by `move_failure_tests.rs::cross_volume_move_error_names_the_child_that_failed_not_the_selected_folder`
(copy phase),
`move_failure_tests.rs::cross_volume_move_delete_error_names_the_child_that_failed_not_the_selected_folder`
(delete phase), and `copy_tests.rs::remove_tree_reports_the_leaf_that_refused` (the walker
itself). The `UndeletableSource` double (`strategy_test_support.rs`) stages it: `InMemoryVolume` alone can't,
because its `delete` drops a directory entry whether or not the directory still holds children.
