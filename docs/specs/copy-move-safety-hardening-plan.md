# A copy that fails can never take data with it

**Status**: SPECCED, not started. **Owner**: David. **Date**: 2026-08-08.

`7046e9dbb` + `bf6d896b3` closed a data-safety hole that lived for three months in the cross-volume copy. This plan
generalizes the three lessons it taught into code, types, and checks, so the next instance of the same shape is caught
by the compiler or a check rather than by a user losing a folder.

**Start with Phase 0.** A claim-by-claim review of this plan found a live data-loss path on MTP: `MtpVolume::delete`
recurses into non-empty directories, which the `Volume::delete` trait contract forbids in bold, and which the
same-volume move's "a child the user chose to Skip keeps its only copy" guarantee rests on entirely. Move a folder
within a phone, merge it onto a same-named folder, choose Skip on one clashing child, and the move's source cleanup
deletes exactly the file the user chose to keep. No probe error, no race. That's not a planning concern, it's a user's
photos. Phase 0 is small, urgent, and independently mergeable, and it goes first.

Read the two commits before starting (`git show 7046e9dbb bf6d896b3`). The short version: a LOCAL scan preview cached
`per_path: Vec::new()`, so `preflight.rs::scan_volume_sources` handed the copy drivers an EMPTY `source_hints` map,
and every driver read it as `source_hints.get(p).copied().unwrap_or_default()`. `SourceHint::default()` claims
`is_directory: false`. So a directory streamed as a file (the visible break), AND the cleanup guard that keeps a failed
directory copy from recursively deleting a MERGED destination folder — a guard keyed on that same flag — went the wrong
way. The cross-volume MOVE was unaffected because it kept an `Option` and probed on `None`; copy had collapsed it to a
bare `bool`.

## The three lessons this effort encodes

1. **A `Default` on a fact-carrying type is a free wrong answer.** `SourceHint::default()` isn't "no information", it's
   a confident claim about the filesystem that nobody made. The type system will refuse to produce it if we stop
   deriving it.
2. **A safety guard keyed on a BELIEF is only as good as the belief.** The cleanup guard was correct code reading a
   corrupt input. Re-key the destructive path on what the code did, not on what it thinks the world looks like.
3. **A test fixture that hand-builds a cross-boundary type reproduces the implementer's assumptions, not production's
   shapes.** Every existing test seeded a fully-populated `per_path` — a shape the LOCAL preview path has never once
   emitted. The fixtures certified the bug.

Product values in play: **protect the user's data** (#4) is the whole point; **elegance above all** (#2) is the reason
several items below get cut or replaced rather than added.

Read before starting: `write_operations/CLAUDE.md` + `DETAILS.md`, `transfer/CLAUDE.md`,
`transfer/volume/CLAUDE.md` + `DETAILS.md` (esp. § "A missing source hint means unknown"), `delete/CLAUDE.md` +
`DETAILS.md`, `scripts/check/checks/CLAUDE.md` + `DETAILS.md`, `docs/testing.md`, `docs/design-principles.md`,
`docs/doc-system.md`. Phase 0 additionally: `crates/cmdr-fs/CLAUDE.md` + `DETAILS.md` and `mtp/CLAUDE.md` +
`DETAILS.md`.

## What I checked, and what it changed about the brief

Findings that re-shape the work. Each is grounded in a grep or a read; don't re-derive them.

- **Compress consumes no preview cache.** `insert_scan_result` has exactly two production call sites, both in
  `scan_preview.rs`. `take_cached_scan_result` has six: local delete (`delete/walker.rs:34`), volume delete
  (`walker.rs:590`), local move (`transfer/move_op.rs:494`), local copy (`transfer/copy/mod.rs:126`), and volume
  copy/move (`transfer/volume/preflight.rs:137` and `:315`). `preview_id` appears nowhere in `archive_edit/` or
  `delete/trash.rs` or `rename.rs`. Compress's `previewId` reaches only `get_scan_preview_totals` for the dialog's
  size estimate. **So the cache-consumer sweep is copy, move, and delete — six pipelines — and nothing else.** Don't
  hunt for a compress or trash or rename consumer; there isn't one.
- **`Volume::delete` is strictly non-recursive by trait contract, and MTP breaks it.** The contract is unambiguous
  (`crates/cmdr-fs/src/volume/mod.rs:337-349`: "**Strict contract: must NOT recurse**", naming rollback and
  partial-file cleanup as callers that "would over-delete if this contract loosened"). LocalPosix, SMB, and
  `InMemoryVolume` honor it. `MtpVolume::delete` (`backends/mtp.rs:461`) forwards to
  `mtp/connection/mutation_ops.rs::delete_object_with_cancel`, which at `:100-146` lists a directory's children and
  recurses per child. So on a phone or a camera the contract is a comment, not a fact, and four guards that lean on
  it lean on nothing. **Phase 0 fixes this first**, and the one that loses data with no probe error and no race is the
  same-volume move's inside-out source cleanup (`rename_merge.rs:186-197`), which is empty-only BY DESIGN and is what
  keeps a Skipped child's only copy alive. Downstream: `conflict.rs:405-422`'s comment asserts a false premise in
  writing, `conflict.rs:447` and `rename_merge.rs:333` reach a bare `delete` on a user directory after a probe error,
  and invariant 10 is aspirational until Phase 0 lands. `walker.rs:749` is NOT in this family — see M1.4 item 3.
- **The real delete hole is different, and nobody listed it.** On a cache hit the LOCAL delete walker iterates
  `scan_result.files` — the paths the PREVIEW walked — and never looks at its own `sources` argument again
  (`walker.rs:34` → `:125`). Nothing anywhere verifies that the `preview_id` the frontend handed back describes the
  selection the operation was asked to act on. That is the exact shape of the bug we just fixed (a fact crossing a
  boundary, believed rather than checked), on the one op that can't be rolled back. **This is the highest-value item
  in the plan and it was not on the list.** See M1.2.
- **Local copy and local move are NOT the same shape, and they differ from each other.** The load-bearing part is
  shared (the file LIST comes from the cache in all three), but both re-read `sources` afterwards, in their own way,
  so a mismatched preview fails differently in each: local copy re-reads it at `transfer/copy/mod.rs:247` (the
  bulk-skip set), so a mismatch silently copies the wrong tree; local move re-reads it at `transfer/move_op.rs:657`
  (`create_scanned_dirs_at_destination`) and `:683` (Phase 3's per-top-level staged rename), and its Phase 4 source
  delete keys off it too, so a mismatch stages the wrong tree and then fails in Phase 3 against a staging directory
  with no entry for the requested source. The VOLUME delete is already bound to its sources (`walker.rs:617` iterates
  `for source in sources` and falls back to a correct fresh scan on `by_path.get(source) == None`). One test per
  pipeline, not one test generalized. See M1.2's test list.
- **The oracle already exists, twice.** `volume/merge_tests.rs::merge_never_deletes_unshadowed_dest_files_under_every_policy`
  and `volume/move_merge_tests.rs::{move_folder_merge_never_loses_a_byte_under_every_policy,
  same_volume_move_folder_merge_never_loses_a_byte_under_every_policy}` already assert "no byte is gone from both
  sides" across the whole policy axis, with a shared fixture builder and a shared assertion helper. Phase 3 EXTENDS
  these; building a third body and leaving those two standing would be worse than not doing it.
- **`InMemoryVolume` already lies a little.** `with_delete_failing`, `set_reported_size`, `set_modified_at` exist in
  `crates/cmdr-fs/src/volume/in_memory.rs`. The test kit in Phase 3 extends this rather than adding a fourth wrapper.
- **`scan_sources_internal`'s `per_path` never crosses the cache.** Its `ScanResult` is returned to the caller
  directly and never inserted into `SCAN_PREVIEW_RESULTS`. See M1.5 for the recommendation.

## Settled decisions

1. **A missing fact is `Option`, never a zero value.** Any type carrying a claim about the filesystem loses its
   `Default`. Absence is resolved by a probe that can fail, or propagated as `Option` to a caller that can decide.
2. **A destructive capability is granted by INTENT, not by proof of provenance.** See M2.1 for why proof-carrying
   newtypes are the wrong shape here.
3. **The cache is bound to its request.** A `preview_id` alone is not authorization to act on a set of paths.
4. **The compiler beats a scanner, where it can see the rule at all.** Where a rule can be expressed as "this type has
   no `Default`", we do that. Where the dangerous shape is hand-written and type-free (`.unwrap_or(false)` on a
   `Result<bool>`), the compiler sees nothing and a scanner is the only mechanism there is. Both, scoped to what each
   can actually cover. See M3.4(d).
5. **A trait contract that no test enforces is a comment.** Every safety promise a `Volume` implementation makes gets
   a shared conformance assertion that all backends run, so a backend can't quietly opt out of it the way MTP did.
6. **Never bump the `file-length` or `claude-md-length` allowlist** (`.claude/rules/file-length-allowlist.md`). M1.1
   exists precisely so a number goes down. Four `CLAUDE.md` files this plan writes to are at or near the 600-word warn
   with no allowlist entry: `write_operations/` (598), `transfer/volume/` (592), `mtp/` (600, exactly at the line), and
   `crates/cmdr-fs/` (427, the only comfortable one). **Every milestone that adds a guardrail line budgets a
   compensating `DETAILS.md` move in the same commit** — condense first, per `docs/doc-system.md`.
7. **Never `git push`.** Every phase lands by fast-forward from its worktree branch when David says so.

### Decisions taken on David's behalf while he was away

He asked to be shown pushback; these are the calls made so execution could start overnight. Each is cheap to reverse.

- **Open question 1 (M1.2's cache binding): KEEP.** It's the same unverified-fact shape as the bug we just fixed,
  sitting on the one operation with no rollback. Cutting it would leave the most dangerous instance of the class we're
  here to close.
- **Open question 2 (`hint-unwrap-or-default`): BUILD IT, in method-scoped form.** This reverses the first call, and
  the reversal is worth reading. The original argument was right that a regex can't see the VALUE TYPE of
  `source_hints.get(p).copied().unwrap_or_default()`, and right that removing `Default` makes that one shape a compile
  error. It was wrong to conclude the compiler is a substitute: the compiler catches nothing at
  `volume.is_directory(p).await.unwrap_or(false)`, which is hand-written, type-free, and is the shape of **every** site
  M1.5(b) fixes. Scoping on the METHOD name instead of the variable name needs no type inference, because the method is
  right there at the call site: `\.is_directory\(.*\)\.await\.unwrap_or\(` finds eight production sites today
  (`rename.rs:112`, `delete/walker.rs:749`, `rename_merge.rs:333`, `conflict.rs:80`, `:82`, `:423`, `move_same.rs:478`,
  `:512`) with a knowable false-positive list. A check with real findings and a stable predicate is a different animal
  from the variable-name one that was rejected. See M3.4(d) for the rebuilt item.
- **Open question 3 (the `PerSource` enum): DEFER**, documented as a contract in M1.6 and left as a named follow-up.
  It has real reach across two crates and deserves its own decision, not a rider on a safety effort.
- **Open question 4 (the derives in `crates/cmdr-fs/src/volume/host/*`): OUT of jurisdiction.** A test stub's zero
  value is a test's problem, and putting them in scope buys annotation churn without buying safety. Note the scoping
  predicate: those six are `#[cfg(any(test, feature = "testing"))]`, not `#[cfg(test)]`. A check matching the literal
  string would demand annotations on all six.
- **Phase 0's shape: option (a) plus a conformance test, not an opt-in recursive trait capability.** Reasoning in the
  Phase 0 section. The short version: making MTP's `delete` refuse a non-empty directory is nearly free (the code
  already has the child listing in hand before it recurses) and no production path loses anything, while a `delete_tree`
  capability would add a second, more dangerous primitive to serve zero callers, and wouldn't have caught this bug
  anyway — MTP never claimed recursion, it just did it.

---

# Phase 0 — `delete` stops at one node, on every backend

Branch `safety-p0-delete-contract`, off `main`. **Do this first, merge it on its own, and don't bundle it.** It's the
only part of this plan that closes a hole users can fall into today. It touches nothing Phases 1-3 touch except one
comment.

## The live bug

`crates/cmdr-fs/src/volume/mod.rs:337-349` states the contract in bold: `delete` handles "a single file or **empty**
directory", "**must NOT recurse**", and names rollback and partial-file cleanup as callers that "would over-delete if
this contract loosened". `MtpVolume::delete` loosened it. What that buys the user on a phone or a camera:

- **The same-volume move's Skip guarantee evaporates, and this is the one that needs no probe error and no race.**
  `rename_merge.rs:186-197` cleans a source directory up inside-out with an EMPTY-ONLY `volume.delete`, and the module
  header (`:22-27`) states the guarantee resting on it: a level still holding a skipped, errored, or unmoved child
  "fails benignly and survives, and so do its ancestors … Never deletes a source dir while content remains."
  `crates/cmdr-fs/src/volume/in_memory.rs:549-557` names that same reliance as the reason the test double honors the
  contract at all. `MtpVolume` implements `rename` (`backends/mtp.rs:493`) and `move_same.rs:529` reaches
  `rename_merge_directory`, so the path is live on a phone: **move a folder within the device, merge it onto a
  same-named folder, choose Skip on one clashing child, and the recursive `delete` destroys exactly the file the user
  chose to keep.** The user made a choice and the app did the opposite of it. **This is Phase 0's reason to exist**,
  and the red test below reproduces it.
- **`rename_merge.rs:333-339`** probes `is_directory`, falls through on a probe error to `exists()` then
  `ctx.volume.delete(&write_path)`. On MTP that takes the user's destination folder; everywhere else it fails
  benignly. **`conflict.rs:447` is the same shape one branch over**: the `else if` arm of a cross-type Overwrite calls
  a bare `dest_volume.delete(dest_path)` when the `dest_is_dir` probe answered `false`, and on MTP that recurses into
  a real directory. Both need a probe error to fire, so they're a step behind the move-Skip path in reachability but
  identical in consequence. (M1.5(b) fixes their inputs; Phase 0 makes the consequence survivable either way.)
- **Rollback's created-dirs prune** leans on the contract in writing too. `cleanup.rs:146-166` deletes each directory
  the operation created with a plain `volume.delete`, and its comment explains why that's safe: a directory still
  holding something "won't be empty, so its `delete` fails … and we leave it standing — exactly the protection that
  keeps rollback from destroying untouched user data." On MTP that delete succeeds. ❌ **Don't write a red test here**;
  it isn't reachable. `created_dirs` can hold a directory the user already had only through a TOCTOU race, and in that
  race MTP has made a duplicate sibling object anyway, so it isn't the user's directory: `DirectoryCreation` never
  feeds `created_dirs` (`copy.rs:511`'s answer is used for exactly one thing, `dest_dir_is_ours` at `:526`), and both
  `record_dir` call sites already pre-check existence on a backend that can't signal a collision
  (`strategy.rs:734-765` records nothing when `backend_create_directory_detects_collisions()` is false; the trait
  default `create_directory_all` gates `Created` on a real `exists() == false`, and no backend overrides it). The
  guard is real and M2.1 hardens it anyway on principle (invariant 13); it is not the live loss.

## Options considered

**(a) Make `MtpVolume::delete` honor the contract.** **(b) Give the trait an opt-in recursive capability**
(`delete_tree` + `supports_recursive_delete()`), so the contract stops being a claim nobody enforces. **(c) Both, or a
combination.**

**Recommendation: (c), read as (a) plus a conformance test every backend runs. Reject (b).**

### Why (a) is nearly free on MTP

"A recursive delete over USB is expensive and `get_metadata` lists the whole parent directory, so a probe per node is a
real cost" is true of the naive implementation and false of the right one.

- The naive implementation asks `is_directory` first. On MTP that's `get_metadata` (`backends/mtp.rs:276-322`), which
  lists the node's whole PARENT to stat one child. Per node. ❌ Don't do that; don't put an `is_directory` anywhere in
  this path.
- The right implementation adds zero USB roundtrips. `delete_object_with_cancel` already resolves the handle, already
  calls `get_object_info` to learn `is_dir`, and on the directory branch already calls `list_objects_with_cancel`
  **before** it recurses. The emptiness answer is in hand before the first recursive call. Refusing becomes
  `if !children.is_empty() { return Err(…) }` where the loop used to be: same traffic, one fewer roundtrip on the
  refusal path.
- **No production path loses recursion.** Every genuine tree delete on a volume already walks caller-side and calls
  `delete` per node, leaf-first: `delete/walker.rs`'s Phase 2 (files first, then dirs deepest-first) and
  `cleanup.rs::delete_preserving_inner` (`:315-360`, which lists each directory itself and deletes children before the
  parent). Both hand `delete` an already-empty directory, so they see no behavior change at all. The single in-repo
  consumer of the recursion is the `delete_mtp_object` IPC command (`commands/mtp.rs:176`), which has a generated
  binding and a `tauri-commands/mtp.ts` wrapper and **zero callers in the Svelte app**. It keeps the recursive entry
  point; nothing else gets one.

### Why not (b)

An opt-in `delete_tree` doesn't fix today's bug on its own — MTP would still have to stop recursing in `delete` — so
it's strictly additive to (a), and what it adds is a second, more dangerous primitive serving zero callers. The callers
that genuinely recurse own a walker that gives them per-child error attribution (`first_child_failure`) and a
`preserve` set; no backend-native tree delete can offer either, so `move.rs:635`'s source sweep could never use one.
And naming the capability wouldn't have caught this: MTP never claimed recursion, it just did it. What catches that
class is a test every backend runs, which is the (c) half.

## The work

**Scope.**

(a) `MtpConnectionManager`'s delete gains an explicit scope: `MtpDeleteScope::{SingleNode, Tree}`, a fieldless enum
with **no `Default` and no `From<bool>`** (same rule as `TreeRemoval` in M2.1). `SingleNode` on a non-empty directory
returns without deleting anything. `MtpVolume::delete` / `delete_with_cancel` pass `SingleNode`; `delete_mtp_object`
passes `Tree` and carries a doc line saying it is the only `Tree` caller in the repo and why. **There are TWO entry
points, not one**: `delete_object` (`mutation_ops.rs:22`) and `delete_object_with_cancel` (`:44`). `delete_mtp_object`
calls the former, `MtpVolume` the latter, so the scope threads through both or the split has a hole in it.

(b) A shared conformance assertion in `cmdr-fs` (`assert_delete_leaves_a_non_empty_dir_intact(volume)`, under
`#[cfg(any(test, feature = "testing"))]`), called from every backend's suite: `local_posix_test.rs`, the SMB suite,
`in_memory_test.rs`, and a virtual-device MTP test. That's what makes decision 5 real rather than a sentence in a doc
comment.

**Landmines.**
- **The refusal must be a typed error**, not a message (`.claude/rules/no-string-matching.md`). `MtpConnectionError`
  has no ENOTEMPTY-shaped variant (`mtp/connection/errors.rs:6-75`), so add one, map it through `map_mtp_error` to
  `VolumeError::IoError`, and assert with `matches!`.
- **That variant crosses IPC.** `MtpConnectionError` is `specta`-exported, so adding a variant regenerates
  `apps/desktop/src/lib/ipc/bindings.ts`, and there's a parallel stub enum at `stubs/mtp.rs:44` for builds without the
  MTP feature. Regenerate the bindings in the same commit and keep the stub's shape consistent with whichever enum
  feeds the committed file. `bindings.ts` sits in the `file-length` `exempt` section, so a stale regeneration surfaces
  as a check failure rather than a compile error, which is the slow way to find out.
- **The path-cache bookkeeping must not drift.** `delete_object_with_cancel` ends by clearing the forward AND reverse
  handle maps and invalidating the parent's listing cache; `mtp/connection/path_cache_sync_test.rs` pins both
  directions (Android reuses handles, so a stale reverse entry resolves to a deleted object's path). That block must
  still run on the `SingleNode` success path, and must NOT run on the refusal path.
- The cancel-between-children checks and the `foreground_guard` nesting comment describe the recursive case. They stay
  with `Tree`; don't leave the comment describing a mode the function no longer has.
- MTP tests need the feature: `pnpm check rust-tests` already runs `--features cmdr/virtual-mtp`
  (`scripts/check/checks/desktop-rust-tests.go:37`), and `mtp/connection/path_cache_sync_test.rs` is the model for
  connecting a virtual device from a plain `#[tokio::test]`.
- ❌ Don't "fix" this by teaching a caller to special-case MTP. The whole point is that `Volume::delete` means one
  thing on every backend.

**Docs owned.**
- `crates/cmdr-fs/src/volume/mod.rs`'s `delete` doc: keep the contract, add that a shared conformance assertion
  enforces it, and fix the stale pointer at `:348` (it sends readers to `delete_volume_path_recursive` in
  `volume/copy.rs`; the function lives in `transfer/volume/cleanup.rs:285` and M2.1 renames it to `remove_tree`).
- `transfer/volume/conflict.rs:405-422`: the comment currently asserts "Today every backend honors that contract
  (delete of a non-empty dir fails benignly)". That is false as written, and it's exactly the kind of confident,
  load-bearing claim that lets a future reader delete a guard. Rewrite it: the contract is enforced by a conformance
  test every backend runs, and the stat-and-skip here is the architectural guarantee regardless of what any backend
  does. (The dir-vs-dir merge itself is safe today; only the comment's premise was wrong.)
- `crates/cmdr-fs/CLAUDE.md`: one guardrail line — a backend's `delete` never recurses, and here's the assertion that
  proves it. 427 words, the one file in this plan with real headroom.
- `mtp/CLAUDE.md` + `DETAILS.md`: the two scopes and who may use `Tree`. **`mtp/CLAUDE.md` is at exactly 600 words**,
  so this milestone moves at least as many words into `DETAILS.md` as it adds. ❌ No allowlist entry.

**Tests — TEST-FIRST, and the red one reproduces the user-visible loss.**
1. **Red, through the real operation, not through the API.** Virtual MTP device: a source folder and a same-named
   destination folder sharing one clashing child; run a same-volume move and answer Skip on that child. Today the
   inside-out source cleanup calls `delete` on the source directory, MTP recurses, and the skipped child's only copy
   is gone. Assert the skipped child still reads back from the source. Watch it go red by the file disappearing, not
   by a compile error. **Write this one first**: a red that reproduces the loss a user would report is worth far more
   than one asserting an API contract, and it's the difference between a phase that's obviously worth doing and a
   phase that looks like tidying.
2. The same defect stated as an API fact, one layer down: a folder holding a file, `MtpVolume::delete` on the folder,
   assert a typed non-empty error AND the file still listed. This is what the conformance assertion generalizes.
3. The conformance assertion, wired into all four backend suites. LocalPosix, SMB, and `InMemoryVolume` pass on day
   one; MTP passes after (a). A backend that fails it is the point.
4. An empty directory still deletes on MTP through `delete` — the regression the refusal could overshoot into.
5. `delete_mtp_object` still removes a tree, pinning the one intentional `Tree` caller so the split stays visible
   rather than accidental.

**Checks.** `pnpm check --fast` while iterating, `pnpm check rust-tests` (carries `--features cmdr/virtual-mtp`), and
**one full `pnpm check` before merging** — this phase changes production behavior on a backend.

**File ownership.** `apps/desktop/src-tauri/src/mtp/connection/mutation_ops.rs`,
`apps/desktop/src-tauri/src/mtp/connection/errors.rs` (the new variant),
`apps/desktop/src-tauri/src/stubs/mtp.rs` (the parallel stub enum),
`apps/desktop/src/lib/ipc/bindings.ts` (regenerated, never hand-edited),
`apps/desktop/src-tauri/src/file_system/volume/backends/mtp.rs`, `apps/desktop/src-tauri/src/commands/mtp.rs`,
`crates/cmdr-fs/src/volume/mod.rs`, the new conformance helper plus each backend's test file, `mtp/CLAUDE.md` +
`DETAILS.md`, `crates/cmdr-fs/CLAUDE.md`, and a COMMENT-ONLY edit to `transfer/volume/conflict.rs:405-422`. That last
file is also M1.5's and M2.1's; Phase 0 merges first and touches no code line in it, so the overlap is a trivial
rebase.

**DONE when.** A Skipped child survives a same-volume merge-move on a virtual MTP device, `MtpVolume::delete` on a
non-empty directory deletes nothing and returns a typed error, every backend's suite runs the shared conformance
assertion, `delete_mtp_object` is the only `Tree`-scoped caller in the repo, and `conflict.rs`'s comment states
something true.

**What Phase 0 does NOT do.** It makes invariant 10 true for the backends that exist. It does not make
`prune_created_dir_if_empty` independent of it — that's M2.1's job, and it's worth doing anyway (see M2.1's second
red test). A guard that needs a contract to be true is one `list_directory` away from a guard that doesn't.

---

# Phase 1 — Close the blast radius

Branch `safety-p1-cache-truth`, off `main` (after Phase 0 lands, so the one shared comment in `conflict.rs` is already
true). Everything here is production behavior. Independently mergeable and valuable on its own.

Milestones run SEQUENTIALLY inside the phase (M1.1 → M1.2 → M1.3 → M1.4 → M1.5). M1.3 and M1.4 are file-disjoint from
each other and could go in parallel once M1.2 has landed, but the signature change in M1.2 touches both of their call
sites, so serializing is cheaper than merging.

## M1.1: `scan.rs` stops being 1,462 lines, and the cache accessors move home

**Scope.** Move `scan.rs`'s inline `#[cfg(test)] mod tests` (lines 1078-1462) to a
`#[path = "scan_tests.rs"] mod tests;` sibling, matching the convention throughout `transfer/volume/`. While there,
move `take_cached_scan_result` (`scan.rs:598`) into `scan_cache.rs`, where both `CachedScanResult` and `ScanResult`
already live and where M1.2 needs to work.

**Intention.** The file-length warn is the symptom; the reason to do this FIRST is that M1.2 has to edit both the cache
type and its one accessor, and having them in two files is what makes the next agent's diff span a 1,400-line module.
Clearing the warn with no allowlist bump is the free win that comes along.

**Landmines.**
- A `#[path]` test sibling changes what `super::` means. Inside `scan_tests.rs`, `super::` is `scan`, so the current
  `use super::*` keeps working, but any `super::super::` in the moved code shifts by one level. `transfer/volume/CLAUDE.md`
  documents this exact trap; re-read it before moving.
- `walk_sources_with_per_path` is `pub(super)`; the moved tests call it. Confirm visibility resolves from the child
  module (it does — a `#[path]` child is still inside `write_operations`).
- Run `pnpm check file-length` and COMMIT the rewritten allowlist. Do not hand-edit the JSON; the check shrink-wraps
  itself. The `scan.rs` entry should drop to ~1,080 or disappear. ❌ Never raise a number.

**Docs owned.** None beyond a one-line file-map touch in `write_operations/DETAILS.md` § Files if it enumerates
`scan.rs`.

**Tests.** No new tests. This is a move; the proof is that the same test names run and pass.
Written after: nothing. TDD does not apply to a file move.

**Checks.** `pnpm check --fast`, then `pnpm check file-length rust-tests`.

**File ownership.** `write_operations/scan.rs`, `write_operations/scan_tests.rs` (new),
`write_operations/scan_cache.rs` (receives one function), `scripts/check/checks/file-length-allowlist.json`.

**DONE when.** `scan.rs` is under 1,100 lines, the allowlist entry shrank or vanished with no manual edit, and
`take_cached_scan_result` lives in `scan_cache.rs`.

## M1.2: the preview cache is bound to its request, and says so when it's incoherent

**Scope.** Two changes at one choke point.

(a) **Bind the entry to its sources.** `CachedScanResult` gains `sources: Vec<PathBuf>` (the paths the preview was
asked to walk). `take_cached_scan_result` becomes
`take_cached_scan_result(preview_id: &str, requested_sources: &[PathBuf]) -> Option<ScanResult>`: on a set mismatch it
`log::warn!`s with both lists and returns `None`. All six call sites already have `sources` in scope; each is a
one-line change, and every one of them already has a "cache miss → fresh scan" fallback (the local ones even log a
warn for it today). Compare as a set, not a slice — order is a frontend detail and `per_path` is already
order-rebuilt.

(b) **The incoherent-state canary.** In `insert_scan_result` (the single insert choke point): when
`file_count > 0 && per_path.is_empty()`, `log::warn!` naming the preview id and both counts, plus a `debug_assert!`.
That is the instrument that would have made the original bug loud in dev on day one.

**Intention.** (a) is the general form of the lesson: a fact that crosses a process boundary is not authorization. The
preview id proves the frontend once asked for a scan; it proves nothing about WHICH scan, and the delete walker acts
on the cached file list without ever re-reading its own `sources`. Making the cache verify itself closes the class for
all six consumers in one place, at the cost of one `Vec<PathBuf>` per live preview (bounded by the TTL sweep already
there). (b) is the cheap tripwire for the sub-case where the entry is right but half-empty.

**Landmines.**
- `debug_assert!` fires in every dev and test build. Confirm no existing test deliberately seeds `files > 0` with an
  empty `per_path`. `scan_preview.rs:680` seeds exactly that (`file_count: 7, per_path: Vec::new()`) — but it inserts
  straight into `SCAN_PREVIEW_RESULTS`, so it bypasses `insert_scan_result` and the canary never sees it. Fix the
  fixture anyway (it should build through M1.3's named constructor), and fix it for the real reason below, not because
  it trips.
- **`insert_scan_result` is not actually a choke point.** `SCAN_PREVIEW_RESULTS` is `pub(super)` (`scan_cache.rs:105`),
  so any module in `write_operations` writes the static directly — three test sites and the `scan_preview.rs` inline
  test do it today. A canary sitting behind a function nobody has to call is decoration. **Make the static private to
  `scan_cache` and give it a `pub(super)` test-only seeding helper that goes through the constructors**, so "every
  entry passed the canary" is a property of the module rather than a habit. That's the change that makes both (a) and
  (b) load-bearing; without it M1.3's `DONE when` is satisfiable while the hole stays open.
- The volume preview legitimately caches `files: Vec::new()` with a populated `per_path`. The canary is one-directional:
  `files > 0 && per_path == 0`, never the reverse. It also fires on a volume batch scan that reports `total_files > 0`
  with an empty `batch.per_path` (`scan_preview.rs:411`) — harmless today, so phrase the assert as "a completed walk",
  not "a local walk".
- Set comparison must normalize nothing. If the frontend can hand a path that differs only by a trailing separator, the
  fix belongs at the IPC edge, not in a lenient comparison here — a lenient comparison is another belief.
- The `#[should_panic]` test below only holds in debug builds, where `debug_assert!` is compiled in. `pnpm check
  rust-tests` is a debug build, so it holds there; say so in the test's comment so nobody chases it under `--release`.

**Docs owned.** `write_operations/DETAILS.md` § "Scan preview caching" (extend with the binding + the canary);
one guardrail line in `write_operations/CLAUDE.md`: *"A `preview_id` alone doesn't authorize acting on a path set —
`take_cached_scan_result` verifies the cached sources match the operation's, and a mismatch is a cache miss."*
**`write_operations/CLAUDE.md` is at 598 of 600 words**, so that line comes with a compensating move into
`DETAILS.md` in the same commit. ❌ No allowlist entry.

**Tests — TEST-FIRST, real red → green.** This is a data-safety change; run the red step and record it. **The three
cache consumers fail differently, so each gets its own test** — one test written against one pipeline and assumed to
generalize is how this bug class survives.
1. `scan_cache_tests`: `take_cached_scan_result` returns `None` and warns when the requested sources differ from the
   cached ones (extra path, missing path, wholly different set); returns `Some` for the same set in a different order.
   *Red first: today the function takes one argument, so this test won't compile — that counts as red only after you
   write the signature and leave the body believing. Add the parameter, ignore it, watch the test FAIL, then use it.*
2. **The flagship red lives on the LOCAL delete walker** (`delete_files_with_progress_inner`, `walker.rs:29-68`), in a
   local delete test file — not on the volume one. A delete asked for `/b` while the cache holds a preview of `/a`
   deletes `/b` and leaves `/a` intact. The local walker takes its file list from `scan_result.files` and never looks
   at `sources` again, so it deletes `/a`: that is the destructive red. **The VOLUME walker is already bound** — it
   iterates `for source in sources` (`walker.rs:617`) and `by_path.get(source)` returns `None` for a foreign preview,
   which falls to a correct fresh recursion at `:670`. A test written there is green on today's code and proves
   nothing. Keep a volume-side test too, but label it a regression fence, not the red.
3. **Local copy**, its own test: `transfer/copy/mod.rs:247` re-reads `sources` for the bulk-skip set while the file
   list comes from the cache, so a mismatched preview copies the wrong tree silently. Assert the destination holds
   what was asked for.
4. **Local move**, its own test: `transfer/move_op.rs:657` and `:683` re-read `sources` for the destination-dir
   creation and the per-top-level staged rename, so a mismatch stages the wrong tree and then fails in Phase 3 against
   a staging directory with no entry for the requested source. Assert both sides survive that failure intact — a
   half-staged move is the worst shape in this plan.
5. `scan_preview` unit test: `insert_scan_result` with `file_count: 3, per_path: []` trips the `debug_assert`
   (`#[should_panic]`, debug builds only) and, in a release-shaped path, still warns.

**Checks.** `pnpm check --fast` while iterating; `pnpm check rust-tests` at the end.

**File ownership.** `write_operations/scan_cache.rs`, `write_operations/scan_preview.rs`,
`write_operations/scan_cache_tests.rs` (new, `#[path]` sibling), ONE line each in `delete/walker.rs`,
`transfer/move_op.rs`, `transfer/copy/mod.rs`, `transfer/volume/preflight.rs`, the four test files that seed
`SCAN_PREVIEW_RESULTS` directly and now go through the seeding helper (`copy_tests.rs`,
`copy_source_hint_tests.rs`, `delete_volume_reuse_tests.rs`, `scan_preview.rs`'s inline test), and the new local
delete / copy / move binding tests. Call it out to the M1.3/M1.4 agents: those single lines are M1.2's, everything
else in those files is theirs.

**DONE when.** No consumer can act on a preview of a different selection, `SCAN_PREVIEW_RESULTS` has no writer outside
`scan_cache.rs`, the canary fires in dev, and all six call sites pass their real `sources`.

## M1.3: two named constructors, so the two production shapes have names

**Scope.** `CachedScanResult::from_local_walk(...)` and `CachedScanResult::from_volume_batch(...)`, replacing the two
struct literals in `scan_preview.rs`. Make the struct's fields `pub(super)`-but-not-constructible-elsewhere if the
module layout allows it; if it doesn't, the check in Phase 3 (M3.4) is the enforcement. Update the five test-side
struct literals to build through them: `copy_tests.rs:1292`, `:1353`, `copy_source_hint_tests.rs:31`,
`delete_volume_reuse_tests.rs:205`, and `scan_preview.rs:682`'s inline test. (There is no `CachedScanResult` literal
anywhere under `commands/` — an earlier draft of this plan listed two, and they don't exist.)

**Intention.** The two shapes are genuinely different (`from_local_walk` carries `files` + `dirs` + `per_path` +
possibly an estimate; `from_volume_batch` carries empty `files`/`dirs`, a populated `per_path`, and never an
estimate). Naming them makes the difference greppable and makes a test that wants "the local shape" ask for it rather
than invent it — which is lesson 3, applied to the exact type that carried the bug. Landing it here rather than in
Phase 2 keeps all `scan_cache.rs` edits inside one phase.

**Landmines.**
- `from_local_walk` should assert its own coherence (a local walk that found files must have per-path entries) —
  that's M1.2's canary, now reachable from a constructor that knows which shape it's building, so it can be a stronger
  claim than the generic insert-time one.
- Keep `inserted_at: Instant::now()` inside the constructors; a caller passing it is one more thing to get wrong.

**Docs owned.** `write_operations/DETAILS.md` § "A completed preview always carries `per_path`" — extend with the two
constructor names.

**Tests.** Written after. The constructors are covered transitively by every existing preview test; add one unit test
per constructor asserting the shape invariant it claims.

**Checks.** `pnpm check --fast`.

**File ownership.** `write_operations/scan_cache.rs`, `write_operations/scan_preview.rs`, plus the four test files
that currently hand-build the struct (`copy_tests.rs`, `copy_source_hint_tests.rs`, `delete_volume_reuse_tests.rs`,
`scan_preview.rs`'s inline test).

**DONE when.** No struct literal of `CachedScanResult` exists outside `scan_cache.rs`.

## M1.4: delete's cache consumption, audited and made honest

**Scope.** Three things in `delete/walker.rs`, in order of how much they matter.

1. **Already fixed by M1.2** — the wrong-selection hole. Confirm the binding lands here and add the delete-specific
   regression test (M1.2's test 2 lives in this milestone's file if the agents prefer; own it in one place, not both).
2. **The `None` arm at `walker.rs:670` is correct as written** — it forwards `is_dir_hint: None`, and
   `scan_volume_recursive` PROPAGATES the probe error (`.map_err(...)?` at `:357-360`) rather than defaulting. Leave
   it. Add a comment saying so, because it looks like the bug and isn't.
3. **The no-preview path at `walker.rs:747-750` IS the asymmetry, and it's loss-class, not annoyance-class.**
   `volume.is_directory(source).await.unwrap_or(false)` guesses "file" on a probe error, while the cached path one
   screen up propagates. Before Phase 0 that guess is destructive on MTP: the resulting `delete` on a directory
   succeeds and takes the tree. After Phase 0 it degrades to a per-item failure with a confusing message. Either way,
   route both through the same propagating resolve. Consequence: a source that can't be stat'd now fails that item
   explicitly. Also fix the oracle-hint lookup above it: it's a listing lookup, so a miss means unknown, and it already
   handles that correctly with `Option` — verify, don't change.

**Intention.** Delete is the op with no rollback, so the bar is "the agent can state, for every branch, what happens
on a wrong or missing fact". The audit is the deliverable as much as the diff. Record in `delete/DETAILS.md` what the
`Volume::delete` non-recursion contract does and doesn't buy this walker: post-Phase-0 a wrong `is_dir` costs a
confusing per-item failure rather than a tree, and that's a property of the conformance test, not of the trait's doc
comment. ❌ Don't restate the contract itself here; it's single-sourced at `crates/cmdr-fs/src/volume/mod.rs`.

**Landmines.**
- `scan_volume_recursive`'s cancel contract: a recursive bail must NOT emit `write-cancelled` itself. Any new early
  return you add needs `emit_cancelled_if_aborted` at the top-level caller, or the FE closes via the settle fallback.
  Pinned by `delete_cancel_during_scan_emits_write_cancelled`.
- The volume delete's cache-hit branch reads a LOCAL preview's `per_path` happily (and correctly, post-`bf6d896b3`).
  Don't add a "reject a local preview" gate; it would regress the win that commit shipped.
- Don't touch `trash.rs`. Trash has no scan phase and no cache.
- **The red step below needs a volume whose `is_directory` ERRORS, and no such knob exists today.** `InMemoryVolume`
  has `with_delete_failing`, `set_reported_size`, and `set_modified_at`, but nothing that fails a stat, and M3.2's
  `set_stat_failing` is two phases away. **Add `set_stat_failing(path)` to `InMemoryVolume` here**, in this milestone
  (roughly 15 lines: a `HashSet<PathBuf>` consulted by `is_directory` and `get_metadata`). It's the fault this whole
  effort is about, M1.5's red step needs the same knob, and M3.2's scope is explicitly "extend, don't add" — it will
  find one knob already there and add the rest. The alternative (hand-rolling a wrapper per test, copying
  `conflict.rs:690-710`) puts the same lie in three places. `in_memory.rs` is therefore touched by M1.4 and later by
  M3.2; the phases are sequential, so that's a rebase, not a conflict.

**Docs owned.** `delete/CLAUDE.md` (one guardrail on the cache binding; 520 of 600 words, the one comfortable file in
Phase 1), `delete/DETAILS.md` (the audit: what each branch does with a missing or wrong fact).

**Tests — TEST-FIRST for the probe-error propagation.**
1. Red: a volume whose `is_directory` errors for one source, no preview. Today the walker classes it a file, tries
   `delete`, and reports the backend's message; assert instead that the operation surfaces the stat failure at that
   path. Watch it fail.
2. After: a cache hit whose `per_path` is EMPTY but whose `file_count` is non-zero (the exact production shape the bug
   rode in on) still deletes the right tree — the `None` arm's probe path, pinned.

**Checks.** `pnpm check --fast`, then `pnpm check rust-tests`.

**File ownership.** `delete/walker.rs`, `delete/CLAUDE.md`, `delete/DETAILS.md`,
`delete/delete_volume_reuse_tests.rs`, `crates/cmdr-fs/src/volume/in_memory.rs` (+ its test file) for the new knob.

## M1.5: `SourceHint` loses its `Default`, and the belief-defaults get decided

**Scope.**

(a) Remove `Default` from `#[derive(Clone, Copy, Default, Debug)]` on `SourceHint` (`preflight.rs:51`). Fix fallout.
Expect it to be near-zero in production (the fix already removed the two `unwrap_or_default()` sites) and to bite only
in tests, which is the point.

(b) Decide each remaining belief-default, one at a time, with the decision written down:
- **`conflict.rs:80`** — `source_volume.is_directory(source_path).await.unwrap_or(false)` when the caller passed no
  hint. **This is not cosmetic.** A wrong `false` with a directory destination makes `is_file_to_folder` true, which
  routes to the cross-type branch at `:435-446` and runs `delete_volume_path_recursive` on the user's destination
  FOLDER. The existing comment says "we'd rather over-prompt than route an unknown clash into the destructive
  file→folder latch" — but `unwrap_or(false)` does exactly the opposite of that stated intent. **Recommendation: make
  it `Result` and fail the conflict resolution**, which surfaces as a per-source failure rather than a destructive
  guess. Verify against Phase 2's split (M2.1) — after that split this call site is the one legitimate recursive
  delete, so it must be the one that's certain.
- **`conflict.rs:82` / `:423`** — `dest_volume.is_directory(dest_path)`. A wrong `false` here means "treat the dest as
  a file", which routes to safe-replace (temp + rename), which fails harmlessly on a directory. Truthful-enough;
  document why and leave it.
- **`rename_merge.rs:333`** — `ctx.volume.is_directory(&write_path).await.unwrap_or(false)`. **This one is
  destructive and was missing from the original list.** A probe error on a DIRECTORY destination falls through to
  `:336`'s `exists()` and then `:339`'s `ctx.volume.delete(&write_path)`. Pre-Phase-0 on MTP that recurses and takes
  the user's destination folder; post-Phase-0 it fails benignly. Same shape as `conflict.rs:80`, same fix: **make it a
  `Result` and fail the item.**
- **`move_same.rs:478`** — same-volume move, `unwrap_or(false)` on the source type. Consequence of a wrong `false`: the
  dir-vs-dir merge branch at `:512` is skipped, so a folder-onto-folder collision goes through
  `resolve_volume_conflict` instead of `rename_merge_directory`. With Overwrite that's a cross-type-looking clash on a
  path where both sides are directories — `conflict.rs:434`'s `same_type_dir` catches it and merges, so it degrades
  rather than destroys. Still a wrong branch on a belief: **route it through `strategy::resolve_source_is_directory`**,
  the same helper the other three pipelines now use.
- **`move_same.rs:512`** — `volume.is_directory(&dest_item_path)`, the guard on the whole rename-merge branch. Same
  wrong-belief consequence as `:478` and it sits one line below it; fix them together or the pair stays inconsistent.
- **`rename.rs:112`** — `v.is_directory(&from).await.unwrap_or(false)`, feeding the journal snapshot only. A wrong
  value mislabels an undo entry and reaches no destructive branch. Truthful-enough: **one-line comment saying so,
  no change.**
- **`delete/walker.rs:749`** — owned by M1.4, listed here for completeness.
- That's the full production set: eight sites, all findable with
  `\.is_directory\(.*\)\.await\.unwrap_or\(` (the same predicate M3.4(d) now automates). Everything else the sweep
  turns up: if a wrong value can't change a destructive branch, write the one-line "why the default is truthful" and
  move on. Don't churn.

**Intention.** (a) is the compiler doing the work a check can't. (b) is the manual pass that has to happen once,
because removing one `Default` doesn't find the `unwrap_or(false)` written by hand. The output is a decision per site,
not a mechanical rewrite.

**Landmines.**
- `resolve_source_is_directory` is `pub(super)` in `strategy`; `move_same.rs` is a sibling, so it resolves. Don't
  widen visibility (`transfer/volume/CLAUDE.md`: the directory is a facade).
- ❌ Don't add a probe where a hint EXISTS. 15k MTP sources = 15k parent listings ≈ two minutes of frozen dialog. Every
  change here fires only on the no-hint branch.
- `conflict.rs` carries an INLINE `mod tests`, so `super::` inside it means something different from the `#[path]`
  siblings. Check which scope you're in.

**Docs owned.** `transfer/volume/CLAUDE.md` (tighten the existing hint guardrail to cover the conflict resolver and
the rename-merge one), `transfer/volume/DETAILS.md` § "A missing source hint means unknown" (add the per-site
decisions). **`transfer/volume/CLAUDE.md` is at 592 of 600 words and M2.1 writes to it too**, so this milestone moves
more than it adds. ❌ No allowlist entry; surface a warn to David rather than silencing one.

**Tests — TEST-FIRST for the `conflict.rs:80` and `rename_merge.rs:333` changes.** Both use M1.4's
`set_stat_failing`.
1. Red: a source volume whose `is_directory` errors, a destination holding a same-named DIRECTORY, policy Overwrite.
   Assert the destination folder and its contents survive and the op reports a failure at that source. This should go
   red on today's code by deleting the folder. **Verify the red is the destructive one**, not a compile error.
2. Red: the same-volume rename-merge path with a stat-failing directory at `write_path`. Pre-Phase-0 the destructive
   red needs a recursive-delete volume to show the tree going; post-Phase-0 the observable red is the wrong branch
   (the delete attempt) rather than lost data. Assert the item fails and the destination directory is untouched.
3. After: `SourceHint` has no `Default` (compile-level; no test needed), and `move_same` with a hintless directory
   source onto a same-named dest directory takes the rename-merge path.

**Checks.** `pnpm check --fast` throughout; **one full `pnpm check` at the end of Phase 1**, which is the phase's
budget.

**File ownership.** `transfer/volume/preflight.rs`, `transfer/volume/conflict.rs`, `transfer/volume/rename_merge.rs`,
`transfer/volume/move_same.rs`, `write_operations/rename.rs` (one comment), `transfer/volume/CLAUDE.md`,
`transfer/volume/DETAILS.md`, plus new/edited `conflict`, `rename_merge`, and `move_same` test siblings.

## M1.6 (decision, not code): should `scan_sources_internal` adopt `walk_sources_with_per_path`?

**Recommendation: NO.** `scan_sources_internal`'s `ScanResult` is returned straight to its caller and is never inserted
into `SCAN_PREVIEW_RESULTS` — `insert_scan_result` has two production call sites, both in `scan_preview.rs`. So its
`per_path: Vec::new()` never crosses the cache boundary and no consumer can read it. Filling it costs a per-source
counter bracket on the local copy/move/delete hot path for a field nobody reads, in service of a symmetry that only
exists on paper.

The uniformity worry underneath the item is real, though, and points somewhere better: `ScanResult` and
`CachedScanResult` are two structs with the same six fields and different truths, and an empty `Vec` is doing the job
of "not collected" — which is the same anti-pattern as `SourceHint::default()`, one level up. **The elegant fix, if we
want one, is to make the field a named enum** (`PerSource::NotCollected` / `PerSource::Collected(Vec<(PathBuf,
CopyScanResult)>)`), so "empty because there were no sources" and "empty because this walk doesn't collect them" stop
being the same value. That is a real change with real reach (six consumers, two crates' worth of `BatchScanResult`
plumbing) and it should be its own decision, not a rider. **Add one line to `scan_sources_internal`'s doc comment
saying its `per_path` is empty BY CONTRACT and why**, and leave the enum as a noted option.

---

# Phase 2 — Make the destructive path unfoolable

Branch `safety-p2-cleanup-intent`, off `main`. One milestone, deliberately. Independently mergeable: it depends on
nothing in Phase 1 and touches no file Phase 1 touches (except `conflict.rs`, at a different line). It's the
complement to Phase 0, not a duplicate of it: Phase 0 makes the "delete stops at one node" contract TRUE, this makes
the cleanup path not need it.

## M2.1: split the recursive delete by INTENT

### Pushback: the proposed newtype guards the wrong thing

The brief asks for a value "mintable only from a `DirectoryCreation::Created` record". I recommend against it, for
three reasons:

1. **It protects the safe case.** A destination directory THIS operation created is exactly the one that's safe to
   sweep — nothing the user had can be inside it. The dangerous case is the merged directory we did NOT create, and a
   `Created`-minted token says nothing about it. Gating on `Created` would let the recursive delete through precisely
   where it's harmless and block it nowhere useful.
2. **`DirectoryCreation` is itself a belief.** Its own doc says an overriding backend "MUST answer that honestly" and
   warns that a dishonest `Created` "would turn 'would have prompted' into 'overwrote'". MTP's `create_folder` can't
   even signal a collision (`create_directory_errors_on_existing_dir() == false`). Keying the recursive delete on it
   re-runs this exact bug one layer up: a guard on a flag a backend supplies.
3. **The provenance we actually want is already recorded, per path, and it isn't a directory.** `CreatedPaths.files` is
   every destination FILE the copy wrote; `CreatedPaths.dirs` is every directory it newly created. `cleanup.rs`'s own
   doc already claims `copied_paths` "are the individual destination FILES the operation wrote (never a merged
   directory root)". The cleanup path doesn't need recursion at all — it uses it as belt-and-braces, and the braces are
   what the bug wore through.

### The design I recommend instead

Split `cleanup.rs`'s one recursive function into three, by capability:

- `delete_written_file(volume, &Path) -> Result<(), PathedVolumeError>` — a plain non-recursive `volume.delete`. The
  ONLY thing `volume_rollback_with_progress`'s `copied_paths` loop and `copy.rs`'s post-loop partial sweep can call.
  Note there are TWO feeds into that loop, not one: `copy.rs:1000`'s post-loop sweep and `copy.rs:940`'s rollback
  branch, which pushes `last_dest_path` into `copied_paths` for `cleanup.rs:88-100` to delete recursively. Same leak,
  same fix, and the second one is easy to miss.
- `prune_created_dir_if_empty(volume, &Path)` — the empty-only, deepest-first sweep rollback does for `created_dirs`.
  **It must check emptiness ITSELF (a `list_directory`), not inherit it from the `Volume::delete` contract.** Today's
  code relies on the contract, the contract was false on MTP until Phase 0, and MTP is exactly the backend where
  `created_dirs` can hold a directory the user already had (`create_folder` can't signal a collision, so
  `DirectoryCreation::Created` there means "we asked", not "it wasn't there"). Phase 0 makes the contract true; this
  makes the guard not need it. The cost is one listing per created directory on the rollback path, which is already
  the slow path and is bounded by what the operation itself created. **A guard that's one `list_directory` away from
  not depending on a promise should not depend on the promise** — that's lesson 2 of this whole effort, applied to
  the plan's own design.
- `remove_tree(volume, &Path, preserve: &HashSet<PathBuf>, why: TreeRemoval)` — the only recursive one.
  `TreeRemoval` is a fieldless enum naming the authorization, with **no `Default` and no `From<bool>`**:
  - `UserChoseOverwriteAcrossTypes` — `conflict.rs:439`, a file→folder Overwrite the user explicitly picked.
  - `MoveSourceAfterDestinationLanded` — `move.rs:635`, with its `preserve` set of skipped children.
  - `ArchiveMoveSourceAfterCommit` — `archive_edit/copy_into.rs:187`, remote originals after the rewrite durably
    commits.

The old `delete_volume_path_recursive` / `_preserving` pair collapses into `remove_tree`; the two names differed only
by an empty `preserve` set, which the enum makes visible at every call site instead.

**Why this is the right shape.** It makes the rollback doc's existing claim into a type: the cleanup path physically
cannot recurse, so no future wrong belief about `is_directory` can reach a recursive delete, because there is no
recursive delete in scope. It's also strictly less code than the proof-carrying alternative, which would have to thread
a token through `CreatedPaths`, both drivers, `CopyTaskSuccess`/`CopyTaskFailure`, and the post-loop. And it names the
three legitimate sweeps, so a reviewer of a fourth one has to answer "authorized by what?" in the type.

**On `delete_volume_path_recursive_preserving` (the MOVE source sweep):** it does NOT need proof. Its authorization is
"the destination landed", which `move.rs` establishes via `flush_created_destinations` before it runs, and its real
safety mechanism is the `preserve` set — which is already correct and pinned by
`move_merge_tests.rs::move_folder_merge_never_loses_a_byte_under_every_policy`. It becomes `remove_tree(...,
MoveSourceAfterDestinationLanded)` and nothing else changes.

**A gap to verify while you're in here.** `copy_serial.rs:400` sets `last_dest_cell = Some(dest_item_path)` for EVERY
source including a directory, and clears it in both the `Ok` arm (`:451`) and the `Err` arm (`:544`). If the serial
closure's future is ever dropped between those two points, a merged directory ROOT survives in the cell and reaches the
post-loop sweep. Today the serial driver awaits that future, so it can't happen — but that's a property of the driver,
not of the cleanup code. After this split it stops mattering at all, because the cell can only feed
`delete_written_file`. **Write the test that would catch it anyway** (see below): it's the cheapest possible proof that
the split did what it claims.

**Landmines.**
- `delete_volume_path_recursive` is re-exported from `transfer/volume/mod.rs` for `archive_edit/copy_into.rs`. The
  facade rule says outside code reaches `volume` only as `transfer::volume::<item>` — so update the re-export, ❌ don't
  widen a submodule.
- The error-naming contract must survive: a directory sweep names the first CHILD that refused, never the parent's own
  `ENOTEMPTY`, and never re-labels with the top-level source. `transfer/volume/CLAUDE.md` § Concurrency and failures.
  `delete_preserving_inner`'s `first_child_failure` logic moves verbatim.
- `copy_tests.rs` has four tests named `delete_volume_path_recursive_*`. Rename them with the function; keep the
  assertions.
- `conflict.rs`'s file→folder Overwrite branch is a genuine recursive delete of user data. Pair the enum variant with
  M1.5(b)'s fix to `conflict.rs:80`, or note the ordering dependency if Phase 1 hasn't landed: the variant makes the
  authorization explicit, and M1.5 makes the input to that authorization certain. They're complementary and neither
  needs the other to compile.

**Docs owned.** `transfer/volume/CLAUDE.md` — replace the "Cleanup and rollback for a DIRECTORY source are per-FILE"
bullet with the stronger structural claim (a REPLACEMENT, not an addition; the file is at 592 of 600 words and M1.5
also writes to it). `transfer/volume/DETAILS.md` — a new § "Three ways to delete, and who may use each", carrying the
rejected `DirectoryCreation::Created` design and why (so nobody proposes it again).

**Tests — TEST-FIRST, this is the phase's whole point.**
1. **Red first, and it must actually be red today.** Reach into `copy_serial`'s post-loop with a fixture where a
   directory source's dest root leaks into `last_dest_path` (simulate the dropped-future window directly if you can't
   provoke it, for example by a unit test on the post-loop sweep function with a directory path in
   `partials_to_clean`) and assert the user's pre-existing dest-only file survives. On today's code the recursive
   delete takes it. After the split, the sweep calls `delete_written_file`, which fails benignly on a non-empty
   directory. Cover the `copy.rs:940` feed as well as `copy.rs:1000`'s.
2. **The second red: a lying volume, for `prune_created_dir_if_empty`.** A thin wrapper whose `delete` recurses
   (`conflict.rs:690-710` already has one to copy) over a rollback whose `created_dirs` holds a directory that in fact
   contains a pre-existing user file. Assert the user's file survives. This is red today AND red after Phase 0 —
   Phase 0 fixes the backends that exist, not a volume that lies — and it goes green only when the prune does its own
   emptiness check. That's the point: it pins the guard against the next backend, not against MTP.
3. Each `TreeRemoval` variant's call site keeps its existing behavior: file→folder Overwrite still clears the dest
   folder (`conflict` tests), the move source sweep still spares skipped children
   (`move_merge_tests.rs` — must stay green untouched), archive move-out still removes remote originals.
4. `remove_tree` still names the leaf that refused (the existing `..._reports_the_leaf_that_refused` test, renamed).

**Checks.** `pnpm check --fast` throughout; **one full `pnpm check`** at the end of the phase.

**File ownership.** `transfer/volume/cleanup.rs`, `transfer/volume/copy.rs`, `transfer/volume/conflict.rs`,
`transfer/volume/move.rs`, `transfer/volume/mod.rs`, `archive_edit/copy_into.rs`, `transfer/volume/copy_tests.rs`
(the four renamed tests), and the two docs above.

**DONE when.** `grep -rn "recursive" transfer/volume/cleanup.rs` shows exactly one recursive function, no cleanup or
rollback call site can reach it, every call that can names its authorization in the type, **and
`prune_created_dir_if_empty` establishes emptiness itself rather than inferring it from a backend's behavior.** That
last clause is what keeps the milestone honest: without it, "no cleanup call site can recurse" is satisfiable while
the recursion just moved behind `Volume::delete`.

---

# Phase 3 — The coverage grid and the checks

Branch `safety-p3-grid-and-checks`, off `main`. Nothing here changes production behavior; it can merge before or after
Phases 1-2 with one stated exception (M3.4 wants Phase 1's `SourceHint` derive already gone, or it annotates one extra
site).

Milestones M3.1 → M3.2 → M3.3 are sequential (each builds on the previous). **M3.4's check work is file-disjoint from
all of them — `scripts/check/**` versus `apps/desktop/**` — and is the one place a second agent can safely run in
parallel.** Its annotation and SMB work is not: it touches `file_system/**` and `crates/cmdr-fs/**` derives, one of
which lives in a file M3.2 absorbs. Give every `#[derive(...)]` line to M3.4 and forbid M3.1-M3.3 from touching one,
and scope the derive check to non-test files so the collision disappears rather than being managed.

## M3.1: one shared safety oracle, extracted from the two that already exist

**Scope.** Pull the assertion helper out of `move_merge_tests.rs` into `transfer/volume/safety_oracle.rs` (a `#[path]`
sibling, `pub(super)`), give `merge_tests.rs` the same helper in place of its inline asserts, and re-point both suites
at it.

**This is NOT a pure extraction, and the plan said it was.** Two corrections an executing agent needs before it
starts:
- `merge_tests.rs` has no assertion helper to extract. Its invariant is inline asserts at `:163-195`. Extracting means
  writing the helper and proving it says the same thing those asserts said.
- The two suites' fixtures are DIFFERENT TREES, not two spellings of one. `merge_tests.rs::make_rich_merge` (`:48`)
  and `move_merge_tests.rs`'s `build_merge_source_tree` / `build_merge_dest_tree` (`:103` / `:123`) differ in clash
  CONTENTS (`b"SRC-clash-larger"` versus `b"SRC-c"`), which is load-bearing for `OverwriteSmaller`, and move's adds
  `/album/swap2`. **Recommendation: extract the assertion helper only, keep the two fixtures.** Unifying them changes
  what the copy suite exercises, and a fixture change that quietly weakens a policy assertion is the worst possible
  outcome for a milestone whose proof is "both suites stay green". If a later pass wants one fixture, it's a
  policy-by-policy review, not a rider here.

**The oracle, stated once.** For a finished operation: (i) **no byte the user didn't approve is gone from either
side** — every source file's content is readable from the source or the destination; (ii) **every byte they did approve
is at the destination**; (iii) **every dest-only file the source didn't shadow is untouched**. `move_merge_tests.rs`'s
`assert_move_merge_preserved_everything` is already (i) + (iii); (ii) is the addition.

**Intention.** One oracle, three op-shaped drivers. The brief asked for ONE parametrized body over all four axes; see
the pushback below for why that's the wrong shape and what replaces it.

**Landmines.** The existing helpers compare CONTENTS, not paths, because Rename relocates files. Keep that. Don't
"simplify" `collect_contents` into a path assertion.

**Tests.** The two existing suites, green, with their fixtures untouched and their assertions now routed through one
helper.
**Checks.** `pnpm check --fast rust-tests`.
**Docs owned.** `transfer/volume/CLAUDE.md`'s module map gets `safety_oracle.rs` — a NAME SWAP inside the existing
file list, not a new line (the file has 8 words of headroom; see decision 6). The oracle's three clauses and why the
two fixtures stay separate go in `transfer/volume/DETAILS.md`.
**File ownership.** `transfer/volume/safety_oracle.rs` (new), `transfer/volume/merge_tests.rs`,
`transfer/volume/move_merge_tests.rs`, `transfer/volume/mod.rs` (module decl), and the two docs above.

## M3.2: a volume that lies, as a first-class fault class

**Scope.** Extend, don't add. Two layers:
- `crates/cmdr-fs/src/volume/in_memory.rs` already has `with_delete_failing`, `set_reported_size`, `set_modified_at`,
  and — after M1.4 — `set_stat_failing(path)`. Add the remaining metadata lie the grid needs:
  `set_reported_type(path, is_directory)` (report a directory as a file and vice versa). If Phase 1 hasn't landed,
  add `set_stat_failing` here too and let M1.4's version fall away in the rebase.
- One shared wrapper in `transfer/volume/faulty_volume_test_support.rs`: `FaultyVolume<V>` that forwards every `Volume`
  method to an inner volume and can fail the Nth call to a named operation. Fold `copy_wedge_test_support.rs`'s and
  `strategy_test_support.rs`'s hand-rolled forwarders into it where they're doing the same job; leave the ones whose
  point is a specific stream behavior (`GatedChunkStream`, `SlowChunkedStream`) alone — those aren't wrappers, they're
  stream doubles.

**Intention.** "Wrong metadata" becomes a fault you can inject the same way you inject an I/O error, because that is
the fault class this whole effort is about and there was no way to express it. The three-wrapper sprawl is the reason
nobody wrote the test that would have caught the original bug.

**Landmines.**
- `InMemoryVolume` buffers a whole file and creates it at the end, which hides mid-write defects
  (`copy_wedge_test_support.rs` says so explicitly). `FaultyVolume` over `InMemoryVolume` inherits that; don't use it
  for anything about partial destination state — those cells use `LocalPosixVolume`.
- A `Volume` impl is large. Prefer a macro or a `Deref`-shaped forward over 40 hand-written methods, and keep the
  overrides explicit.
- `set_reported_type` is a lie the tests need but production must never see. Keep it `#[cfg(any(test, feature = "testing"))]`.

**Tests.** Self-tests for the wrapper: the Nth-call failure fires on the Nth call and not the (N-1)th; a lied-about
type is what `is_directory` returns.
**Checks.** `pnpm check --fast rust-tests`.
**Docs owned.** `crates/cmdr-fs/CLAUDE.md`'s module map gets the new lies in the `in_memory` line (427 words, real
headroom), with the fault-class list in `crates/cmdr-fs/DETAILS.md`. `transfer/volume/CLAUDE.md`'s module map gets
`faulty_volume_test_support.rs` and LOSES the two support files it absorbs — a net word saving, which is why this
milestone is a good place to pay back some of the phase's `CLAUDE.md` budget.
**File ownership.** `crates/cmdr-fs/src/volume/in_memory.rs`, `crates/cmdr-fs/src/volume/in_memory_test.rs`,
`transfer/volume/faulty_volume_test_support.rs` (new), the two existing support files it absorbs from
(`copy_wedge_test_support.rs`, `strategy_test_support.rs`), and the docs above. **`strategy_test_support.rs` also
carries `#[derive(Default)]` lines that M3.4(c) owns** — see the sequencing note there.

## M3.3: the grid

### Pushback: the proposed cell count is roughly 360, and most of it is meaningless

Taken literally, (op × item kind × volume pair × cache state) = 6 ops × 4 item kinds × 5 volume pairs × 3 cache states
= **360 cells**, before the failure axis the brief wants exhaustive on part of it. Three reasons that's the wrong grid:

1. **Three of the six ops have no cache axis.** Verified above: rename, compress, and trash consume no preview cache
   at all. For them the axis is size 1, not 3, which deletes two-thirds of half the grid.
2. **`InMemoryVolume` cannot distinguish local from SMB from MTP.** What actually forks the production code is three
   booleans: `operations_are_local()`, `max_concurrent_ops() > 1` (which picks the serial vs concurrent driver), and
   `Arc::ptr_eq(src, dst)` (same-volume). So the honest volume axis is **{same-volume, cross-volume-serial,
   cross-volume-concurrent}**, size 3. A "local→SMB" cell against in-memory doubles is the same cell as "local→MTP"
   wearing a different name — a silent cap dressed as coverage, which is worse than a stated one. Real backend
   differences are M3.4's job, on the real wire.
3. **A silent cap is worse than a stated one — and 360 unread cells IS a silent cap.** A suite nobody reads and nobody
   can afford to run is coverage on paper.

### The grid I recommend: 3 tiers, ~39 new cells

**Tier A — exhaustive, the blast-radius cell.** `{copy, move, delete}` × item kind × cache state
`{miss, hit-with-per-path, hit-without-per-path}` × outcome `{clean, fail-mid-op, cancel-mid-op}` = **27 cells**.
Every one must exist and pass. This is the exact intersection the bug lived in: a directory, merging into the user's
folder, with a half-populated cache, interrupted.

Two things the grid has to nail down before an agent writes a line of it, because leaving them implicit is how a
"27-cell grid" becomes 18 real cells and nine shrugs:

- **The item kind is per-op, not one label.** For copy and move it's `dir-onto-an-existing-dir (merge)`. **"Merge" has
  no meaning for delete**, which has no destination; delete's nine cells are `dir-with-mixed-contents-and-a-sibling-
  the-op-was-not-asked-to-touch`, and their oracle clause is (i) alone, read as "nothing outside the requested set is
  gone". Nine cells on a different axis wearing the same column heading is a silent cap.
- **Name the pipeline per cell, and mind that one combination is unreachable.** Tier A has no volume axis in the
  original wording, so an agent has to invent one. Use the same three-value axis Tier B uses
  (`{same-volume, cross-volume-serial, cross-volume-concurrent}`), and pin Tier A to the cross-volume-serial pipeline
  unless a cell says otherwise. **For LOCAL copy and move, `hit-without-per-path` cannot reach the driver at all**:
  `copy/mod.rs:126` and `move_op.rs:494` filter on `!c.files.is_empty()`, so a per-path-less local cache is a cache
  MISS by construction. Those cells belong to the volume pipelines; if a local variant is wanted, it asserts the miss
  and the fresh rescan, and says so in its name.

**Tier B — sampled, the shape axis.** `{copy, move}` × item kind `{file, dir-onto-fresh-dest, symlink-to-directory}` ×
driver `{serial, concurrent}` × cache state `hit-without-per-path` only = **12 cells**. Cache state is pinned to the
one shape that used to be a lie, because the other two are already covered by the existing suites.

**Tier C — already covered, not rebuilt.** The full conflict-policy matrix
(`merge_tests.rs`, `move_merge_tests.rs`) stays where it is, now running through the shared oracle from M3.1.

**Explicitly NOT covered, and why** (state this in the test module's doc comment, not just here):
- **rename, compress, trash × cache state** — they consume no cache. Their safety properties are covered by their own
  existing suites.
- **remote↔remote against real backends** — needs two live SMB shares and ~30 s per cell for a class the in-memory
  cross-volume pair forks identically. The three cells worth real wire go in M3.4.
- **MTP** — the virtual device is an E2E-only fixture; the driver differences MTP exercises (`max_concurrent_ops() ==
  1`) are the "cross-volume-serial" column, which IS covered.
- **hardlinks and inode fidelity** — `InMemoryVolume` has no inodes. Covered by the local-FS scan tests.
- **permission-denied and quota faults** — the backend decides these, not the driver; injecting them tests
  `FaultyVolume`, not the code.

**Intention.** The grid's value is the intersections nobody thinks to write by hand, not the count. Tier A is the
intersection the bug lived in. Tier B is the shape axis where a future wrong assumption would land.

**Landmines.**
- The two symlink cells matter: `walk_sources_with_per_path` deliberately reports a symlinked directory as a FILE
  (a transfer copies the link, never dereferences it). Assert that, don't "fix" it.
- Delete's nine Tier A cells run a degenerate oracle (see above). Say so in the driver rather than letting the helper
  quietly assert nothing.
- Table-drive the cells; ❌ don't generate names with a macro that makes a failure un-greppable.
- Watch the per-test duration budget (`docs/testing.md` § Caps are not runtimes). 39 in-memory cells should be well
  under a second each.

**Tests.** This milestone IS the tests. Written after the oracle exists (M3.1) — the TDD discipline for this effort
lives in Phases 1 and 2, where the production changes are; a coverage grid written test-first against unchanged code
would be red for the wrong reason.

**Checks.** `pnpm check rust-tests`, then **the phase's one full `pnpm check`**.
**Docs owned.** `transfer/volume/CLAUDE.md`'s module map gets `safety_grid_tests.rs` (one short line; pay for it from
M3.2's savings). The tier structure, the per-op item kinds, and the "explicitly NOT covered" list live in the test
module's own doc comment AND in `transfer/volume/DETAILS.md` — the module comment is where an agent adding a cell
looks.
**File ownership.** `transfer/volume/safety_grid_tests.rs` (new), `transfer/volume/mod.rs`,
`transfer/volume/CLAUDE.md` + `DETAILS.md`.

## M3.4: four real-SMB cells, and three new checks

Two independent deliverables in one milestone because they share nothing with M3.1 and M3.3 and can run alongside
them. Read the ownership collisions at the end of this section before starting anything in parallel.

### (a) The real SMB cells

Three `smb_integration_*` tests in `apps/desktop/src-tauri/src/file_system/volume/backends/smb_transfer_semantics_test.rs`
(which already holds `smb_integration_same_share_move_merges_with_no_folder_prompt` and friends, and which
`desktop-rust-integration-tests` already brings the SMB stack up for via `scripts/check/smb_orchestrator.go`):

1. **The production bug, on the wire.** A LOCAL directory copied onto a same-named directory that already exists on the
   share, with a cache hit whose `per_path` is empty, forced to fail mid-copy. Assert the share's pre-existing files
   survive and the source is intact.
2. **The same for the cross-volume MOVE.**
3. **An SMB delete consuming a LOCAL preview's cache**, asserting it deletes the requested tree and nothing else.
4. **A cross-type clash where the source type is unknown**, on the wire. A local FILE onto a same-named DIRECTORY on
   the share, policy Overwrite, with the source's `is_directory` failing. That's the `conflict.rs:80` +
   `rename_merge.rs:333` intersection (M1.5(b), B6), and it's the one combination the in-memory grid can't reach
   honestly, because "what a real backend does when asked to delete a non-empty directory" is the whole question.
   Assert the share's directory and contents survive and the op reports a failure at that source.

These four are the cells where the in-memory doubles genuinely can't stand in: a real share publishes bytes at the
write path, has real failure timing, and has a `create_directory` that really does signal collisions.

**Landmine.** Read `apps/desktop/test/smb-servers/README.md` and the orchestrator's lease model before running these
locally; a manual `start.sh` alongside a `pnpm check` run is fine (both take leases), a manual `docker compose down` is
not.

### (b) `desktop-rust-no-hand-rolled-fixture` — build it, it has teeth

Ban struct-literal construction of an allowlisted set of cross-boundary types inside `*_tests.rs` and `#[cfg(test)]`
blocks. Allowlist: `CachedScanResult`, `SourceHint`, `VolumePreflight`. Sibling of `desktop-rust-fixed-temp-dir` and
`desktop-rust-test-sleep`: same `ScannerRoots` + `isRustTestPath` + `advanceTestModRegion` + `directiveTracker` shape,
opt-out `// allowed-hand-rolled-fixture: <why>`.

**It's regex-feasible** — a struct literal NAMES ITS TYPE at the construction site (`CachedScanResult {`), so no type
inference is needed. **What it is NOT is a finder**, and the honest version of that argument matters:

- The real inventory is 5 test-side literals in 4 files, all `CachedScanResult` (`copy_tests.rs:1292`, `:1353`,
  `copy_source_hint_tests.rs:31`, `delete_volume_reuse_tests.rs:205`, `scan_preview.rs:682`). The "2 in
  `commands/file_system/volume_copy.rs`'s tests" an earlier draft claimed don't exist.
- `SourceHint {` and `VolumePreflight {` literals appear only in PRODUCTION `preflight.rs` (`:154`, `:160`, `:250`,
  `:265`, `:325`, `:360`), which `#[cfg(test)]` scoping excludes. Those two allowlist entries match nothing today and
  by design will keep matching nothing.
- M1.3 converts all five `CachedScanResult` sites, so **the check ships with zero findings.**

So this is a **regression fence, not a finder**: it makes M1.3's win permanent instead of a state the next test author
undoes by copy-pasting an old fixture. That's a legitimate reason to build a check, and it's a different reason from
the one the original draft gave. State it that way in the check's own doc comment, so nobody later "cleans up" a check
that never fires.

**Landmines.** Don't flag the named constructors' own bodies (they're in production files, so the `#[cfg(test)]`
scoping already excludes them). Don't flag `Type::from_x(...)`. Handle a multi-line literal (the type name and the
brace are on the same line in every current instance, but assert that in the check's own test).

### (c) `desktop-rust-derive-default-justified` — build it, it's the real enforcement

Any `#[derive(..., Default, ...)]` on a struct or enum under `apps/desktop/src-tauri/src/file_system/**` or
`crates/cmdr-fs/**` must carry a `// DEFAULT-OK: <why the zero value is a truthful claim>` line in the comment block
immediately above (same block-comment handling `desktop-rust-fixed-temp-dir` already implements).

**Current population: 26 derives** in those two trees: 11 under `write_operations/`, 8 in `crates/cmdr-fs/` (the six
host stubs plus `process_memory.rs` and `volume/types.rs`), 4 in `listing/`, 1 in `sync_status/`, 1 in
`volume/backends/mtp.rs`, 1 in `open_with.rs`.

Two scoping details that decide whether this check lands clean:
- The six host stubs in `crates/cmdr-fs/src/volume/host/*` are **`#[cfg(any(test, feature = "testing"))]`, not
  `#[cfg(test)]`** (`events.rs:52-55`, and the same in `activity.rs`, `analytics.rs`, `credentials.rs`, `indexing.rs`,
  `listings.rs`). A check matching the literal `#[cfg(test)]` will demand annotations on all six, which is exactly the
  churn the out-of-jurisdiction decision was meant to avoid. Match both forms.
- **Three of the 26 sit in test-support or test files under `file_system/**`** (`strategy_test_support.rs`,
  `transfer_driver_test_support.rs`, `scan_preview_listing_progress_tests.rs`), and the carve-out as written only
  excuses cmdr-fs host stubs. **Recommendation: extend the same reasoning** — a test double's zero value is a test's
  problem, so scope the check to non-test files in both trees, and say so in the check's doc comment. That leaves 23
  production derives to annotate, which is the honest number.

**This is what makes M1.5's `Default` removal a rule rather than a one-off.** Annotating 20-odd sites is the
milestone's real work, and each annotation is a small honest thought: `ListingProgress`'s zero really is "nothing
enumerated yet"; `SortColumn`'s really is a preference default. If an annotation is hard to write, that derive is the
next bug.

### (d) `desktop-rust-probe-unwrap-justified` — BUILD IT, method-scoped

**This reverses an earlier decision in this plan, and the reversal is the interesting part.** The item started life as
`hint-unwrap-or-default`, scoped on the RECEIVER VARIABLE name, and was rejected for three reasons. Two of them still
stand and one was wrong:

1. **Still true: a regex can't see the value type of a map lookup.**
   `source_hints.get(p).copied().unwrap_or_default()` and `counts.get(k).copied().unwrap_or_default()` are the same
   token stream, so a type denylist has nothing to match on. **Still true: variable-name scoping rots.** A
   hand-maintained list of `source_hints` / `by_path` / `hint` misses the day someone writes `hints_by_source`.
2. **Still true: for THAT shape, the compiler is exact.** Removing `Default` from `SourceHint` (M1.5) makes
   `unwrap_or_default()` on it a compile error naming the precise call site, and
   `desktop-rust-derive-default-justified` keeps that true for the next fact-carrying type.
3. **Wrong: "the compiler already does this" was over-claimed.** It covers the `unwrap_or_default()`-on-a-typed-value
   shape and NOTHING ELSE. It catches zero of the hand-written `volume.is_directory(p).await.unwrap_or(false)` sites,
   which is the shape of **every single site M1.5(b) fixes**, including the two destructive ones
   (`conflict.rs:80`, `rename_merge.rs:333`). A rule with two sub-classes and one mechanism isn't "one mechanism for
   one rule", it's a rule half-enforced.

**Scope on the METHOD name instead.** The method is at the call site, needs no type inference, and doesn't rename
itself: flag `\.is_directory\(.*\)\.await\.unwrap_or\(` under `apps/desktop/src-tauri/src/file_system/**`, opt-out
`// allowed-probe-unwrap: <why the guess is truthful here>`. Today that finds **eight production sites** —
`rename.rs:112`, `delete/walker.rs:749`, `conflict.rs:80`, `:82`, `:423`, `rename_merge.rs:333`, `move_same.rs:478`,
`:512` — of which M1.5 fixes four, and the rest take a directive with the one-line reasoning M1.5 already writes for
them. A real finding set, a stable predicate, and an opt-out that forces the exact thought this whole plan is about.
That's a better check than (b), which fires on nothing.

**Landmines.**
- Six more matches sit in test files and test wrappers (`merge_tests.rs:589`, `:650`, `:907`,
  `strategy_sequential_tests.rs:235`, `copy_tests.rs:1055`, `:1102`, `conflict.rs:703`,
  `strategy_test_support.rs:509`, `smb_transfer_semantics_test.rs:382`). Those are assertions reading a final state,
  not guesses driving a branch. Scope the check to non-test files, same as (c).
- Widening the predicate to `exists()` or `get_metadata()` would double the finding set with mostly-truthful sites.
  Start with `is_directory`, the one whose wrong answer picks a destructive branch, and widen only if a real bug
  shows up elsewhere.
- ❌ Don't make this an error-level check on day one if four sites still carry directives; warn-level with a clean
  slate after M1.5 is the honest ordering.

**Landmines for (b), (c), and (d).**
- Register all three in `registry.go`'s `AllChecks` with the `desktop-rust-` ID prefix the convention uses
  (`desktop-rust-no-hand-rolled-fixture`, `desktop-rust-derive-default-justified`,
  `desktop-rust-probe-unwrap-justified`), nicknames without it.
- **Declare `Inputs`** (`rustInputs`) or `TestEveryCheckDeclaresInputs` fails the suite.
- **Wire all three into `.github/workflows/ci.yml`** or `ci-coverage` fails both ways.
- Each needs its own `_test.go`.
- Take roots from `ScannerRoots(ctx.RootDir, "<check-id>")`, ❌ never a hardcoded source path
  (`workspace-member-coverage` enforces it).
- `IsFast: true` for all three (they're line scanners).
- After authoring: `pnpm check go-vet staticcheck` and update `scripts/check/checks/DETAILS.md` § "Apps and check
  counts".

**Docs owned.** `scripts/check/checks/DETAILS.md` (the three new checks + the count), one line each in
`scripts/check/checks/CLAUDE.md`'s module map if the genre list enumerates them, and `docs/testing.md` § "When you add
X, also add Y" if the fixture rule belongs there.

**Tests.** Go `_test.go` per check (positive, negative, directive-honored, orphan-directive-fails). The four SMB
cells are the tests for (a).

**Checks.** `pnpm check go-vet staticcheck`, `pnpm check ci-coverage workspace-member-coverage`,
`pnpm check desktop-rust-integration-tests` for the SMB cells. **`pnpm check --include-slow` runs once, at the very
end of Phase 3**, and closes the effort.

**File ownership.** `scripts/check/checks/desktop-rust-no-hand-rolled-fixture.go` (+ `_test.go`),
`scripts/check/checks/desktop-rust-derive-default-justified.go` (+ `_test.go`),
`scripts/check/checks/desktop-rust-probe-unwrap-justified.go` (+ `_test.go`), `scripts/check/checks/registry.go`,
`scripts/check/checks/DETAILS.md`, `.github/workflows/ci.yml`, every `#[derive]` line under `file_system/**` and
`crates/cmdr-fs/**` that needs an annotation, and
`apps/desktop/src-tauri/src/file_system/volume/backends/smb_transfer_semantics_test.rs`.

**Ownership collisions to respect** (the "M3.4 is the only safe parallel slot" claim holds against M3.1 and M3.3, not
against M3.2 or Phase 1):
- The `#[derive]` annotation pass covers `preflight.rs:51` (M1.5's), `scan.rs` (M1.1's), and
  `strategy_test_support.rs` + `transfer_driver_test_support.rs` — and **M3.2 absorbs `strategy_test_support.rs`.**
  Scoping (c) to non-test files removes that overlap entirely, which is a second reason to do it.
- `transfer/volume/mod.rs` is claimed by M2.1 (the re-export), M3.1 (a module decl), and M3.3 (a module decl). Three
  one-line additions in different places, but they must be sequential, not parallel.

---

## Sequencing and the execution model

Each phase is a subagent on its own worktree, branched off `main`. Ownership boundaries above are exhaustive: two
agents never write the same file.

- **P0 goes first and merges alone.** It's the only phase closing a hole that's open right now, it's small, and P2's
  design reasoning reads differently once it's true. Don't bundle it with P1 to save a merge.
- **P1 → P2 → P3** is the recommended order after that, but only P3(c)'s annotation pass has a real dependency (it
  wants P1's `SourceHint` derive already removed; if P1 hasn't landed it annotates one extra site and P1's removal
  deletes the annotation with the derive — a one-line conflict at worst).
- **P1 and P2 are file-disjoint** (`scan*.rs` + `delete/` + `preflight/conflict/move_same` versus
  `cleanup/copy/move/mod` + `archive_edit`) with one overlap: `conflict.rs`, which P1.M5 edits at line 80 and P2.M1
  edits at line 439. **Recommendation: run them sequentially anyway.** They're the same subsystem and the second agent
  benefits from reading the first's diff. We're not in a hurry.
- **P3.M4 is the only safe parallel slot** — `scripts/check/**` shares nothing with `apps/desktop/**` except the
  annotation pass, which is `#[derive]` lines nobody else may touch.

**Check budget.** `--fast` freely inside every milestone. **One full `pnpm check` per phase** (at P0, P1.M5, P2.M1,
and P3.M3). **`--include-slow` once, at the very end of P3.** An agent that needs more for confidence should take it
and say why in its report. ❌ Never `git push`.

**Doc budget.** Four `CLAUDE.md` files this plan writes to are at or near the 600-word warn (decision 6). Each
milestone that adds a guardrail line pays for it with a `DETAILS.md` move in the SAME commit. ❌ Never add a
`claude-md-length` allowlist entry; a warn left standing and reported to David is always the safe move.

**Per-milestone ritual** (`AGENTS.md` § Workflow): step back and ask whether it's solid AND elegant AND documented
before calling it done.

## Invariants register

The properties every milestone must leave true. An end-of-phase conformance pass reads this list.

1. A missing `source_hints` entry means UNKNOWN, never "file", and the RESOLVED answer drives the cleanup/ledger
   branch. ❌ No probe where a hint EXISTS.
2. No type carrying a filesystem fact has a `Default`.
3. Cleanup and rollback for a DIRECTORY source are per-FILE, never the dir root — structurally, not by convention,
   and not by trusting a backend to refuse (M2.1's `prune_created_dir_if_empty` checks for itself).
4. A merge never deletes or overwrites a dest file the source doesn't shadow.
5. A MOVE's source sweep spares every child the merge skipped.
6. A `preview_id` doesn't authorize acting on a path set; the cached sources must match the operation's.
7. A completed preview always carries `per_path`, whichever walk produced it.
8. A failure carries the path it happened ON; a directory sweep names the first child that refused.
9. Dir-vs-dir is never a conflict; only files prompt.
10. `Volume::delete` never recurses, **on every backend, proven by a conformance assertion each backend's suite
    runs.** ⚠️ False until Phase 0 lands: `MtpVolume::delete` recurses today. Nothing else in this register may be
    read as depending on it.
11. Every check declares `Inputs` and is wired into CI.
12. The `file-length` and `claude-md-length` allowlists only ever shrink.
13. A safety guard doesn't rest on a promise it could verify itself for the cost of one listing.

## Open questions for David

All were decided provisionally so execution could proceed overnight; see § "Decisions taken on David's behalf". Each
is cheap to reverse.

1. **Phase 0 exists and goes first.** A review found `MtpVolume::delete` recursing in violation of the trait contract
   three guards depend on, which makes it a live data-loss path on phones and cameras, not a planning concern.
   Decided: fix it first, as option (a) plus a conformance test, and reject the opt-in `delete_tree` capability. This
   is the one item worth reading the reasoning on before the rest.
2. **M1.2's cache binding** is an addition, not on the original list. It's the highest-value thing the planning pass
   found and it touches six call sites plus every test that seeds the cache. Decided: KEEP. It also grew a scope: the
   `SCAN_PREVIEW_RESULTS` static is `pub(super)` and directly writable, so the canary only means something once the
   static is private.
3. **`hint-unwrap-or-default`**: **reversed.** Decided to BUILD it, method-scoped, as
   `desktop-rust-probe-unwrap-justified` (M3.4d). The original rejection was right that variable-name scoping can't
   work and wrong that the compiler substitutes for it: the compiler sees none of the eight hand-written
   `is_directory(...).await.unwrap_or(false)` sites, two of which pick a destructive branch.
4. **M1.6's `PerSource` enum** (replacing the empty-`Vec`-means-two-things field): decided to DEFER as a documented
   contract plus a named follow-up.
5. **The derives in `crates/cmdr-fs/src/volume/host/*`**: decided OUT of
   `desktop-rust-derive-default-justified`'s jurisdiction, and the carve-out widened to test-support files under
   `file_system/**` on the same reasoning. Note they're gated `#[cfg(any(test, feature = "testing"))]`, not
   `#[cfg(test)]`.
