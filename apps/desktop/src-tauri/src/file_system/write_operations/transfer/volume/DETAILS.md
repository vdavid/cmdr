# Cross-volume transfer details

Pull-tier docs for `transfer/volume/`: the copy and move engine that spans Local ↔ MTP ↔ SMB ↔ archive backends, its
merge and conflict semantics, the rollback ledger, the destination pre-check, the single-shot staging exemption, and how
a failure names the item it happened on. Must-know invariants live in `CLAUDE.md`.

The shared scaffolding this engine runs on is documented one level up, next to the code that owns it: staging
(`../DETAILS.md` § "File writes are staged"), the parked-chunk pause (§ "Pause reaches between chunks"), the stall
signal (§ "The stall signal"), per-file retry and its watchdog (§ "Retrying one FILE, and the watchdog that ends a wait
nothing else will"), and cancel/rollback against a parked driver (§ "Cancel and rollback reach a parked driver").

The both-local copy delegation preserves an explicit leaf name in its config. The serial drivers pass that leaf to
`landing.rs::top_level_precheck`, which substitutes it before probing the destination. Contract:
`../DETAILS.md` § "Named destinations".

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
  `stream_pipe_file`, both cancel tiers, `copy_single_path`); the vocabulary both halves and both drivers speak
  (`FileWindow`, `MergeCtx`, `MergeProbe`, `CreatedPaths`) sits beside it in `merge_ctx.rs`. `merge.rs` walks a tree (`copy_directory_streaming`, `resolve_merge_child`): a directory child
  recurses there, a file child goes to `strategy.rs`. `sequential_extract.rs` reuses the same walk in plan mode, which
  is why the merge/conflict/rollback code is not reimplemented for one-pass archives. `folder_dates.rs` holds what the
  walk notes about the folders it created, dated once the subtree landed (§ "Copies keep the source's date").
- **The move is three files, and the dependency runs ONE way.** `r#move` is the DISPATCHER and nothing else: it picks
  same-volume (`move_same`), both-local (`move_files_start`, one level up), or cross-volume (`move_cross`), then owns
  the managed-op lifecycle around whichever it picked. Both engines are leaves under it. ❗ Nothing in an engine may
  import the dispatcher back — that is exactly what the three `FetchFut` / `ResolveFut` / `TransferFut` aliases used to
  do from `r#move`, and three lines of type alias were the whole of a three-module cycle. They live with the driver
  whose contract they are now (`../transfer_driver/mod.rs`), which also single-sources the shape `copy_serial.rs` had
  been restating privately. Shared vocabulary between the dispatcher and its engines belongs in `transfer_driver` or in
  `preflight.rs`, ❌ never in the dispatcher. One engine may hand off to the other: `move_same` calls
  `move_cross::move_volumes_with_progress` when a rename copies (§ "A same-volume move whose renames copy"), and
  `move_cross` names nothing back, so the edge stays one-way.
- **`conflict.rs` DECIDES; two thin siblings act and answer.** The resolver keeps the policy (`resolve_volume_conflict`,
  `apply_volume_conflict_resolution`, the conditional reduction). `landing.rs` answers where a name lands before any
  of that (§ "Look-alike names and new-name spelling"). `finalize.rs` holds the
  landing (`temp_sibling_path` + `finalize_safe_replace`), which no resolver call reaches: the five write sites
  (`copy_serial.rs`, `copy_concurrent_task.rs`, `merge.rs`, `sequential_extract.rs`, `move_cross.rs`) call it once
  their stream is done, long after the resolver returned. `item_identity.rs` holds `is_the_same_item` /
  `is_the_same_volume_path`, asked by three callers that resolve no conflict at all (`copy.rs`'s bulk skip,
  `move_same.rs`, `../../routing.rs`'s pre-flight). Neither belongs behind a policy module the callers don't want.
- **`conflict.rs`'s tests are three `#[path]` children, one per question the resolver answers**: `conflict_tests.rs`
  (merge safety, the overwrite ordering, the finalize swap), `conflict_conditional_tests.rs` (`OverwriteSmaller` /
  `OverwriteOlder`, on hints and on `get_metadata`), and `conflict_cross_type_tests.rs` (a blanket policy never crosses
  types). `conflict_same_item_tests.rs` is a `#[path]` child of `item_identity.rs`, which is where its subject lives.
  A test name reads `conflict::<child>::<test>` rather than the flat `conflict::tests::`.
- **`move_file.rs` is the per-FILE cross-volume move**, under the operation-level driver in `move_cross.rs`: one
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
  rather than growing one file.
- **The copy suite is split by contract, all declared from `copy.rs`, and `copy_tests/` is the general suite rather
  than the default one.** It is a directory of five children named after what each holds: `scan.rs` (config defaults,
  the skipped-count suffix, the scan/preview happy path over both volume types, and the batch-scan progress callback),
  `progress.rs` (the multi-file execution matrix and what a running copy reports), `conflicts.rs` (the Skip and
  Overwrite policies plus the two cross-type overwrites), `cancellation.rs`, and `destination.rs` (the destination
  auto-create, and a destination fault naming itself rather than the source). Each of the others owns a named contract:
  `copy_space_tests.rs` (the destination free-space pre-flight as a matrix: can't tell / a real ceiling / no ceiling,
  each asserted through BOTH the preview and the copy, since the two ask independently; plus a destination spanning
  several filesystems, judged by its FOLDER's filesystem in both directions),
  `copy_precheck_tests.rs` (is this top-level name taken), `copy_prescan_reuse_tests.rs` (what the scan's findings let
  the copy skip — the size hint instead of a rescan, the pre-known conflicts as one upfront bulk skip — and the
  skip-mode-only and stale-entry guards on both), `copy_crashsafe_tests.rs`, `copy_rollback_tests.rs`,
  `copy_cancel_tests.rs`, `copy_retry_tests.rs`, `copy_source_hint_tests.rs` (the ABSENT hint specifically, the other
  half of the pair),
  `copy_staged_write_tests.rs`, `copy_extract_out_tests.rs`, `copy_window_tests.rs`, and the two
  `copy_concurrent*_tests.rs`. The fixtures `make_state` / `make_volumes` live in `copy_tests/mod.rs` and every
  sibling reaches them as `super::tests`. ❗ Tests for a symbol ANOTHER module owns belong to that module's suite, not
  here: `map_volume_error`'s are `transfer_error_tests.rs`, `remove_tree`'s are `cleanup_tests.rs`. Both sets sat in
  the copy suite once, which is how `remove_tree` came to look untested from `cleanup_tests.rs`.
- **The same-volume rename-merge suite is six files split by subject**, all declared from `mod.rs` and sharing the
  fixtures in `rename_merge_test_support.rs` (which also holds the reason the whole family runs on `LocalPosixVolume`
  over a tempdir: `InMemoryVolume` models neither real subtree-rename nor empty-only-delete semantics).
  `rename_merge_tests.rs` holds the merge semantics (prompts, file policy, source-dir cleanup, the dest-inside-source
  guard, symlinks, and why a merged folder isn't reversible); the other five each carry the backend or rig their family
  needs: `rename_merge_cancel_tests.rs` (a volume that cancels on the first child rename),
  `rename_merge_case_fold_tests.rs` (a `CaseInsensitiveVolume` for the late-detected collision),
  `rename_merge_walk_tests.rs` (a counting volume for the no-subtree-walk perf pin), `rename_merge_stat_tests.rs` (a
  stat that refuses to answer), and `rename_merge_mtp_tests.rs` (a virtual MTP device).
- **Naming a destination is `naming.rs`, not `conflict.rs`.** `find_unique_volume_name` (plus `resolve_local_path`, the
  root-anchoring its `O_EXCL` reservation needs) walks the shared `unique_name::NameCandidates` and owns only HOW a pick
  is held on a volume; `conflict.rs` decides POLICY and asks it for a name. Its tests are `naming_tests.rs`.
- **Duplicating in place is pinned once, for both engines**, in `self_collision_tests.rs` (declared from `mod.rs`, so `super::` is `volume`): it drives the real `copy_volumes_with_progress` and `move_within_same_volume_with_progress` over one `InMemoryVolume` used as BOTH sides, and pins the folding cases at the `resolve_volume_conflict` seam, because `InMemoryVolume` is itself case- and normalization-sensitive.
- **The move suite is six files split by subject, all declared from `move_cross.rs`** (beside the engine they mostly drive; `super::super` is `volume` from there, the same as before): `move_tests.rs`
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
- **A `*_tests.rs` file is a `#[path]` CHILD of the module it pins**, not a sibling of it: `copy_tests/mod.rs` is
  `volume::copy::tests` and `strategy_pause_tests.rs` is `volume::strategy::pause_tests`. So inside one, `super::` is
  that parent module and `super::super::` is `volume` — one level shallower than the same text at file scope in
  `copy.rs`. A `copy_tests/` child sits one level deeper again, so `super::` there is the suite and `super::super::` is
  `copy`. Check which scope you are in before touching a `super::` chain here; a wrongly-deepened one can still
  compile against a same-named module at the other level.
- **`copy_bench.rs` is `#[ignore]`d** and needs a QNAP NAS plus `SMB2_TEST_NAS_PASSWORD`, so it never runs in CI.
- **Four test-support files, split by what they fake.** `faulty_volume_test_support.rs` holds `FaultyVolume` (wrap any
  volume, arm the Nth call to a named operation to fail) plus `forward_volume_methods!`, the macro every `Volume` double
  in this directory builds its boilerplate from — a new double lists the methods it forwards and hand-writes only what
  it lies about. `strategy_test_support.rs` holds the doubles whose lie is behavioral (`RecursiveDeleteVolume`,
  `UndeletableSource`, `FlakyDest`, the yield/pause sources); `strategy_dest_yield_test_support.rs` holds the two upload
  destinations the destination-yield suite drives (`ForegroundBusyDest`, whose share signal a test scripts through a
  `BusyVolumes`, and `PanicIfProbedDest`, which fails loudly if a non-opting target is ever probed);
  `copy_wedge_test_support.rs` holds the STREAM doubles
  (`GatedChunkStream`, `IncrementalDest`), which control WHEN bytes arrive and are a different axis entirely. ❌
  `FaultyVolume` over an `InMemoryVolume` can't show a half-written destination — the in-memory store buffers a whole
  file and creates it at the end — so partial-destination cells use `LocalPosixVolume`.
- **An armed fault that never fires reads exactly like a passing test.** A cell that arms `FaultyOp::IsDirectory` on the
  source and then lets the operation run a REAL preflight scan gets a confident `is_directory` hint in `source_hints`,
  so `resolve_volume_conflict` takes its `Some(hint)` arm and never probes. The cell then asserts the UNFAULTED
  behavior — for a cross-type Overwrite, the documented rename-aside of the destination — while reading as though it
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
**Why**: A directory source merges into an existing dest directory ("Overwrite means merge for dirs"), so dest-only files the user already had legitimately coexist in the merged tree. Recording the top-level dest directory in `copied_paths` and recursively deleting it on Rollback destroyed those untouched files — silent data loss on the one operation advertised as the safe undo. The local-FS path never had this bug because `CopyTransaction` records individual `created_files`. The volume path now mirrors that granularity: `copy_single_path` / `copy_directory_streaming` thread a `CreatedPaths` ledger (`merge_ctx.rs`) that records every destination FILE the copy streamed plus every directory it NEWLY created (the `create_directory` call returned `Ok`, not `AlreadyExists`). On Rollback, `volume_rollback_with_progress` deletes the recorded files individually (reverse order), then prunes the newly-created dirs deepest-first with `prune_created_dir_if_empty`, which lists each one and removes it only if that listing came back empty — a created dir that still holds a pre-existing sibling stays put. A top-level FILE source still records its single landed path (the original after a safe-replace finalize, never the temp), so file→file Overwrite rollback is unchanged. Pinned by `copy_rollback_tests.rs::rollback_of_merged_directory_preserves_preexisting_dest_files`. **Don't** revert to recording the directory root or to a recursive delete for created dirs — either reintroduces the merged-dir data loss. The cleanup path can't reach one any more (§ "Three ways to delete, and who may use each"), which turns that "don't" into a fact about the code.

The same ledger must flow out of the **interrupted-mid-stream** path, not just the completed-copy path. A directory source cancelled/rolled-back/errored while still streaming its children returns `Err` from `copy_single_path`; both the serial transfer closure's `Err` arm and the concurrent task's `CopyTaskFailure` carry the per-file `CreatedPaths` ledger so the post-loop records the individual files (into `copied_paths`) and newly-created subdirs (into `created_dirs`) — and CLEARS `last_dest_path` for a directory source, so the Stopped/error partial-cleanup sweep is never handed the dest directory ROOT. On a merge that root holds pre-existing dest-only files. The sweep itself can no longer recurse into one (§ "Three ways to delete, and who may use each"), so the ledger discipline and the capability split are two independent defenses of the same file. A FILE source routes its dest/temp through `last_dest_path` (serial) or `in_flight_partials` (concurrent) ONLY when that path can hold OUR partial: the caller's safe-replace temp, or a name the resolver claimed (`../staged_write.rs::failed_write_leaves_ours_at`). ❗ A plain staged write (`WriteStaging::Stage`) never qualifies: its bytes are on a `.cmdr-tmp-*` sibling `stream_pipe_file` abandons itself, and its final name was free when the driver looked, so whatever is there after a failure is someone else's. The commonest way to get there is exactly that: another writer took the name mid-upload and the no-replace landing refused, and the sweep used to delete their file. Pinned by `copy_landing_race_tests.rs` (both drivers) and, on live WebDAV, SFTP, and SMB servers, `a_name_taken_mid_upload_is_never_replaced`. A write that fell back to the final name unstaged (a destination that can't rename) cleans its own partial there instead (`remove_unstaged_partial`). Pinned by `copy_rollback_tests.rs::{cancel_mid_merge_stream_preserves_preexisting_dest_file, rollback_mid_merge_stream_preserves_preexisting_dest_file, cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file}` (serial Cancel, serial Rollback, concurrent Cancel) and `rollback_after_rename_keeps_preexisting_dest_file` (file→file Rename rollback removes only the `name (1)` it landed). **Don't** drop the `created` ledger in the `Err`/cancel arms or let a directory source's dest root reach `last_dest_path`. A task the concurrent driver ABANDONS at its cancel-drain deadline never reaches either arm, so the driver holds an `Arc` on each in-flight task's `CreatedPaths` (`CopyTask.created`) and `ConcurrentDriver::finish` hands over, and journals, what every abandoned directory source had already landed. Pinned by `copy_cancel_tests.rs::rollback_undoes_what_an_abandoned_folder_task_already_landed`.

### Three ways to delete, and who may use each

**Decision**: `cleanup.rs` offers three deletes, split by capability, and exactly one of them recurses.

- **`delete_written_file(volume, path)`** — one `Volume::delete`, no listing, no recursion. The rollback loop over
  `copied_paths` and the post-loop `clean_partial_writes` sweep may call nothing else. A path that's already gone
  (`NotFound`) counts as success: the job is "make sure this isn't there", and a partial that never landed is not a
  failure worth a warn. Anything else (a non-empty directory's `ENOTEMPTY`, a permission refusal) comes back as a
  `PathedVolumeError` the caller logs.
- **`prune_created_dir_if_empty(volume, dir)`** — lists the directory and deletes it only when the listing comes back
  empty. A listing that FAILS leaves the directory standing: unknown is not empty.
- **`remove_tree(volume, path, why: TreeRemoval)`** — the recursive sweep, with the first-child-failure reporting
  (§ "Naming the item that failed"). `TreeRemoval` is fieldless with **no `Default` and no `From<bool>`**, and its
  one variant is the only authorization that exists: `UserChoseOverwriteAcrossTypes` (`displaced_destination.rs`,
  discarding what a cross-type Overwrite the user picked set aside, once its replacement landed). A move's SOURCE is never a tree removal, into a zip included: it goes through the ledger
  sweep in `source_sweep.rs` (§ "The move invariant"). `remove_tree` logs the variant
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

- **`conflict.rs`'s source probe (no hint) — PROPAGATES.** `is_file_to_folder` is `!source_is_directory && destination_is_directory`, so a guessed `false` on a real FOLDER flips the cross-type latch on, and Overwrite's cross-type arm replaces the user's destination folder (today by setting it aside, which success then discards as a tree). The old comment claimed the opposite of what the code did ("we'd rather over-prompt than route an unknown clash into the destructive file→folder latch"); `false` is precisely what routes it there. An unanswerable stat now fails the item.
- **`conflict.rs`'s destination probes (two of them, in the resolver and in `apply_volume_conflict_resolution`) — PROPAGATE, via `resolve_dest_is_directory`.** A guessed `false` reaches the same arm's replace-the-destination branch. The helper keeps ONE exception, and it's load-bearing: `VolumeError::NotFound` means the destination raced away between detection and resolution, which is an ANSWER (nothing to protect), so it resolves as "not a directory" and the write proceeds. Failing there would break a write that would simply have succeeded. It asks `Volume::entry_kind`, so a LINK at the destination is a leaf too: a merge into one lands the files in its target (`../DETAILS.md` § "Symlinks are opaque to a move").
- **`rename_merge.rs`'s `write_path` dir check — PROPAGATES.** A guessed `false` falls through to `exists()` and then `ctx.volume.delete(&write_path)`, aimed at a destination directory. Its input is now guarded upstream by the resolver, so this is the transient-fault residue plus defense in depth on the last destructive branch of the family. It asks `entry_kind`, because a `true` here descends: a link that took the name after the resolver freed it must not read as a directory to merge into (pinned by `link_following_backend_tests.rs::a_link_racing_into_a_freed_name_is_never_merged_into`).
- **`move_same.rs`'s source and destination probes — PROPAGATE, through `move_same.rs::source_merges_as_a_directory` and `Volume::entry_kind`, with the same NotFound-is-an-answer shape.** A wrong `false` here picks `resolve_volume_conflict` over `rename_merge_directory` for a folder-onto-folder collision and mislabels the journal row's entry type. Neither asks `strategy::resolve_source_is_directory`: its probe is `Volume::is_directory`, which some backends answer by following a link, and a wrong `true` for a link is the dangerous direction here (a merge through it renames the target's children out).
- **`write_operations/rename.rs`'s probe — LEFT ALONE, with a comment saying why.** It feeds the journal snapshot's entry type only. A wrong value mislabels an undo entry and reaches no destructive branch.
- **`delete/walker.rs`'s no-preview probe** is the same family and is decided in `../../delete/DETAILS.md` § "What each branch does with a missing or wrong fact".
- **The two dest-inside-source guards — LEFT ALONE, deliberately.** `copy.rs` and `move_same.rs` each spell the probe `matches!(volume.is_directory(source).await, Ok(true))`, so an unanswerable stat means the guard doesn't fire. That's the safe direction here: the guard refuses a runaway (§ "Dest-inside-source guard on the same volume"), it deletes nothing, and a transfer whose source can't be stat'd fails on its own a moment later. Note the spelling is the same guess `.unwrap_or(false)` makes, so `desktop-rust-probe-unwrap-justified`'s method-plus-`unwrap_or` predicate doesn't see either one; that's why they're written down here instead.

The rule the list encodes: **a probe whose answer can select a destructive branch may not have a default.** "It isn't there" is an answer and may stay one; "I couldn't find out" fails the item.

**Cost**: exactly zero where hints exist. The probe fires only for a source the preflight didn't describe. ❌ Don't reintroduce a probe on the hinted path: it was removed because 15k MTP sources meant 15k parent listings, roughly two minutes of frozen dialog. Pinned by `copy_source_hint_tests.rs` (serial and concurrent × copies-the-subtree and spares-the-dest-folder, all against an EMPTY `per_path`). The root fix that keeps the map full lives in `../../DETAILS.md` § "A completed preview always carries `per_path`".

**The destination free-space pre-flight compares only where a CEILING exists.** Two different silences reach it and both mean "go ahead", which is why `copy.rs::room_to_check` collapses them into one `Option<u64>`. The first is a backend that can't measure: `Volume::get_space_info` is explicitly allowed to answer `VolumeError::NotSupported`, and `SftpVolume` does exactly that on a server without `statvfs@openssh.com`, so `dest_space_if_known` maps it to `None`. The second is a backend that measured and found no ceiling: `SpaceInfo::Unbounded`, which a Nextcloud account with no quota reports and which is the DEFAULT state of a real account, so it is the common case rather than an edge one. Its `available_bytes()` is `None` by construction, and that is the whole point of the enum: an `available` with no `total` can't be built, so no comparison site has to remember to skip it. ❗ Every OTHER error still propagates: a destination that answered with a dead mount or a permission refusal has told us something, and walking past it would fail later and worse. ❌ Reading either silence as "no room" makes such a volume a destination nothing can ever be written to; that shipped once, and every copy into an SFTP server died in pre-flight after ~500 ms with `IoError { message: "Operation not supported by this volume type" }`, naming the destination path and nothing else. The helper asks `Volume::get_space_info_at(dest_path)`, the filesystem the destination FOLDER is on, ❌ never the volume's one figure: a phone over ADB mounts a read-only system image at `/` that reports 0 free beside its shared storage and any SD card, and a volume-wide answer refused every copy onto a real Pixel. Backends on one filesystem inherit the default, which is their `get_space_info`. `copy_volumes_with_progress`' Phase 2 goes through it. Pinned by `copy_space_tests.rs`, which is exactly this matrix: the three answers (can't tell, a real ceiling, no ceiling), the real ceiling being the one that stops the tolerance growing into ignoring a genuine "no room", and end to end by `write_operations/backend_suites/sftp_transfer_integration_test.rs`. A real ceiling that refuses is still a question for the person: the refusal is `InsufficientSpace`, and "Copy anyway" re-runs the copy with `SpaceShortfall::Proceed`, which skips the Phase 2 comparison. Unlike the local engine, this one never looks at what's already at the destination (`../../DETAILS.md` § "The free-space pre-flight").

**A destination folder that takes no writes is refused BEFORE its space is measured.** `copy.rs::destination_refusal` asks `Volume::write_access_at(dest_path)` first: in `copy_volumes_with_progress` (Phase 0.4, before Phase 0.5 creates the folder), in `move_volumes_with_progress` (before its Phase 0 create), and in `move_within_same_volume_with_progress` (before its create). An `Unwritable { reason }` answer becomes `WriteOperationError::DestinationNotWritable { path, reason }`, which the frontend words per reason and about the FOLDER, never the device. Why before: a phone's `/` over ADB is a read-only system image reporting 0 free, and with only the space check a copy onto it said "Not enough space: the destination needs X but only has 0 bytes available" (observed on a Pixel 9 Pro XL, 2026-09-10), true of `df` and wrong as the reason. `Unknown` goes ahead, ❌ never a refusal, for the same reason `NotSupported` does in the space check: SMB over smb2, SFTP, and WebDAV have no way to ask without writing. It's a pre-flight, so the answer can go stale before the first write; a write refused later still fails, with the backend's own error. What each backend answers: `Volume::write_access_at` in `crates/cmdr-fs/src/volume/mod.rs` (default `Unknown`), `local_posix.rs::write_access_for_path` (`statvfs` `ST_RDONLY` plus `access(W_OK)`, so read-only and no permission are told apart), `MtpVolume` (a storage the device reports read-only), and `AdbVolume` (`test -w`, where exit 1 is `Unexplained`: `crates/cmdr-adb/DETAILS.md`). Pinned by `copy_space_tests.rs` § "The destination folder takes no writes", by `move_tests.rs` and `move_same_tests.rs` (the refusal leaves the source whole and creates nothing), and end to end by `adb_transfer_test.rs::a_copy_onto_a_phones_root_is_refused_as_not_writable_rather_than_as_out_of_space`.

**A volume nobody has connected yet is refused as such, before anything starts.** `routing.rs` (copy, move, compress), `../../delete/volume_start.rs`, and `commands/file_system/volume_copy.rs` (the copy preview and the conflict check) resolve their volumes first, and an empty lookup goes through `transfer_error.rs::unregistered_volume_error`, which asks `crate::unregistered_volumes::why_unregistered`. A device its provider lists but nobody dialed, or a saved SFTP / WebDAV server nobody connected, becomes `WriteOperationError::SourceNotConnected` / `DestinationNotConnected { path }` (the preview's `VolumeScanError::*VolumeNotConnected`), and the frontend says "Not connected yet" and points at the volume switcher, with no Retry. Any other id is a volume that left the registry: as a SOURCE it becomes `SourceNoLongerConnected { path }` ("Not connected anymore", plug the phone back in or reopen the server; a phone unplugged under a search-results pane is how a person gets there), and as a destination it stays the `IoError` naming the volume, as an unmount race is. The scan preview (`commands/file_system/write_ops.rs::scan_preview_source_volume`) refuses any unregistered non-local id up front with `ScanPreviewRefusal::SourceNotConnected`, ❌ never walking it on the Mac (frontend side: `apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "When the dialog can't find out"). `map_volume_error` maps a backend's `VolumeError::NotConnected` to the same pair by `PathRole`. ❌ Never `DeviceDisconnected`: no session existed to drop. ❌ Never "volume not found" for a listed one either: the user reads a place that's gone, when opening it is the way through. A refusal never dials; only a pane's connect does. Pinned by `adb_transfer_test.rs` § "A phone the switcher lists but nobody dialed" (copy, move, delete, new folder, preview, against the fake ADB server), `server_volumes_test.rs::a_copy_onto_a_saved_server_nobody_connected_is_refused_as_not_connected`, and `routing/tests.rs` (the mapping row, an unknown destination id staying a missing volume, and an unknown source id reading as no longer connected).

**Dest-inside-source guard on the same volume.** `copy_volumes_with_progress` rejects copying a directory into its own descendant when `Arc::ptr_eq(source_volume, dest_volume)` (the command layer hands the same `Arc` for a same-volume-id copy). Without it, `copy_directory_streaming` re-lists each subdir live, so copying `/A` into `/A/sub` re-discovers and re-copies the files it just wrote — unbounded recursion that fills the volume (or overflows the streaming copy's stack). Returns `WriteOperationError::DestinationInsideSource`, mirroring the local-FS path's `validate_destination_not_inside_source`. Cross-DEVICE copies can't hit it (separate path spaces), so the guard is scoped to the same-volume branch and uses a path-prefix check (no `std::fs::canonicalize`, which doesn't apply to MTP/SMB/InMemory paths). Pinned by `copy_rollback_tests.rs::{same_volume_copy_into_own_descendant_is_rejected, same_volume_copy_into_sibling_dir_is_allowed}`.

**Self-collision: a source that would land on ITSELF duplicates, and never prompts.** The rule, why it isn't a conflict, and why no policy answer is acceptable for it: `../DETAILS.md` § "Self-collision (duplicating in place)". What's specific to volumes:

- **Identity without `dev+ino`.** No backend out here offers an inode, so `item_identity.rs::is_the_same_item` asks `Arc::ptr_eq` (what everything in this directory means by "the same volume" — the command layer hands one `Arc` for a same-volume-id transfer) plus `is_the_same_volume_path`: the SAME parent directory, and a final component that folds together (`dest_name_index::fold`, which is `cmdr_fs::name_fold`'s NFC + lowercase, the same key `DestNameIndex` buckets destination names under). The leaf fold settles a case-differing name (SMB shares, macOS volumes) and an NFC/NFD-differing one, which is most of what `dev+ino` buys the local engine. A non-UTF-8 leaf can't be folded the way a backend would, so there only a byte-exact match counts. It's answered at the TOP of `resolve_volume_conflict`, before the destination probe and before the dir/dir short-circuit below, because both are wrong for this shape.
- **The PARENTS are compared as they are, never folded** (the rule lives on `is_the_same_volume_path`, and so does the ban on widening it). `fold` answers "would this backend treat these two names as the same, within one listing", and the only component a transfer has a listing for is the leaf. Whether `/DCIM` and `/dcim` are one directory is the backend's call: MTP is case-sensitive and an SMB share can be, so folding the parents makes a genuine cross-folder move (`/DCIM/photo.jpg` into `/dcim/`) look already-in-place — `move_same.rs` writes nothing, emits `Done`, and the user is told an item moved that didn't. The other mistake is much cheaper: a case-insensitive backend reached by a differently-cased route falls into the ordinary conflict path, which is exactly where such a transfer went before this rule existed. `commands/file_system/volume_copy.rs`'s pre-flight draws the same line, because the dialog and the engine have to agree. Pinned by `conflict_same_item_tests.rs::{a_case_differing_leaf_names_the_same_item, a_case_differing_parent_names_a_different_item}` and by `self_collision_tests.rs::moving_into_a_case_differing_folder_on_one_volume_really_moves`.
- **A directory's ` (N)` name is never reserved.** `find_unique_volume_name` takes its `exists()`-probe branch for a directory on EVERY backend, local-FS destination included. Its reservation is an `O_CREAT|O_EXCL` FILE, and one sitting where the copy is about to create a directory makes `merge_level`'s `create_directory` report `AlreadyExists`, so the walk would list and merge into it; the walker creating the directory itself is also what records it in `CreatedPaths`, and therefore what lets rollback remove it.
- **The names one operation hands out are remembered** (`state.claimed_names`, `unique_name::ClaimedNames`; the rule and the local twin are in `../DETAILS.md` § "Self-collision (duplicating in place)"). `find_unique_volume_name` claims its pick and walks past anything already claimed, on both branches. Out here that's load-bearing twice over: a directory never reserves at all (bullet above), and three or more top-level sources put the copy on the CONCURRENT driver, so several picks happen at once and the `exists()` probe of a backend with no exclusive create can't see a name whose bytes haven't landed. `photo.jpg` and `photo (1).jpg` duplicated together would otherwise both take `photo (2).jpg`.
- **A pre-known conflict naming the source itself never bulk-skips it.** The pre-flight matches by NAME, so a copy into the folder the sources already live in lists every one of them as conflicting; under `Skip all` `copy_volumes_with_progress`'s bulk-skip prelude would drop them all and the duplicate would silently do nothing. The prelude excludes them (`is_the_same_item`, the resolver's own question), the same exclusion the local engine makes.
- **No `dir_remap` equivalent is needed, and that asymmetry with the local engine is the point.** `merge_level` threads `dest_path` down through its OWN recursion (`child_dest = dest_path.join(&entry.name)`; it's `merge_level` that recurses, not its caller `copy_directory_streaming`, which only sets up the leaf pool), so renaming the top-level destination carries the whole subtree with it for free. The local engine can't: `FileInfo::dest_path()` recomputes `destination.join(relative)` from the original relative path on every call, which is why it needs `dir_remap`'s longest-mapped-ancestor rewrite.

**The move drops the item rather than renaming it**, in `move_same.rs` before the driver runs, so no engine below can represent the case. Only the same-volume path can reach one at all: `move_between_volumes` routes `Arc::ptr_eq` there FIRST, the both-local branch hands off to `move_files_start` (whose own rule lives in `../DETAILS.md`), and a genuine cross-volume move names two different volumes, so no two paths there name one item. Without the drop a folder reaches `rename_merge_directory`, which threads the destination down through recursion and renames every leaf onto itself or shuffles it aside to `name (1)`. Dropped items still emit `write-source-item-done` as `Done` with `source_removed: false`, and they count in `files_total` and `files_processed` so the tally matches what was asked for. Pinned by `self_collision_tests.rs`.

**Dir-vs-dir is NEVER a conflict — it always merges, silently.** `resolve_volume_conflict`'s first check after the self-collision one, and still before any policy lookup or `write-conflict` emit, is "are both sides directories?" — if so it returns the dest path as the merge target with `Replaces::Nothing`, regardless of `conflict_resolution`. A source folder landing on an existing same-named dest folder always merges into it; the configured **file** policy governs every clash _inside_ the merge. So even Stop / Skip / Rename merge the folder itself; only files ever prompt. The FE never sees a dir-vs-dir `write-conflict`. Cross-type clashes (file↔folder) are NOT merges — they keep the full conflict machinery (a red warning worded per direction, explicit Overwrite/Rename).

**Scan-as-you-merge: deep per-file conflicts resolved inline, one dest listing per merged level.** `merge.rs::copy_directory_streaming` discovers deep clashes as it walks, with no upfront recursive pre-scan. The trigger is `create_directory`'s result: `Ok(())` means WE created the level fresh (nothing can clash — skip the dest listing, stream every child straight in); `AlreadyExists` means we're MERGING into the user's pre-existing dir (list the dest level ONCE, index it into a `DestNameIndex`, dispatch each clashing source child through `resolve_volume_conflict`). Dir-vs-dir children recurse unconditionally (no resolver call for the folder); a type mismatch routes through the resolver; a Skip leaves the dest child untouched. A destination LINK is a leaf here whatever it points at, so a folder meeting one is a type mismatch, never a recursion (`../DETAILS.md` § "Symlinks are opaque to a move"). The listing answers every ordinary child in memory; the one `get_metadata` a level can owe is for a name the listing can't settle (see "The deep merge asks the same question" below). Context is threaded via a `MergeCtx` struct (sink, op id, config, `state`, the op-wide apply-to-all latch cell, source hints) so `copy_single_path`'s signature doesn't grow per item. The merge engine is shared by all three pipelines: volume copy (serial `copy.rs` AND concurrent `copy_concurrent.rs`), and cross-volume move (`move_cross.rs::move_volumes_with_progress`). `MergeCtx` is `None` only for the cross-volume move's _staging_ writes and tests that never merge.

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

**A skipped child credits both bars immediately.** `MergeChildDecision::Skip` calls `SourceProgress::skip_leaf(bytes)`
beside `created.record_skip(...)`: `record_skip` is what the move's source sweep spares, and `skip_leaf` is what
moves the progress bars and keeps the skip out of the rate sample. A merge whose children all clash moves no bytes at
all, so without it both bars sit at `0 of N` for the whole run. The netting rule and the throttling behind it:
`../DETAILS.md` § "Skipped work moves the bars, and stays out of the rate".

**MTP can't signal collisions via `create_directory` — the merge walker pre-checks existence there.** Every backend except MTP returns `VolumeError::AlreadyExists` for an existing same-name dir (LocalPosix: `std::fs::create_dir`; SMB: smb2 typed STATUS_OBJECT_NAME_COLLISION; InMemory: explicit check). MTP's `create_folder` happily makes a same-name sibling object (the protocol allows duplicates), which would make the merge target the wrong dir. `Volume::create_directory_errors_on_existing_dir()` (default `true`, `false` for MTP) gates this: on MTP the walker pre-checks `exists()` with the one listing the merge level pays anyway, before creating.

### One window for the whole operation

**Decision**: a directory subtree's file copies run concurrently, through a SINGLE `merge_ctx.rs::FileWindow` that the
whole operation shares — sized by the same `copy.rs::transfer_concurrency` that sizes the top-level driver's window, and
carried on `MergeCtx` so the three production pipelines (`copy_serial.rs`, `copy_concurrent.rs`, `move_cross.rs`) each hand it
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
window got. The top-level source's own row stays alongside them, in a phase the watchdog never acts on.

**Whoever builds a window owes the probe two things**, and both are one line each: `MergeCtx.probe`, so the leaves can
open rows; and the SAME width to `register_operation`, so the dump's `in_flight=<open>/<width>` names the fan-out rather
than the driver's own source loop. The first is a `MergeProbe` (`merge_ctx.rs`), which pairs the operation's probe with
the walker's own `TaskRow`: a walk can't be handed the table without also being told which source to number its leaves
under. Miss the field entirely and every leaf reports into the enclosing SOURCE's row through the outer task-local — the clobbering above, silently, plus a dump that names the folder. Miss the second and the table
declares a width nothing uses. The cross-volume move missed both while running a `transfer_concurrency`-wide walk, and a
user's bundle shows what that costs: a 282-file folder to a NAS dumped `in_flight=1/1` with ten writes open, its one row
the top-level directory carrying a leaf's byte count. Pinned for both drivers side by side in `probe_row_tests.rs`,
which photographs the live table from inside a write.

**A row declares whether it holds a window slot** (`TaskRole`), and the dump's `in_flight=<open>/<width>` counts only
the ones that do. A DIRECTORY source's row is a `Walker`: it lists levels and hands files to the window, and holds no
permit of its own (one held across a recursive descent deadlocks the operation at width 1, § "The N² trap"). Every other
row is a `File` (a merge leaf, or a top-level FILE source). Counted together, a perfectly healthy 10-wide copy of one
folder renders as `in_flight=11/10`, which reads as a limiter that stopped working and costs a reader time mid-incident.
So the walker is reported apart (` walkers=1`, and a ` (walker)` marker on its own row) rather than folded into the
count. `activity.in_flight`, which the UI reads, still counts every row: it answers "how many things are open", not
"how full is the window".

**Both drivers hand a failure BACK, and neither returns it.** `drive_transfer_concurrent` answers a `ConcurrentOutcome` with no `Result` around it, and the serial driver's `async_driver.rs` turns a resolver refusal into `PostLoopIntent::Failed`. Every failure a driver can meet, a task's and conflict resolution's own alike, therefore reaches `copy_volumes_with_progress`'s post-loop, which is the only place that syncs the counters, folds the deep skips, journals the created dirs, reclassifies a Cancelled-shaped error into the cancel path, sweeps the abandoned staged writes, runs the rollback branch, and emits the terminal event. ❌ Never give either driver an `Err` path back to the phase runner: the concurrent one had one (`spawn_ready_tasks().await?`), and a resolver refusal then skipped that entire list, dropped every in-flight task's future mid-write, left the sources that HAD copied out of the rollback ledger, and gave the user no terminal event to explain the half-built destination. Pinned by `copy_concurrent_driver_tests.rs::a_conflict_resolution_failure_comes_back_in_the_outcome`.

**Every driver announces its own phase** through `OperationProbe::set_driver_phase`, and a driver that forgets reports
the initial `starting` for the whole transfer, which tells a dump's reader nothing. What each phase means is on
`DriverPhase`; where the two shapes set them:

- **Concurrent** (`copy_concurrent*.rs`): `PreparingNext` before the destination pre-check, `ResolvingConflict` before
  the resolver, `AwaitingTasks` on the window await, `PostLoop` in `finish`.
- **Serial** (`copy_serial.rs`, `move_cross.rs`): the same `PreparingNext` and `ResolvingConflict` inside the shared driver's
  `dest_meta_fetcher` and `conflict_resolver` closures, then `TransferringSource` for the whole of `transfer_one`, then
  `PostLoop` once `drive_transfer_serial_async` returns. There is no `AwaitingTasks` here: this driver streams the
  source itself, so the phase's job is to tell a reader the wedge can only be in the rows below.

A copy of ONE top-level folder is the commonest transfer there is and always lands on the serial driver, so an
uninstrumented serial path leaves that entire class of stall unreadable: two real stall dumps said only
`driver=starting()` at 113 s and at 33 s. `PreparingNext` and `ResolvingConflict` go in BEFORE their awaits, for the
same reason the concurrent driver does it: the call they name is exactly the one that may never return.

**Every leaf the window holds owns its own share of the reported byte total.** `transfer_driver/progress.rs` keeps one
`LeafProgressLedger` per operation, and each file minted from it holds a `LeafProgress` for its whole life; what the
user sees is `finished + sum(in-flight leaves)`. ❌ Never collapse those shares back into one watermark: a single
shared slot shows whichever leaf is furthest along, and then the next leaf to finish — any leaf, however small — wipes
it and drops the reported total by everything the big one had streamed. That was visible as a Size bar sawtoothing
between ~300 MB and ~80 MB, once per completed file, on a 664 MB folder whose largest member was 259 MB
(`transfer_driver/DETAILS.md` § "Progress stays honest").

### Answering the pre-check from one listing

Before spawning each top-level source, the CONCURRENT driver has to know whether something already sits at that name; if so, conflict resolution runs. Asked as a `dest_volume.get_metadata(dest_item_path)` per source it is one round trip PER FILE, serialized on the driver, and no window width can overlap it — a batch of N files carries a hard floor of `N × RTT`. Measured against David's QNAP at 3.7 ms RTT: **2.378 s of a 3.224 s best run for 500 files, 74%** (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).

Three answers, cheapest first, in `copy.rs`'s spawn loop:

1. **Nothing to ask.** THIS operation created the destination directory (Phase 0.5's `create_directory_all` answered `DirectoryCreation::Created`): nothing the user already had can be inside a folder that didn't exist a moment ago, so there is neither a probe nor an index. Same rule the deep-merge walker one level down has always used (see "Scan-as-you-merge" above).
2. **The listing Phase 0.6 already paid for.** `reap_stale_transfer_temps` does one `list_directory` of `dest_path` on every copy, merges included, immediately before the spawn loop. It now RETURNS that listing (minus the temps it reaped), and the driver indexes it into a `DestNameIndex` (`dest_name_index.rs`) the loop consults in memory. This is the ordinary F5 copy's case: `TransferDialog` seeds the destination with the opposite pane's current folder, which exists, so a merge is what most copies are.
3. **The per-file probe**, for anything the index won't answer.

**The merge walker hands the same fact to the destination.** A level its own `create_directory` made (`Ok`, ❌ not the
`NotSupported` "treat as fresh" fallback, which proved nothing) writes each child with
`LandingName::FreeInFreshFolder` → `WriteStaging::StageInFreshFolder` → on a whole-publishing destination
`WriteMode::CreateNewInFreshFolder`. Every backend treats that mode as `CreateNew` (`WriteMode::refuses_occupied`)
except where its no-overwrite check is a request of its own: S3 then skips the HEAD before each object (the
"No-overwrite writes" section of `crates/cmdr-s3/DETAILS.md` has the accepted window). A staged write lands exactly as `Stage` does. Top-level
files copied straight into a destination folder Phase 0.5 created keep `ExpectedFree` (the concurrent and serial
drivers don't thread the fact yet); the folders a copy or a rename carries are where the requests were.

**A probe that can't ANSWER fails the item, and is never read as "the name is free."** `landing.rs::where_it_lands` is the one rule for all four top-level pre-check sites (`copy_serial.rs`, `move_cross.rs`, and `move_same.rs` through `landing.rs::top_level_precheck`, and the concurrent driver's `copy_concurrent_source.rs::existing_dest_entry`) and for every merge child: only `VolumeError::NotFound` means free, and every other refusal — `ConnectionTimeout`, `DeviceSessionReset`, `PermissionDenied` — becomes a failure of THAT item at the DESTINATION path. Nothing is written for it. A stat that fails is what a flaky share or a phone mid-session-reset looks like, and folding it into "nothing is there" runs no resolver, consults no Skip/Stop policy, and lets the landing clear whatever the probe was asked about — a silent overwrite under a policy that promised the opposite. The driver's `FetchFut` carries the `Result` so the shape is unrepresentable rather than only written down. ❌ No retry here: per-file retry is `retry.rs`'s, inside `stream_pipe_file`, and a second layer would multiply the wait on a dead link. Pinned by `dest_precheck_failure_tests.rs` (one cell per site) and `transfer_driver_async_tests.rs::async_driver_fails_the_item_whose_destination_probe_refuses`.

**Decision (2)**: answer the merge case from the one listing, and accept that it is a snapshot.
**Why**: the round trip is spent either way, so the cost side is zero; the alternative is `N × RTT` of pure serialized latency that no other change can remove.

#### The staleness trade, stated plainly

The listing is taken once, at operation start. By file 400 of a large batch it can be MINUTES old. **A file that arrives at the destination mid-batch is missed: an Overwrite replaces it with no prompt, a Skip doesn't skip it.** That is a real narrowing of the guarantee, not a free win, and it is wider than the created-directory case's window (which needs someone to target a folder Cmdr made seconds ago).

David weighed exactly this and chose it (2026-08-02), with the alternative on the table. ❌ **Do NOT "fix" it with re-listing, polling, a freshness window, or a re-probe before Overwrite.** Each buys back part of the latency this removes, and the simple version is the decision. If the trade ever needs revisiting it's a product call, not a cleanup.

#### Why a name lookup is not a `get_metadata`, and how the gap is closed

The two are NOT equivalent, and every gap is a conflict that becomes a silent overwrite. `DestNameIndex` therefore answers `Absent` only for a name no backend could route onto an entry it holds; a byte-exact hit is `Present`, a look-alike is `LookAlike` (below), and everything else is `Unknown` and falls through to the probe, which stays authoritative. Wrong-way-round costs one round trip; wrong-way-forward costs the user's file.

- **Case.** SMB shares and macOS volumes are typically case-INsensitive, so `get_metadata("foo.txt")` finds a stored `Foo.txt`. Entries are bucketed under a folded key, and a case-only match is `Unknown`, not a hit — whether two names a person can tell apart are one file is the destination filesystem's call, and a case-SENSITIVE destination legitimately holds both.
- **Unicode normalization.** One user-visible name can be two byte strings (NFC and NFD), and they reach a transfer from different places: Cocoa writes NFD, most else NFC, and an SMB server stores and matches whichever bytes it was sent (`SmbVolume::to_smb_path` sends names byte-for-byte, `crates/cmdr-smb/DETAILS.md` § "SMB names are opaque bytes"). The fold is NFC + lowercase, so both spellings share a bucket, and a bucket entry differing ONLY in form is settled in memory as taken: § "Look-alike names and new-name spelling". An ASCII fast path skips the normalizer without changing the answer.
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

#### The deep merge asks the same question

`merge_level` answers each child's "is this name taken?" from the level listing through the SAME `DestNameIndex`, via the same `landing.rs::where_it_lands`, so `Report.docx` arriving where the user keeps `report.docx` — or an NFD name against a stored NFC one — reaches the resolver at depth 5 exactly as it does at the top. `Present` and `LookAlike` carry the listed entry into `resolve_merge_child` (and into the dir-vs-dir decision), at the entry's OWN path; `Absent` is a fresh write under the destination's spelling; `Unknown` costs one `get_metadata` of the child's destination path, which the backend resolves the way it will resolve the write.

- **The probe is rare by construction, not by luck.** A byte-exact hit and a name no stored entry can fold onto are both settled in memory, so an ordinary ASCII tree pays zero probes per level; it fires for a fold-only collision, a `~` name, and a trailing dot or space. The old exact-name map answered `Absent` for a fold-only collision, and a Skip or Stop child then streamed to its temp and LANDED — `staged_write.rs::land` saw the backend's `AlreadyExists`, deleted what was in the way, and renamed over the user's file under a policy that promised not to touch it.
- ❌ **A probe that can't answer fails the item.** `NotFound` means absent; any other error is that child's failure, reported at the child's own path. Reading a transport blip as "nothing is there" hands the child to the fresh-write path, whose landing then clears whatever the probe was asked about — the same discipline this file states for `conflict.rs`'s own probes.
- **The behavior change is consistency, and it's accepted**: a deep child whose name only folds onto a destination name now costs one probe and can raise a prompt it didn't before. That is what the top level already does.
- Pinned by `merge_case_fold_tests.rs` (case, normalization, a case-SENSITIVE destination keeping both spellings with no prompt, an unanswerable probe, a folded directory child merging into the directory that is there, and `an_ordinary_merge_costs_no_probes` for the cost side: a plain ASCII tree asks the destination nothing beyond the driver's own top-level pre-check). The same-volume engine's twin is `rename_merge.rs::late_detected_collision`.

### Listed names are untrusted

A source listing comes from whoever answers it: an SMB, SFTP, WebDAV, or S3 server, an MTP or ADB device, an archive. A hostile one can list `../x` or `/x`, and `dest_dir.join(name)` would then write outside the folder the user dropped onto. So a listed name reaches a destination path ONLY as a `cmdr_fs::volume::ChildName`, proven to be one plain path component (not empty, `.`, or `..`, no `/`, no NUL). A refusal is the typed `VolumeError::InvalidName`, which fails that item the way any unusable name does.

- `landing.rs::where_it_lands` takes a `ChildName`, so the merge walk, the concurrent top-level copy, and the top-level pre-check all validate before joining; the same-volume rename-merge and the native drag-out fulfillment (`apps/desktop/src-tauri/src/native_drag/fulfillment.rs`) do too. One check under every backend, so a new backend can't forget it.
- `InMemoryVolume::set_reported_name` models a source that lists a hostile name. Pinned by `hostile_names_tests.rs` (a bad file name, a folder listed as `..`, and the symlink-then-folder trick: a copy never merges a source folder through a link already at the destination, under every policy) and a drag-out cell in `apps/desktop/src-tauri/src/native_drag/fulfillment_test.rs`.

### Look-alike names and new-name spelling

A byte-exact destination (an SMB share since paths reach it byte-for-byte, SFTP, a phone, `InMemoryVolume`) holds `café` composed and `café` decomposed as two entries and finds each only by its own bytes. So a copy asking about a name in the other spelling would hear "free" and write a second entry nobody can tell from the user's: Skip and Overwrite silently wouldn't apply (`ERR-VETBX`, `crates/cmdr-smb/DETAILS.md` § "SMB names are opaque bytes").

**Decision**: a name the destination holds only under another Unicode FORM is taken by that entry, and every engine addresses it by the entry's stored bytes from then on. **Why**: the end state must be ONE entry. A look-alike is a conflict like any other (Skip, Overwrite, Rename, apply-to-all, and folder merge all behave as for an exact name), and because the resolver is handed the entry's own path, an Overwrite's safe-replace lands on it and a merge walks into it.

- **Form only, never case** (`cmdr_fs::name_fold::differ_only_in_form`). A person can't tell NFC from NFD on screen, so two of them are never a choice anybody made; `Report` and `report` read as two names, and a case-sensitive destination keeps both on purpose (`merge_case_fold_tests.rs` still pins that).
- **One decision point, `landing.rs::where_it_lands`**, asked by every engine for every name: merge levels (`DestFolder::Listed` or `CreatedByUs`), the concurrent driver (`Listed` / `Unlisted` / `CreatedByUs`), and the async driver's top-level pre-check (`top_level_precheck`, `Unlisted`), which answers `transfer_driver::NameAtDest` so the path to use travels with the answer. The same-volume rename-merge indexes its levels with the same `DestNameIndex`.
- **The probe path finds a look-alike with one listing, only when it can matter** (`../../look_alike.rs::look_alike_in`): never for an ASCII name (one form) and never on a volume whose own lookups match any form (`Volume::matches_names_in_any_unicode_form`: `LocalPosixVolume` on macOS, since APFS resolves both). So an ordinary copy pays nothing, and an accented one onto a share pays one listing of the parent per miss.
- **Ambiguity is refused, never guessed.** Two look-alikes and a source spelled like neither is `VolumeError::AmbiguousName`, which a transfer maps to `WriteOperationError::DestinationExists` (the truth, with no Retry that would ask again).
- **A free name takes the destination's spelling; an existing one never does.** `NewName::Respell` asks `Volume::spell_new_name` for a copy's new entries (SMB, SFTP, and WebDAV compose, so a new name lands NFC; `../../DETAILS.md` § "Look-alike names" has why); `NewName::Keep` is the same-volume move, which keeps the entry it moves. A Rename's ` (N)` pick is new too, so `naming.rs` respells it and skips a numbered name the folder holds in another spelling.
- The pre-flight's dialog radios hear about a look-alike through `cmdr_fs::volume::scan_walk::conflicts_against`, named in the destination's own spelling.
- **The Stop prompt says it's a look-alike** (`WriteConflictEvent::destination_is_look_alike`), because the two names print identically and the dialog would otherwise read as an ordinary clash; the conflict dialog adds a line saying the server spells them differently and that Overwrite replaces the entry that's there, and MCP's `pendingConflict` carries `destinationIsLookAlike`. It's read off the two leaves (`../../look_alike.rs::is_look_alike_clash`), not threaded from the landing: every engine names a destination after its source, and a look-alike landing hands the resolver the stored entry's path, so the leaves differ in form exactly when the landing found one. All three event builders ask it.
- Pinned by `look_alike_tests.rs` (InMemory: deep Skip, Overwrite in place, the prompt names the stored entry and says it's a look-alike, an exact clash doesn't, Rename, folder merge, serial and concurrent top level, ambiguity refused, new names composed and as given) and, against a live server, the shared `../../backend_suites/network_look_alike_test_support.rs` scenarios (Skip, Overwrite leaves one entry, folder merge, new names composed, an existing decomposed entry kept under its own bytes, same-server move) driven by `../../backend_suites/smb_look_alike_test.rs`, `../../backend_suites/sftp_look_alike_test.rs`, and `../../backend_suites/webdav_look_alike_test.rs`.

**The conflict-dispatch mutex serializes the human across concurrent / nested merges.** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next to `conflict_slot` — same concern: one human, one oneshot slot) guards the whole Stop-mode dispatch inside `resolve_volume_conflict`: acquire → check `is_cancelled` (bail with `Cancelled` if so — load-bearing: a dropped sender on cancel unblocks only the ONE awaiting task, so a task parked on the mutex must not then emit a prompt nobody will answer, a hang) → re-check the latch (a prior "…all" answer collapses this queued prompt) → emit + await → store latch → release. Released on every exit path, NEVER held across the subsequent file write — serialize the human, not the I/O. The concurrent spawn loop's top-level dispatch and every deep merge acquire the SAME lock. Known acceptable residual: a prompt already emitted before another task latched "…all" isn't retroactively resolved — a rare extra prompt, never a data risk. Pinned by `merge_dispatch_mutex_tests.rs` (concurrent-two-deep-clashes, top-level-vs-deep race, cancel-while-queued no-hang).

**The merge invariant.** A merge never deletes or overwrites a dest file the source doesn't shadow — under every file policy, on every backend, including cancel and rollback mid-merge. Pinned by `merge_tests.rs::merge_never_deletes_unshadowed_dest_files_under_every_policy` (the property test) and the SMB integration pin `smb_integration_merge_deep_clash_skip_all_preserves_dest_only_files`.

**The move invariant: no byte is ever lost.** A move is copy-then-delete-source, so every source file must end up readable from the destination (it moved) or the source (it didn't). The hazard is a folder merge: the deep walker can resolve individual children to Skip, and a skipped child never reached the destination, so its source copy is the only one in existence. `move_volumes_with_progress` therefore sweeps a directory source through `source_sweep.rs::sweep_moved_folder`, sparing `CreatedPaths::skipped_source_paths()` — the source path the merge walker records for each skipped child. The sweep deletes a directory only once nothing stays under it, so sparing one leaf keeps its ancestor spine; a child that FAILS to delete stays too, so the parent isn't attempted (an `ENOTEMPTY` on top of the real leaf error tells the user nothing). It matters on ordinary use, not just an explicit Skip: `OverwriteSmaller` / `OverwriteOlder` reduce to Skip **per file**, so "Overwrite all smaller" on a folder move hits this for every non-qualifying child. The **same-volume** move is inherently correct — its rename-merge leaves a skipped child in place and each level's cleanup is an empty-only `Volume::delete`, so a surviving child keeps its directory. Both are pinned by `move_merge_tests.rs::{move_folder_merge_never_loses_a_byte_under_every_policy, same_volume_move_folder_merge_never_loses_a_byte_under_every_policy}`, which drive every file policy (including the Stop-mode answers) and assert each source file is readable from one side or the other. **Don't** sweep a merged source folder unconditionally.

**The conflict dialog never fabricates a destination size.** `resolve_volume_conflict` reports `destination_size` from the caller's hint, else from the `get_metadata` it already does for the mtime annotation, else `None` ("unknown" in the dialog); a folder destination is always `None` (the volume layer never walks a remote tree for a size). A deep-merge child carries no top-level hint, so the old `Some(dest_size_hint.unwrap_or(0))` reported "Existing: 0 bytes" for files that had content. That number is not cosmetic: the dialog's answer feeds `reduce_volume_conditional_resolution`, so a fabricated `0` made every destination look strictly smaller and silently degraded "Overwrite all smaller" into an unconditional overwrite (and, on a file→folder clash, into a recursive delete of the destination folder). `merge.rs::resolve_merge_child` now passes the dest `FileEntry` the walker already listed for the level, so the common case costs no extra round trip. Pinned by `merge_tests.rs::{deep_merge_clash_reports_the_real_destination_size, overwrite_all_smaller_keeps_a_larger_destination_on_the_first_deep_clash}`.

**Crossing types needs consent, and only a person can give it.** `Overwrite`, `Overwrite all smaller`, and `Overwrite all older` are answers about two FILES: "the one at the destination is stale, put the source there". Across types the act is a different thing — replacing a folder with a file throws a tree away, replacing a file with a folder throws the file away. So `resolve_volume_conflict` turns any of the three into `Skip` when `source_is_directory != destination_is_directory` and nobody was asked: `config.conflict_resolution`, and a same-kind apply-to-all carry. A plain `Overwrite` answered on a prompt for that shape replaces, carry included, at both sites (the up-front lookup and the Stop arm's under-the-lock re-check). A skipped item reports as skipped, and a MOVE's source sweep spares it like any other skip. The rule itself is `../../conflict.rs::resolution_for_clash`, shared with the local-FS and in-archive engines so all three answer the same, and the full consent table lives at `../../DETAILS.md`. Pinned by `conflict_cross_type_tests.rs`.

**Overwrite means merge for dirs, replace for files, enforced architecturally, not by trait contract.** `apply_volume_conflict_resolution` stats the dest first; for directories it skips the delete entirely (the recursive copy merges into the existing tree). This is enforced at the call site rather than relying on `Volume::delete`'s "file or empty directory only" contract. A future backend with recursive delete semantics, or a refactor that consolidates `delete` + `delete_recursive`, would otherwise silently flip the UX from merge to wholesale replace and delete files unique to dest. `conflict_tests.rs::dir_overwrite_must_merge_not_replace_even_with_recursive_delete` pins this with a wrapper Volume that violates the trait contract.

**Cross-volume file→file Overwrite is a safe-replace, NOT a delete-then-write.** A cross-volume file Overwrite (Local↔SMB↔MTP↔USB) must never destroy the existing destination before the new bytes are fully written — otherwise a mid-stream failure (network drop, USB yank, cancel) leaves the user with neither the old file nor a complete new one. So `apply_volume_conflict_resolution`'s file→file branch does NOT delete the dest. It returns a `ResolvedConflict { write_path: <temp sibling>, replaces: Replaces::ViaTemp(orig) }`: the streaming writer lands bytes in a `<name>.cmdr-tmp-<uuid>` sibling on the dest volume, and only after the temp is fully written does the caller call `finalize_safe_replace(dest_volume, temp, orig)`, which deletes `orig` (which survived the whole write) then `rename(temp, orig, force=false)`. On a failure BEFORE the finalize the original is untouched and the existing partial-cleanup sweep removes the temp; on a failure INSIDE it the original may be gone, so the temp is rescued to a ` (recovered)` name instead (below).
- **Why explicit delete-then-rename, not `rename(force=true)`:** MTP's `rename(force=true)` does NOT delete an existing destination — it can create a duplicate. SMB(force=true) deletes-then-renames internally and Local replaces atomically, but the finalize must be uniform across all backends, so it always deletes `orig` first then renames into the now-absent slot. There is a tiny window between the delete and the rename where neither name resolves, but the complete new data lives in the temp throughout, so a crash there leaves a recoverable `.cmdr-tmp-*` sibling rather than data loss. If the `delete(orig)` fails, `finalize_safe_replace` returns the error WITHOUT deleting the temp (the new data must survive), and reports no rescue: nothing was lost, since the destination still holds the user's file.
- **Threading:** `resolve_volume_conflict` / `apply_volume_conflict_resolution` return `Option<ResolvedConflict>`. The three streaming write sites (`volume::copy` serial + concurrent, `volume::move_cross`) carry `replaces` through to their `transfer_one` work, track the TEMP as the in-flight partial (so cancel/error cleanup removes the temp, never the original), and after a successful `copy_single_path` call `finalize_safe_replace` and record the ORIGINAL (not the temp) in `copied_paths` / the milestone for rollback bookkeeping. The cross-volume move finalizes BEFORE deleting the source (a move must never delete the source if the dest isn't fully in place). Under `Replaces::Nothing` nothing is finalized. A destination that publishes every write whole takes the Overwrite in place instead (`Replaces::InPlace`, § "Whole-publish destinations").
- **The post-write temp is committed data, NOT a cleanable partial, and it does not STAY a temp.** `finalize_safe_replace` deletes the original first, then renames the temp in. If the rename fails after the delete succeeded (a disconnect at that instant), the temp holds the ONLY complete copy of the new data and the original is gone. So the finalize immediately renames it out of temp space, to `<name> (recovered)<.ext>` (continuing the house ` (N)` series if that name is taken), and returns a `FinalizeFailure` naming where the bytes are. ❗ **Getting it out of `.cmdr-tmp-*` space is a data-safety act, not tidiness**: `cleanup.rs::reap_stale_transfer_temps` matches on the `.cmdr-tmp-` marker plus an age and runs at the start of every copy and every volume move into that directory, and `staged_write::land` deregisters its temp before clearing the way, so the in-flight ledger neither protects nor deletes it. Left under its temp name, the user's only copy is deleted by the next transfer into the same folder an hour later. Only `AlreadyExists` earns another candidate: the rename has already failed once, so a dead link would fail identically under eight names and only add eight timeouts. When no rename lands at all, the bytes stay under the temp name and the failure reports THAT path, which is the one shape in which committed data still wears a sweepable name. The partial-cleanup contract ("delete partials on error") must not touch either. Each write site stops treating the temp as a partial the moment `copy_single_path` returns `Ok`, BEFORE finalize runs: the **serial** closure clears `last_dest_cell` to `None` up front; the **concurrent** task returns `cleanup_temp = false` so the result handler skips adding the temp to `last_dest_path` (a stream failure sets `true` and cleans as before); the **cross-volume move** has no dest partial-cleanup at all. Pinned by `finalize_recovery_tests.rs` (the rescue, the reap that can no longer reach it, the taken-name series, and both give-up shapes), `copy_crashsafe_tests.rs::{cross_volume_overwrite_serial_preserves_new_data_on_finalize_failure, cross_volume_overwrite_concurrent_preserves_new_data_on_finalize_failure}`, and `move_failure_tests.rs::cross_volume_move_preserves_new_data_on_finalize_failure`.
- **A cross-type Overwrite renames the destination ASIDE and holds it until the operation ends.** A type swap can't stage the new side the way file→file does: a folder replacing a file appears the moment its walk starts and fills in leaf by leaf over the rest of the operation, and any leaf can fail or be cancelled. Deleting the destination first would turn that ordinary failure into losing both: no old file, no complete folder. So `apply_volume_conflict_resolution` calls `displaced_destination.rs::displace_destination` (a refused rename fails the item: the name is still taken) and hands the aside back on `ResolvedConflict::displaced`. The cross-volume copy and move give it to the operation's `DisplacedLedger` (top-level resolvers and `merge.rs::resolve_merge_child` alike, through `MergeCtx.displaced`), op-wide rather than per source because the concurrent driver can abandon a task's future at its cancel deadline. Each source that finishes marks every aside at or under its landed path as replaced (`landed_under`). The post-loop settles them: success discards all (a folder as a tree, `TreeRemoval::UserChoseOverwriteAcrossTypes`); a rollback restores all AFTER the reversal frees their names; a stop or failure discards the replaced ones and restores the rest AFTER the partial cleanup, keeping one beside a half-built folder under a ` (recovered)` name, which a failure reports as `WriteOperationError::OriginalsKeptAside`. The same-volume move settles its aside on the one replacing rename, below. Reachable only from an Overwrite a person picked on a Stop prompt that named both types; see "A blanket Overwrite never crosses types" below. Same-type dir→dir still merges (no delete). Pinned by `cross_type_aside_tests.rs`, `move_failure_tests.rs::a_cross_type_move_that_fails_halfway_keeps_the_file_it_was_replacing`, and `move_same_overwrite_tests.rs::a_refused_rename_puts_back_the_folder_a_file_was_replacing`.
- **Same-volume Overwrite renames the destination ASIDE, it does not delete it.** A same-volume move replaces by renaming the SOURCE onto the name, so the name has to be free first (`rename(force=false)` can't replace, and MTP's `force = true` doesn't delete an existing dest either). There is no stream here, so no temp dance is needed for the new bytes — but the replacing rename is still a SEPARATE call, and an SMB `STATUS_SHARING_VIOLATION` (the source is open elsewhere), an MTP `MoveObject` refusal, or a session blip fails it. Freeing the name with a delete makes that ordinary failure fatal: the destination gone, the source not moved, neither copy where the user left it. So `displaced_destination.rs::displace_destination` renames it to a `.cmdr-temp-<uuid>` sibling, the source rename runs, and the aside is dropped on success or renamed back on failure. One extra call on every backend. Both sites: `move_same.rs`'s resolver (top level, the entry crossing to the transfer closure through `displaced_dests` the way the overwrite verdict crosses through `overwritten_sources`, with a post-loop sweep for anything whose rename never ran) and `rename_merge.rs::rename_replacing` (deep children, both the file→file Overwrite and the dir-subtree-over-a-dest-file case). A cross-type clash arrives already set aside by the resolver (`ResolvedConflict::displaced`), and both sites answer for that aside the same way. ❗ The `.cmdr-temp-` marker is deliberately the one `cleanup.rs::reap_stale_transfer_temps` does NOT match (it reaps `.cmdr-tmp-` only), so an aside nothing could put back survives for the user. When the put-back refuses too — the same dead link — `../recovered_name.rs::rescue_out_of_temp_space` gives it a ` (recovered)` name and the failure carries that path as a typed `WriteOperationError::OriginalsKeptAside`. The one delete that stays a delete is the `Rename`-policy branch clearing its own `O_EXCL` placeholder: that zero-byte file is ours, not the user's. Pinned by `move_same_overwrite_tests.rs`.
- Pinned by `conflict_tests.rs::{file_overwrite_keeps_original_until_temp_is_written, finalize_safe_replace_swaps_temp_over_original}` and `copy_crashsafe_tests.rs::{cross_volume_overwrite_preserves_dest_on_midstream_failure, cross_volume_overwrite_success_replaces_and_cleans_temp, cross_volume_overwrite_concurrent_replaces_and_cleans_temp}`.

**Cross-volume move source-delete removes a LEDGER, never the tree** (`source_sweep.rs`, the twin of the local `move_op/source_sweep.rs`). The copy walk (`merge.rs::merge_level`) records into `CreatedPaths::recording_sources()` every source folder it lists and every source file it carries, stamped from the listing that found it (`SourceStamp`: size, `modified_at`, and `inode` where the backend has one). The walk lists a level before any of its leaves stream, so a save during the copy counts too. After the copy lands, `sweep_moved_folder` lists each walked folder ONCE more (one round trip per folder, beside the per-delete ones it already pays) and deletes only a ledgered file whose listing still matches its stamp; a folder goes only once nothing stays in it, through the empty-only `Volume::delete`. What stays is the user's only copy of something, and the completion event counts it through `AppearedDuringMove` (`../left_in_source.rs`, shared with the local engine): an entry the walk never saw APPEARED (a whole unknown folder counts once), a ledgered file that no longer matches CHANGED, and a merge Skip is spared without being counted. A top-level FILE source gets the same check with `get_metadata` before the copy and again before its delete (`stamp_file`, `file_is_unchanged`); a stamp that couldn't be taken keeps the file. **Decision/Why an mtime**: both reads ask the same backend about the same file the same way, so a coarse clock truncates both alike, and a field a backend never reports (an MTP device's missing mtime, every non-local inode) compares `None` to `None` and drops out, leaving size. The gaps: a same-size save inside one listing second (a local source's inode still catches a save-by-rename), a backend whose listing is served from a cache that missed the change (MTP invalidates on the device's `ObjectInfoChanged` events, and a device that sends none can show a stale stamp), and the window between the listing and the delete. Without the ledger, a recursive `remove_tree` deleted whatever the folder held when the copy ended, including a file saved over after its copy and a download that landed mid-copy. Pinned by `move_source_drift_tests.rs` (changed, appeared, a top-level file, and a clean move). A link is a leaf here too: any entry that might be a folder or a link asks `Volume::entry_kind` (`kind_of`), and a carried link is deleted as the link, ❌ never walked, so a backend whose listing follows links (the walk copied the target) still leaves the target alone (`link_following_backend_tests.rs`). An into-zip move reads its sources outside the merge walk, so it stamps them up front with `stamp_source` (the same listing-per-folder walk, links never followed) and sweeps with the same `sweep_carried_source`: `../../archive_edit/DETAILS.md`.

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
   of streaming each file's bytes it records the resolved destination (`PlannedWrite { dest_path, replaces }`)
   in the plan, keyed by the file's full source path, and streams nothing.
2. **Data** — it opens `source_volume.open_sequential_extract(source_path)` (the archive's one-pass extractor, decode
   ONCE; mechanism in `crates/cmdr-archive/src/read/DETAILS.md` § "One-pass subtree
   extract") and walks the files in ARCHIVE order. Each file the plan kept is streamed through the destination's
   `write_from_stream` (same safe-overwrite temp+rename, downloads-watcher registration, fsync, and
   `finalize_safe_replace` safe-replace as `stream_pipe_file`), recorded in `created`, and reported through a
   `LeafProgress` of its own. A file the plan SKIPPED (conflict resolution said skip) is drained and dropped.

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

## Server-side copy

Duplicating a file inside one remote volume used to pull every byte down the link and push it straight back up. A
protocol that can copy for itself (SFTP's `copy-data`, S3's `CopyObject`) skips both halves, so `stream_pipe_file`
asks `Volume::copy_on_server(source, from, to, mode, progress)` before it opens a stream (`server_side_copy.rs`'s
`try_server_side_copy`). It's also the byte path of a same-volume move whose renames copy (§ "A same-volume move whose
renames copy").

**Which sources a backend copies from is the BACKEND's decision**, from `source`'s concrete type and identity: the
trait default answers only for the very same instance (through `copy_within`, so SFTP and ADB are unchanged), and S3
copies between any places of one account (`crates/cmdr-s3/DETAILS.md` § "Server-side copy"). ❗ Asking a server to copy
a path that belongs to a DIFFERENT server is not a failure; on one that holds a same-named file it is the wrong file,
copied silently. So the engine never decides it from paths or ids; it hands over both volumes and takes the answer.

**`Ok(None)` means "do it the ordinary way"**, and that is the answer for everything except a clean success, a cancel,
and a taken name:

- `NotSupported`, from a backend with no server-side copy, one whose SERVER lacks the extension, or one that doesn't
  recognize the source (another account, another backend, a bucket-bound provider asked to cross buckets).
- ⚠️ **Any other failure.** The streaming loop below carries the retry policy, the stall watchdog, and the pause
  checkpoints, so a fast path that failed for a real reason fails there too with better handling and a better report.
  The cost is one doomed extra attempt on a genuinely broken destination, which is the cheaper side of the trade.
- ❌ **A cancel is never one of them.** The intent is consulted as well as the error variant, so a backend that labels
  its own stop something other than `Cancelled` can't turn a Cancel click into a second, full-speed attempt.
- ❌ **Nor is `AlreadyExists`**: the streamed write would refuse the same name.

**Staging follows the destination.** A whole-publishing destination (§ "Whole-publish destinations") publishes a
server-side copy whole as well, so `resolve_staging(staging, publishes_writes_whole)` sends it to the final name, as
`CreateNew` when the name was expected free; a temp there would only cost a landing rename, a second full copy.
Everywhere else the destination genuinely holds a byte-incomplete file while the copy runs, so it stages exactly like a
streamed write, ❌ never with the single-shot exemption however small the file is. The quit deadline
(`state.backend_abort`) rides the same `select!` it rides for a streamed write, and cleans up nothing for the same
reason.

**A pause lands at the backend's checkpoints.** The engine's `ServerCopyProgress` answers `checkpoint` with the
operation's own `stop_or_park_async`, and S3 asks it before each part, so a paused multipart copy parks between parts
while the parts already in flight finish. A backend that copies in one call (SFTP's `copy-data`) has no checkpoint, so
its pause lands at the next file, bounded by the walk above: `merge_level` parks per entry (§ "Pause in the volume
walks"), at most the operation's `FileWindow` width of leaves still finishing.

Cells: `strategy_server_side_copy_tests.rs` (who's asked, the fallback, staging, the final-name copy on a whole-publish
destination, a sibling place of one account, cancel, and the checkpoint parking a paused copy) with counting doubles;
`crates/cmdr-sftp/src/volume/copy_test.rs` and `crates/cmdr-s3/src/volume/copy_test.rs` for what real servers do;
`backend_suites/s3_rename_integration_test.rs` for a cross-bucket copy through the engine, on the server or streamed.

## A same-volume move whose renames copy

**Decision**: a same-volume move asks `Volume::rename_work` for each top-level source, and if ANY answers
`CopyThenDelete` (an S3 folder, or an object past the part floor), the whole move runs through the copy-then-delete
engine (`move_cross.rs`'s `move_volumes_with_progress`) with the one volume on both sides (`move_same.rs`). **Why**:
`rename_merge` assumes a rename is one cheap call that carries a subtree; on an object store it's a copy per object, and
only the transfer engine gives that a scan, byte progress, pause, cancel, conflicts, and journaling. A volume that
renames everything in one call answers with no I/O, so nothing changes for local disks, SMB, MTP, SFTP, or WebDAV.

- **Copy everything, then delete.** Per top-level source, the copy lands every file (server-side, § "Server-side
  copy") before the source sweep removes anything, so the worst a crash or a cancel leaves is duplicates, ❌ never
  loss. The sweep removes what the copy's LEDGER carried, as every cross-volume move does (`source_sweep.rs`).
- **Batched deletes.** The sweep collects each folder level's files and hands them to `Volume::delete_files` in one
  call (S3: `DeleteObjects`, 1,000 keys a request, per-key failures reported against their own paths), then deletes
  the folder. ❗ A folder that existed only through what was in it (an object store's prefix with no marker) is gone
  once its last file is, so the folder's own `NotFound` counts as removed.
- **The dest-inside-source guard and the already-in-place filter run first**, the same as for a rename-merge; only
  the remaining sources take the copy route.
- **Not rollbackable while running**, like every cross-volume move (`supports_rollback: false`); what it journals
  (per-leaf rows under the one volume id) is what the operation log offers to undo afterwards.

**Renames that run as moves name their target.** A move's destination is a FOLDER, so a rename (`/a/foo` → `/a/bar`)
carries the new NAME per source on `WriteOperationState::target_names` (`../../target_names.rs`), which the async
driver reads where every volume engine builds a top-level destination path. Both engines honor it: the rename-merge
renames straight to the new name, and the copy engine copies to it. A name is one plain component, refused otherwise,
so a source can't land outside the folder it was given; ❌ the local and cross-volume engines take none
(`move_between_volumes` refuses a map there). Who starts these: `../../DETAILS.md` § "Renames that run as moves".

Cells: `move_by_copy_tests.rs` (routing, batched sweep, a rename to a new name on both kinds of volume, a stop keeping
every source) with `InMemoryVolume::with_renames_by_copy`, which refuses `rename` outright so a move that forgot to
route fails loudly; `backend_suites/s3_rename_integration_test.rs` against both S3 fixtures (a 1,005-object folder
rename paging its deletes, a multipart-copy rename keeping the date, pause and cancel).

## The single-shot exemption

**Decision**: a write the DESTINATION performs as one indivisible operation skips the staging and goes straight to the
file's final name (`WriteStaging::SingleShot`). The destination answers `Volume::write_is_single_shot(size)`;
`transfer::staged_write::resolve_staging` is the only place that upgrades a staged write (`Stage` or
`StageOntoClaimedName`) to it, and never an `AlreadyStaged` one (a caller's safe-replace temp keeps the ORIGINAL alive
until the new bytes are complete, which is strictly stronger). Today SMB is
the only backend that answers `true`; MTP, local FS, archives, and in-memory keep the trait default of `false`.

The answer has a SECOND consumer that is not about staging: the destination-side foreground yield exempts a single-shot
write from its min-progress floor, because such a write holds nothing open on the server while it drains
(`../DETAILS.md` § "Foreground auto-yield"). So `stream_pipe_file` probes once and passes the raw boolean to both,
rather than reading the resolved enum — which under-reports, since it stays `AlreadyStaged` for a caller-staged write
however single-shot it is.

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

❗ **A single-shot write refuses a taken name itself.** Staging buys a second property besides "no partial at a real
name": the landing is a rename that must not replace, so a name another writer took mid-upload is refused
(`staged_write.rs::land`). A single-shot write has no landing, so `SingleShot` carries the staged write's `LandingName`,
and `StagedWrite::write_mode` passes `WriteMode::CreateNew` to `Volume::write_from_stream` for an `ExpectedFree` name.
SMB sends that as smb2's `write_file_compound_exclusive` (CREATE with `FileCreate`), which the server refuses
atomically with `STATUS_OBJECT_NAME_COLLISION` if the name is taken, so the other writer's file survives byte for byte
and the copy reports `AlreadyExists`, same as a refused landing. ❌ Don't send it with `FileOverwriteIf`: under Skip,
that reported success with the other writer's file gone (verified on the fixture's Samba, 2026-09-23). A
`ClaimedByTheCaller` single-shot write passes `CreateOrReplace`, since the caller's `O_EXCL` placeholder sits at the
name. `WriteMode` is a required argument, so every caller states which it means. Pinned on a live share by
`backend_suites/smb_transfer_safety_test.rs::smb_integration_a_single_shot_upload_never_replaces_a_name_taken_mid_upload`,
and per backend by `cmdr_fs::volume::conformance::assert_write_from_stream_create_new_refuses_to_clobber`.

**Backend obligations** taken on with a `true` answer, all in `crates/cmdr-smb/src/volume/streams.rs`:

- `WriteMode::CreateNew` goes out as the exclusive compound write (and the exclusive `FileWriter` on the rare
  streaming fallback), so the refusal is the server's, atomic, and a CREATE failure the cleanup below never touches.
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
exemption is scoped to single-shot writes and nothing wider. Under `CreateNew` that leftover also makes the retry's
exclusive CREATE answer `AlreadyExists`, so the file fails with a clash rather than being written over: we can't prove
the 0-byte file is ours, and that is the same call the landing makes.

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
`cmdr-smb`'s `wire_shape_integration_test.rs::smb_integration_a_single_shot_write_leaves_as_one_compound_frame`, which counts wire
frames to prove the promised write really is one compound frame.

## Whole-publish destinations

**Decision**: a destination answering `Volume::publishes_writes_whole` (an object store: S3's PUT and multipart
completion) is written at the FINAL name, a fresh file and a file→file Overwrite alike. **Why**: the protocol already
gives both properties staging buys (no partial at a real name even after a force-quit, and a replace that keeps the
original readable until the new bytes are complete), while a staged landing there is a `rename`, which on S3 is a
server-side copy plus a delete: twice the requests, a copy that fails outright past 5 GB, and no more atomic than the
write. A failed landing would also have left committed data under a `.cmdr-tmp-*` name the hourly reap matches.

- **A fresh file**: `resolve_staging` takes `write_is_single_shot || publishes_writes_whole`, so a `Stage` becomes
  `SingleShot(ExpectedFree)` and goes out as `WriteMode::CreateNew`. The backend owns that refusal, by the provider's
  conditional header where it's trusted, else by a check just before the write plus a check after it that reports a
  clash it notices (`crates/cmdr-s3/DETAILS.md` § "No-overwrite writes").
- **A file→file Overwrite**: `apply_volume_conflict_resolution` answers `Replaces::InPlace` (write path = the original)
  instead of a temp sibling. `staging_for` maps it straight to `SingleShot(ClaimedByTheCaller)`, so it goes out as
  `CreateOrReplace` with nothing to finalize. ❗ It must never fall back to a staged landing, and ❗ a failed write's
  cleanup must never touch the name: what's there is the user's original (`failed_write_leaves_ours_at` answers `false`
  for `SingleShot`). Every write site still records the overwrite (`Replaces::overwrites`), so the op stays
  not-rollbackable. A same-volume move treats `InPlace` like a safe-replace: the original goes aside and the source is
  renamed onto its name.
- **Only staging reads the OR.** The destination-side yield floor and the stall watchdog keep reading
  `write_is_single_shot`: an upload holds a request open while the source drains, so neither exemption applies.

Pinned by `strategy_single_shot_tests.rs::a_whole_publishing_destination_*`, `conflict_tests.rs::
file_overwrite_on_a_whole_publishing_destination_writes_in_place`, and `copy_tests/conflicts.rs::{
test_overwrite_on_a_whole_publishing_destination_replaces_in_place, test_a_failed_in_place_overwrite_leaves_the_original}`
(the last fails if `InPlace` is ever staged onto a claimed name: the cleanup deletes the original). The double is
`InMemoryVolume::with_whole_publish`.

## What mode a landed file wears

**The rule: the volume REPORTS a mode, this engine APPLIES it, and only on a LOCAL destination.** `landed_mode.rs` is
the whole of it. Every backend that knows a file's permissions answers them on `FileEntry::permissions` — the git
portal off the tree entry's `EntryKind`, the archive backend off the zip external attributes / the tar header / the 7z
unix extension, `LocalPosixVolume` off `st_mode`, ADB off the device's `stat`. Nothing carried that across: the bytes
go through `Volume::write_from_stream`, which creates a plain new file, so a `run.sh` copied out of a branch snapshot
or extracted from a release zip landed `0o644` and the user had to `chmod` it themselves.

**`0` is the "no permission concept" answer, and it is the whole gate.** `FileEntry::permissions` documents it, and
every backend keeps to it: SMB, SFTP, WebDAV, and MTP all report `0`. ❗ MTP used to report a fabricated
`0o755`/`0o644`; PTP has no mode, and once a non-zero reading became a fact the copy engine acts on, a
plausible-looking guess would have widened a landed file under a strict umask on nobody's authority. It is
`NO_PERMISSION_CONCEPT` in `cmdr-mtp`'s `directory_ops.rs` now, named so the next person doesn't re-add one.

**The fold, and why it can't widen anything.** `landed_mode(source_mode, created_mode)` intersects the source's low
nine bits with the mode the destination filesystem just gave the fresh temp, allowing an execute bit wherever the
matching read bit survived (`created | ((created & 0o444) >> 2)`). `created_mode` already has the user's umask baked
in, so the intersection re-applies that umask without ever reading it — no `umask(2)` get-then-set race, no cached
process-wide value. It is what `git checkout` and every unzip do:

- `0o755` source, `022` umask ⇒ `0o755`. Same source, `077` umask ⇒ `0o700`, ❌ never a world-readable `0o755`.
- `0o600` source ⇒ `0o600`: the fold narrows as readily as it widens, which is what makes it a copy of the mode rather
  than an executable-bit patch.
- A Windows-made zip reports `0o666` (rc-zip turns a DOS creator's attributes into that stand-in, a read-only flag
  wearing a mode's clothes). Folded, it lands the `0o644` a plain new file would have had anyway.
- Equal to what was created ⇒ `None`, so the common case spends no `chmod` at all.

**setuid, setgid, and sticky never travel.** A routed volume's mode is metadata out of a repo object or an archive
header, which is untrusted input and not authority enough for one of those bits. The local-FS-to-local-FS copy DOES
keep them (macOS `copyfile` with `COPYFILE_STAT`, `chunked_copy.rs`'s `set_permissions` on the fallback), and that
asymmetry is deliberate: there both sides are the filesystem.

**It happens on the STAGED temp, before the rename.** Same reason the staging exists — the visible file never appears
with one mode and flips to another. A `SingleShot` write has no temp and takes its `chmod` right after the bytes, which
is the only order available and harmless (the file is complete). ❌ It never fails a copy: an unreadable stat, a
destination that ignores `chmod` (FAT, exFAT, some network mounts), a source that vanished — each is one debug line and
the bytes stand.

**Who supplies the mode, and the one round trip it can cost.** `SourceFileFacts` (beside `SourceHint` in
`preflight.rs`) carries what the caller already learned about a source FILE: its size and its mode, both honest
`Option`s where absent means nobody looked. The merge walker lists each level anyway, so every deep file's mode is free
(`SourceFileFacts::from_entry`). A TOP-LEVEL file has only the preflight hint, and `CopyScanResult` counts bytes rather
than stat'ing modes — so `apply_source_mode` asks the source itself, once per top-level file, only when the destination
is local, and only after that file's bytes have already crossed.

❗ **That probe is gated on `Volume::reports_posix_mode()`**, which is why it costs nothing on a backend that has no
modes to give. Without the gate, pulling 10,000 selected files off an SMB share would spend 10,000 stats to be told
`0` each time — the same shape as the 15k-MTP-listing stall the source hints exist to prevent. `true` on
`LocalPosixVolume`, the git portal, the archive backend, and ADB; `false` (the default) everywhere else. ❗ A backend
answering `true` without real bits is worse than one answering nothing: the engine treats a non-zero mode as a fact and
puts it on the user's file. ❌ Don't instead add a mode to `CopyScanResult`: that would put a field through 40-odd
construction sites across every backend to save a stat the copy has already paid a whole file for.

**Three write paths, three hooks, and they must stay in step.** `stream_pipe_file` (every streamed cross-volume file,
copy and cross-volume move alike, since `move_cross.rs` routes through `copy_single_path`), `sequential_extract.rs`'s
data pass (which writes its own files — the mode rides `PlannedWrite::source_mode`, recorded by the plan pass, the only
one that lists the archive), and `try_server_side_copy`, which needs no hook: it is `Volume::copy_on_server`, where the
backend copying a file on its own server owns what it copies. A fourth write path owes the same call.

Pinned by `landed_mode_tests.rs` (the fold, per umask and per source shape), `copy_snapshot_out_tests.rs` and
`copy_extract_out_tests.rs` (end to end out of the two routed volumes, into a real local destination, both alone and
inside a folder — the two shapes take different routes through the engine), `move_tests.rs`
(`a_cross_volume_move_carries_the_executable_bit`), and `strategy_sequential_tests.rs`
(`a_sequential_extract_carries_the_executable_bit`).

## Copies keep the source's date

**The rule: the source REPORTS its file's modification date on the read stream, the destination WRITES it.** This is
the canonical home of the contract; the trait doc, the conformance assertions, and the backend docs point here. Local →
local copies keep file dates on their own path (`chunked_copy.rs`, copyfile/clonefile); their folder dates are below.

- **Source half**: `VolumeReadStream::modified_at` is a REQUIRED trait method, so a new backend can't skip it by
  omission. A stream answers the date its open already learned (the stat or listing it did anyway), ❌ never an extra
  round trip. `None` means "no meaningful date" (fresh `create_file` bytes, a generated ZIP, a git blob, a test double),
  and a wrapper (`CheckpointStream`, `fresh_compress.rs`'s pause wrapper) forwards its inner stream's answer. `ChannelReadStream` takes the
  date through `with_modified_at`.
- **Destination half**: every destination that can store a date writes `stream.modified_at()` inside
  `write_from_stream`, so every engine path (staged, single-shot, whole-publish) gets it with no engine code. A staged
  write sets it on the temp, and the engine's final rename carries it, so the real name never shows a wrong date. An
  in-place writer sets it after the last byte (a later write would bump it again).
- **Best effort**: a date that won't set is a `log::warn!`, and the copy still succeeds; the bytes are the copy.
- **`None` leaves the destination's own date**, ❌ never an invented one. Only mtime is in scope: where a protocol
  forces atime alongside (SFTP `ATTR_ACMODTIME`), atime gets the same value. Sub-second where both ends keep it.
- **Not covered**: birth time and permissions (§ "What mode a landed file wears").

**Folders keep their source's date too, set AFTER their contents land.** Writing a child bumps its folder's date on
every real store, so a folder dated when it's created lists "now" by the end of the copy.

- **Cross-volume** (`folder_dates.rs`): `merge_level` answers whether IT created a level (`DirectoryCreation`), and its
  parent notes each created child folder with the source date the parent's listing carried, as the walk leaves it
  (post-order). `copy_directory_streaming` stamps the list through `Volume::set_modified` only once the subtree's leaves
  have drained with no error, then the top-level folder last. Its date is the one no walk listing carries, so it rides
  in from the preflight scan's stat of the top-level path (`CopyScanResult::top_level_modified_at` → `SourceHint` →
  `SourceFileFacts::modified_at`) at no round trip of its own. A source whose scan carried none (an S3 prefix, MTP's
  single-path scan, a top-level dispatch with no hint) leaves it the destination's. The one-pass sequential extract
  hands the list to its `ExtractPlan` and stamps after the data pass, since the planning pass writes no file.
- **Decision/Why the scan carries the root's date, ❌ not a `get_metadata` at stamp time**: that stat was one more
  request per top-level folder, which `s3_engine_integration_test.rs::the_engine_sends_what_the_estimate_counts`
  caught as a HEAD and a LIST over the cost estimate (an S3 prefix has no date to find, so it was pure cost), and a
  whole parent listing on MTP.
- **Local → local and the cross-FS local move** (`../copy/scanned_dirs.rs::date_created_dirs_like_their_sources`):
  after the file loop and the empty-folder pass, every folder in the transaction's `created_dirs` takes its scanned
  source folder's `lstat` mtime. The move dates its STAGED folders, and Phase 3's rename carries each date along.
- **Only folders the copy CREATED.** A merge into a folder the user already had leaves it to the store: it's theirs,
  and the copy only added to it. A folder the merge created inside it is the copy's own and is dated.
- **Never on a failed or stopped copy**: one failure fails the whole subtree, so nothing in it is dated, not even a
  folder whose own contents all landed. Every stamp also checks the intent first.
- **Best effort, and cheap where it can't happen**: a refusal is a `log::warn!`. `set_modified` defaults to
  `NotSupported` (S3's prefixes, MTP, WebDAV, ADB's sync protocol has no setstat, read-only backends), and the first
  `NotSupported` ends the pass.
- **Decision/Why a defaulted trait method, ❌ not a required one**: every mutation on the trait defaults to
  `NotSupported` and opts in, and over 20 test doubles and wrappers would each need a refusal body. The omission a
  required method would catch is caught instead by `conformance::assert_set_modified_dates_a_folder`, which every
  backend that implements it runs. ❗ A wrapper volume forwards it explicitly (`forward_volume_methods!`'s
  `set_modified`), or it silently takes the default.

**Where each backend stands.**

- **Local** (`local_posix/streams.rs`): reports the open's `stat` date; writes it with `File::set_modified` on the open
  handle after the last byte, before `sync_data`. Dates a folder with `filetime::set_file_mtime`.
- **S3**: reports and writes the date as `x-amz-meta-mtime` (`crates/cmdr-s3/DETAILS.md`).
- **`InMemoryVolume`**: keeps the stream's date (whole seconds), so an engine test copying onto it sees what a real
  destination does. It also moves a folder's date to now when an entry lands in or leaves it, which is what lets an
  engine test tell a folder dated after its contents from one dated before.
- **ADB**: reports the open's `STAT`/`STA2` date; writes it as the push's `DONE` mtime (a u32: whole seconds, clamped
  at 2106), falling back to now only for a dateless source (`crates/cmdr-adb/DETAILS.md`).
- **MTP**: reports the `DateModified` from the `ObjectInfo` the read's open already fetched; writes it as the upload's
  `DateModified`, in UTC with a `Z`. Whole seconds. A zoneless device date reads as the Mac's local time at that
  date (`crates/cmdr-mtp/DETAILS.md` § "Dates on copies").
- **SMB**: reports the server's `LastWriteTime` on both foreground read paths (streamed and one-frame compound); writes
  `LastWriteTime` alone through SET_INFO, on the streaming writer's own handle before it closes, or by path right after
  a one-frame compound write (one more frame). The read date rides on the read's own CREATE response
  (`crates/cmdr-smb/DETAILS.md` § "Dates on copies"). Dates a folder by path (`Tree::set_times`, one frame).
- **Archive (source only)**: reports each entry's date from the parsed index, on random-access reads and the one-pass
  sequential extract alike (whole seconds; zip's DOS time keeps even seconds).
- **SFTP**: reports the mtime from the `fstat` its open already sends; writes it with a path `SETSTAT` on the staging
  temp after the awaited close (so a server buffering until close can't bump it), before the final rename. Whole
  seconds, atime set alongside. Server-side `copy-data` copies keep it too (`crates/cmdr-sftp/DETAILS.md` § "Dates on
  copies"). Dates a folder with the same path `SETSTAT`.
- **WebDAV**: reports the GET's `Last-Modified`; writes it as an `X-OC-Mtime` header on the PUT, which Nextcloud,
  ownCloud, and rclone honor. ❗ Plain Apache `mod_dav` can't store a date at all, so a copy onto it keeps the server's
  own: best effort by design, with the destination half pinned on Nextcloud (`crates/cmdr-webdav/DETAILS.md` §
  "Dates").

**How it's pinned.** Two layers, so a gap shows where it lives:

- **Per backend**, in its own crate: `cmdr_fs::volume::conformance::assert_write_from_stream_keeps_the_source_date`
  (the destination half, with a `tolerance` for a coarse clock) and `assert_read_stream_reports_the_listed_date` (the
  source half, on a file dated a day or more back, so a stream reporting "now" can't pass). Every mutable backend's
  `conformance_test.rs` runs both; a read-only backend, or a server that stores no date (Apache `mod_dav`), runs only
  the second, seeded by its fixture's own means.
- **Through the engine**: `backend_suites/network_dates_test_support.rs`'s
  `a_copy_onto_the_server_keeps_the_source_date` and `a_copy_off_the_server_keeps_the_source_date`, with cells for ADB,
  MTP (`mtp_dates_test.rs`, on the virtual device), SMB (`smb_transfer_semantics_test.rs`), and SFTP; WebDAV runs only the copy-off half, through `a_copy_off_the_server_keeps_the_date_it_lists` on a file its
  fixture dated. Plus `in_memory_dates_test.rs`, which pins the engine's own half (the checkpoint wrapper,
  staging, the final rename) against the double in the unit lane. S3's engine cell is
  `s3_transfer_integration_test.rs::copying_onto_a_bucket_lands_every_byte_and_the_mtime`.
- **Folder dates**: `conformance::assert_set_modified_dates_a_folder` per backend (local, in-memory, SFTP, SMB);
  `copied_folders_onto_the_server_keep_their_dates` and `copied_folders_off_the_server_keep_their_dates` through the
  engine (in-memory, SFTP, SMB cells); `folder_dates_tests.rs` (both drivers, a merge, a failure partway);
  `strategy_sequential_tests.rs::sequential_extract_keeps_the_folders_dates`; and for the local engine
  `../copy/folder_dates_tests.rs` plus `move_op_tests.rs::cross_fs_move_keeps_folder_dates`.

**Why it took a test layer of its own.** Every copy suite checksums both ends, and a destination stamping its own date
passes all of them. On 2026-10-07 a 562-photo copy from a Pixel (ADB) onto a QNAP (SFTP) landed every file dated to
the copy, with both gaps at once: ADB's stream reported no date, and SFTP never set one.

## Pause in the volume walks

Three loops in this directory are a WALK rather than a byte pump, and each parks at its own per-entry boundary by asking `state.stop_or_park_async()` exactly where it already observed cancel (`../../DETAILS.md` § "Pause / resume" owns the primitive and the ordering):

- **`merge.rs`'s `merge_level`**, per entry. None of what a walk does — a listing per level, destination creation, a conflict decision per child — passes through the between-chunks checkpoint the byte path parks at, and file children go to the `FileWindow` rather than blocking the walk. So a merge whose children mostly clash, mostly skip, or land through `copy_within` moves down the whole tree with the byte path barely involved, which is what a paused merge used to do. Pinned by `merge_pause_tests.rs`, which makes every child a `Stop`-mode clash so no bytes stream at all and the prompt count is exact evidence about the walk.
- **`rename_merge.rs`'s `rename_merge_directory`**, per child, and the one that matters most: a child with no destination counterpart rides ONE server-side rename that carries its entire subtree, so a single iteration is unbounded and this boundary is the only place a paused same-volume move-merge can stop. Pinned by `rename_merge_pause_tests.rs`.
- **`sequential_extract.rs`'s member loop**, between members. A decode pass over a solid archive can't be re-entered, so the member boundary is the only place it can stop, and one iteration covers the remainder of the subtree whenever skipped members drain through it. Pinned by `strategy_sequential_tests.rs::sequential_extract_pauses_between_members_and_resumes`.

Each test wires its pause to the operation's own progress (the first rename landing, the first prompt answered, the first member written) rather than a wall clock, then holds a window open to prove the walk stopped advancing, then resumes and asserts the outcome is byte-for-byte what an unpaused run produces.

## Pause and the concurrent copy path

The serial drivers (`drive_transfer_serial_{sync,async}`) ask `stop_or_park_{sync,async}` at each per-source loop top, so local copy/move, the cross-volume *serial* path, and delete all honor pause between files; the cross-volume serial path additionally parks between chunks (see above).

**The concurrent copy path is deliberately NOT gated for mid-batch pause.** `copy_volumes_with_progress`'s `FuturesUnordered` path (several files in flight at once) has no single "between files" boundary to park at, so it does **not** honor mid-batch pause: its per-chunk progress callback (`LeafProgress::on_chunk`) stays **cancel-only** (it breaks on `is_cancelled`, ignores `paused`). A pause on a concurrent-path op takes effect once the in-flight batch drains to the next admission point. (Threading the `CheckpointStream` checkpoint into the concurrent path too is possible — each in-flight file already streams through `stream_pipe_file` — but isn't wired yet; the admission-point framing is the current contract.) Pinned by `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`.

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
calls (`copy.rs` Phase 0.5, `move_cross.rs`, `move_same.rs`), the dest space query, and the dest `is_directory` probes;
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

**A `FinalizeFailure` becomes a pathed one through `PathedVolumeError::at_destination` /
`at_source_or_rescued_dest`, which are inherent to `PathedVolumeError` rather than methods on `FinalizeFailure`.**
`cargo-modules` attributes an `impl` to the module defining the type, so `impl FinalizeFailure` written in
`transfer_error.rs` prints as `recovered_name → volume::transfer_error` and welds the two modules into a cycle that
reads backwards from the code (`scripts/check/checks/DETAILS.md` § "Rust module cycles", trap 4). Hanging them off the
type they PRODUCE leaves one honest edge, `transfer_error → recovered_name`. The call syntax is the wordier
`PathedVolumeError::at_destination(f, path)`; that's the whole cost.

**Where each driver attaches it**:

- Cross-volume move (`move_cross.rs`) and serial copy (`copy_serial.rs`) map with `e.path`, never the loop's
  `source_path`.
- The concurrent driver carries TWO paths on `CopyTaskFailure` (`copy_concurrent_task.rs`) and they are not
  interchangeable: `failed_path` is the DESTINATION entry to drop from `in_flight_partials` and possibly clean, while
  `reported_path` is the SOURCE item the user is told about. Merging them would either clean the wrong path or report
  the dest dir root.
- `pull_path_to_local` deliberately drops back to a bare `VolumeError`: it materializes into a scratch dir that is
  discarded wholesale on failure, so no consumer reads a per-item path.

**The source-delete phase, and what a directory sweep reports**: `remove_tree` (`cleanup.rs`) and the move's
`source_sweep.rs` both keep sweeping after a child fails, so it clears everything it can, and it remembers the FIRST child failure with that
child's own path. When the directory's own `delete` then fails, that remembered child comes out instead of the
directory's `ENOTEMPTY` — the surviving child is the diagnosis and the parent's refusal is only its symptom, named
after the folder the user selected. When the directory DOES go, the sweep returns `Ok`: nothing survived to report, and
promoting a child failure there (a race with another deleter, say) would turn a finished move into a reported failure.
The `remove_tree` caller (discarding a cross-type Overwrite's aside in `displaced_destination.rs`) and the into-zip move's source sweep only log, and they
log `e.path` alongside the root they asked for, so the log names the leaf too. Rollback and partial cleanup don't reach
this walker at all — they delete one node each, so their failure already names the only path they asked about.

**A refused source delete is `SourceNotRemoved`, never a plain failure.** By the time `move_cross.rs` deletes the
source, the copy has LANDED, so the item is in both places and nothing is lost. Reporting the refusal flat (a
Finder-locked original's `EPERM` read "you don't have permission to move files here", cmdr-reports#17) tells the
user nothing happened, and their retry then collides with the copy that did. So the refusal becomes
`WriteOperationError::SourceNotRemoved { path, landed_at, cause }` (built by `transfer_error.rs::source_not_removed`): `path` is the leaf that refused (the sweep's rule
above), `landed_at` the destination root, and `cause` the mapped refusal, whose own advice the dialog keeps. No Retry
(`errorDisplayMetaMap`), for the same collision reason. `transfer_sides.rs::name_the_vanished_drive` names a departed
source drive INSIDE `cause` and never replaces the outcome. Pinned by
`move_failure_tests.rs::cross_volume_move_that_cannot_remove_the_original_says_the_copy_landed`.

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
(delete phase), and `cleanup_tests.rs::remove_tree_reports_the_leaf_that_refused` (the walker
itself). The `UndeletableSource` double (`strategy_test_support.rs`) stages it: `InMemoryVolume` alone can't,
because its `delete` drops a directory entry whether or not the directory still holds children.
