# A copy that fails can never take data with it

**Status**: SPECCED, not started. **Owner**: David. **Date**: 2026-08-08.

`7046e9dbb` + `bf6d896b3` closed a data-safety hole that lived for three months in the cross-volume copy. This plan
generalizes the three lessons it taught into code, types, and checks, so the next instance of the same shape is caught
by the compiler or a check rather than by a user losing a folder.

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
`docs/doc-system.md`.

## What I checked, and what it changed about the brief

Findings that re-shape the work. Each is grounded in a grep or a read; don't re-derive them.

- **Compress consumes no preview cache.** `insert_scan_result` has exactly two production call sites, both in
  `scan_preview.rs`. `take_cached_scan_result` has six: local delete (`delete/walker.rs:34`), volume delete
  (`walker.rs:590`), local move (`transfer/move_op.rs:494`), local copy (`transfer/copy/mod.rs:126`), and volume
  copy/move (`transfer/volume/preflight.rs:137` and `:315`). `preview_id` appears nowhere in `archive_edit/` or
  `delete/trash.rs` or `rename.rs`. Compress's `previewId` reaches only `get_scan_preview_totals` for the dialog's
  size estimate. **So the cache-consumer sweep is copy, move, and delete — six pipelines — and nothing else.** Don't
  hunt for a compress or trash or rename consumer; there isn't one.
- **`Volume::delete` is strictly non-recursive by trait contract** (`crates/cmdr-fs/src/volume/mod.rs`: "must NOT
  recurse … if `path` is a non-empty directory, the implementation must return an error"). So the delete walker's
  `unwrap_or(false)` at `walker.rs:749` cannot over-delete: a directory wrongly classed as a file just fails its
  `delete` and surfaces as a per-item failure. **Under-delete, not data loss.** Don't let an agent "fix" this as if it
  were the copy bug; fix it for honesty, not for blast radius.
- **The real delete hole is different, and nobody listed it.** On a cache hit the LOCAL delete walker iterates
  `scan_result.files` — the paths the PREVIEW walked — and never looks at its own `sources` argument again
  (`walker.rs:34` → `:125`). The same is true of local copy and local move. Nothing anywhere verifies that the
  `preview_id` the frontend handed back describes the selection the operation was asked to act on. That is the exact
  shape of the bug we just fixed (a fact crossing a boundary, believed rather than checked), on the one op that can't
  be rolled back. **This is the highest-value item in the plan and it was not on the list.** See M1.2.
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
4. **The compiler beats a scanner.** Where a rule can be expressed as "this type has no `Default`", we do that, and we
   only write a Go check for what the compiler genuinely can't see.
5. **Never bump the `file-length` allowlist** (`.claude/rules/file-length-allowlist.md`). M1.1 exists precisely so the
   number goes down.
6. **Never `git push`.** Every phase lands by fast-forward from its worktree branch when David says so.

### Decisions taken on David's behalf while he was away

He asked to be shown pushback; these are the calls made so execution could start overnight. Each is cheap to reverse.

- **Open question 1 (M1.2's cache binding): KEEP.** It's the same unverified-fact shape as the bug we just fixed,
  sitting on the one operation with no rollback. Cutting it would leave the most dangerous instance of the class we're
  here to close.
- **Open question 2 (`hint-unwrap-or-default`): DON'T BUILD.** The planner's argument is empirical, not stylistic: the
  shape has zero instances repo-wide after the fix, and the regex genre provably can't see the value type the rule is
  predicated on. Removing `Default` (M1.5) makes the compiler enforce it exactly, and
  `desktop-rust-derive-default-justified` keeps that true for the next fact-carrying type. If David wants belt over
  braces anyway, the name-scoped fallback in M3.4(d) is ~20 lines.
- **Open question 3 (the `PerSource` enum): DEFER**, documented as a contract in M1.6 and left as a named follow-up.
  It has real reach across two crates and deserves its own decision, not a rider on a safety effort.
- **Open question 4 (`#[cfg(test)]` derives in `crates/cmdr-fs/src/volume/host/*`): OUT of jurisdiction.** A test
  stub's zero value is a test's problem, and putting them in scope buys annotation churn without buying safety.

---

# Phase 1 — Close the blast radius

Branch `safety-p1-cache-truth`, off `main`. Everything here is production behavior. Independently mergeable and
valuable on its own.

Milestones run SEQUENTIALLY inside the phase (M1.1 → M1.2 → M1.3 → M1.4). M1.3 and M1.4 are file-disjoint from each
other and could go in parallel once M1.2 has landed, but the signature change in M1.2 touches both of their call sites,
so serializing is cheaper than merging.

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
  empty `per_path` — `scan_preview.rs`'s inline test seeds `file_count: 7, per_path: Vec::new()` and WILL trip it.
  Fix the fixture (it should use a named constructor from M1.3 anyway), don't weaken the assert.
- The volume preview legitimately caches `files: Vec::new()` with a populated `per_path`. The canary is one-directional:
  `files > 0 && per_path == 0`, never the reverse.
- Set comparison must normalize nothing. If the frontend can hand a path that differs only by a trailing separator, the
  fix belongs at the IPC edge, not in a lenient comparison here — a lenient comparison is another belief.

**Docs owned.** `write_operations/DETAILS.md` § "Scan preview caching" (extend with the binding + the canary);
one guardrail line in `write_operations/CLAUDE.md`: *"A `preview_id` alone doesn't authorize acting on a path set —
`take_cached_scan_result` verifies the cached sources match the operation's, and a mismatch is a cache miss."*

**Tests — TEST-FIRST, real red → green.** This is a data-safety change; run the red step and record it.
1. `scan_cache_tests`: `take_cached_scan_result` returns `None` and warns when the requested sources differ from the
   cached ones (extra path, missing path, wholly different set); returns `Some` for the same set in a different order.
   *Red first: today the function takes one argument, so this test won't compile — that counts as red only after you
   write the signature and leave the body believing. Add the parameter, ignore it, watch the test FAIL, then use it.*
2. `delete_volume_reuse_tests.rs`: a delete asked for `/b` while the cache holds a preview of `/a` deletes `/b` and
   leaves `/a` intact. *This is the one that would be red today for the right reason.* Verify it actually goes red
   before the fix.
3. `scan_preview` unit test: `insert_scan_result` with `file_count: 3, per_path: []` trips the `debug_assert`
   (`#[should_panic]`) and, in a release-shaped path, still warns.

**Checks.** `pnpm check --fast` while iterating; `pnpm check rust-tests` at the end.

**File ownership.** `write_operations/scan_cache.rs`, `write_operations/scan_preview.rs`,
`write_operations/scan_cache_tests.rs` (new, `#[path]` sibling), and ONE line each in `delete/walker.rs`,
`transfer/move_op.rs`, `transfer/copy/mod.rs`, `transfer/volume/preflight.rs`. Call it out to the M1.3/M1.4 agents:
those single lines are M1.2's, everything else in those files is theirs.

**DONE when.** No consumer can act on a preview of a different selection, the canary fires in dev, and all six call
sites pass their real `sources`.

## M1.3: two named constructors, so the two production shapes have names

**Scope.** `CachedScanResult::from_local_walk(...)` and `CachedScanResult::from_volume_batch(...)`, replacing the two
struct literals in `scan_preview.rs`. Make the struct's fields `pub(super)`-but-not-constructible-elsewhere if the
module layout allows it; if it doesn't, the check in Phase 3 (M3.4) is the enforcement. Update the five test-side
struct literals to build through them.

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

**File ownership.** `write_operations/scan_cache.rs`, `write_operations/scan_preview.rs`, plus the five test files
that currently hand-build the struct (`copy_tests.rs`, `copy_source_hint_tests.rs`, `delete_volume_reuse_tests.rs`,
`commands/file_system/volume_copy.rs` tests, `scan_preview.rs`'s inline test).

**DONE when.** No struct literal of `CachedScanResult` exists outside `scan_cache.rs`.

## M1.4: delete's cache consumption, audited and made honest

**Scope.** Three things in `delete/walker.rs`, in order of how much they matter.

1. **Already fixed by M1.2** — the wrong-selection hole. Confirm the binding lands here and add the delete-specific
   regression test (M1.2's test 2 lives in this milestone's file if the agents prefer; own it in one place, not both).
2. **The `None` arm at `walker.rs:670` is correct as written** — it forwards `is_dir_hint: None`, and
   `scan_volume_recursive` PROPAGATES the probe error (`.map_err(...)?` at `:357-360`) rather than defaulting. Leave
   it. Add a comment saying so, because it looks like the bug and isn't.
3. **The no-preview path at `walker.rs:747-750` IS the asymmetry.** `volume.is_directory(source).await.unwrap_or(false)`
   guesses "file" on a probe error, while the cached path one screen up propagates. Route both through the same
   propagating resolve. Consequence: a source that can't be stat'd now fails that item instead of silently becoming a
   `delete` that fails anyway with a worse message. Also fix the oracle-hint lookup above it: it's a listing lookup, so
   a miss means unknown, and it already handles that correctly with `Option` — verify, don't change.

**Intention.** Delete is the op with no rollback, so the bar is "the agent can state, for every branch, what happens
on a wrong or missing fact". The audit is the deliverable as much as the diff. Record the `Volume::delete` non-recursion
contract in `delete/DETAILS.md` as the reason a wrong `is_dir` here is annoyance-class rather than loss-class — so the
next reader doesn't over-correct.

**Landmines.**
- `scan_volume_recursive`'s cancel contract: a recursive bail must NOT emit `write-cancelled` itself. Any new early
  return you add needs `emit_cancelled_if_aborted` at the top-level caller, or the FE closes via the settle fallback.
  Pinned by `delete_cancel_during_scan_emits_write_cancelled`.
- The volume delete's cache-hit branch reads a LOCAL preview's `per_path` happily (and correctly, post-`bf6d896b3`).
  Don't add a "reject a local preview" gate; it would regress the win that commit shipped.
- Don't touch `trash.rs`. Trash has no scan phase and no cache.

**Docs owned.** `delete/CLAUDE.md` (one guardrail on the cache binding), `delete/DETAILS.md` (the audit: what each
branch does with a missing/wrong fact, and the `Volume::delete` non-recursion reasoning).

**Tests — TEST-FIRST for the probe-error propagation.**
1. Red: a volume whose `is_directory` errors for one source, no preview. Today the walker classes it a file, tries
   `delete`, and reports the backend's message; assert instead that the operation surfaces the stat failure at that
   path. Watch it fail.
2. After: a cache hit whose `per_path` is EMPTY but whose `file_count` is non-zero (the exact production shape the bug
   rode in on) still deletes the right tree — the `None` arm's probe path, pinned.

**Checks.** `pnpm check --fast`, then `pnpm check rust-tests`.

**File ownership.** `delete/walker.rs`, `delete/CLAUDE.md`, `delete/DETAILS.md`,
`delete/delete_volume_reuse_tests.rs`.

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
- **`move_same.rs:478`** — same-volume move, `unwrap_or(false)` on the source type. Consequence of a wrong `false`: the
  dir-vs-dir merge branch at `:512` is skipped, so a folder-onto-folder collision goes through
  `resolve_volume_conflict` instead of `rename_merge_directory`. With Overwrite that's a cross-type-looking clash on a
  path where both sides are directories — `conflict.rs:434`'s `same_type_dir` catches it and merges, so it degrades
  rather than destroys. Still a wrong branch on a belief: **route it through `strategy::resolve_source_is_directory`**,
  the same helper the other three pipelines now use.
- **`delete/walker.rs:749`** — owned by M1.4, listed here for completeness.
- Everything else the sweep turns up: if a wrong value can't change a destructive branch, write the one-line "why the
  default is truthful" and move on. Don't churn.

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

**Docs owned.** `transfer/volume/CLAUDE.md` (tighten the existing hint guardrail to cover the conflict resolver),
`transfer/volume/DETAILS.md` § "A missing source hint means unknown" (add the per-site decisions).

**Tests — TEST-FIRST for the `conflict.rs:80` change.**
1. Red: a source volume whose `is_directory` errors, a destination holding a same-named DIRECTORY, policy Overwrite.
   Assert the destination folder and its contents survive and the op reports a failure at that source. This should go
   red on today's code by deleting the folder. **Verify the red is the destructive one**, not a compile error.
2. After: `SourceHint` has no `Default` (compile-level; no test needed), and `move_same` with a hintless directory
   source onto a same-named dest directory takes the rename-merge path.

**Checks.** `pnpm check --fast` throughout; **one full `pnpm check` at the end of Phase 1**, which is the phase's
budget.

**File ownership.** `transfer/volume/preflight.rs`, `transfer/volume/conflict.rs`, `transfer/volume/move_same.rs`,
`transfer/volume/CLAUDE.md`, `transfer/volume/DETAILS.md`, plus new/edited `conflict` and `move_same` test siblings.

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
nothing in Phase 1 and touches no file Phase 1 touches.

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
- `prune_created_dir_if_empty(volume, &Path)` — the empty-only, deepest-first delete rollback already does for
  `created_dirs`. Unchanged behavior, now a named function instead of an inline `volume.delete` with a five-line
  comment explaining why it must not recurse.
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
bullet with the stronger structural claim. `transfer/volume/DETAILS.md` — a new § "Three ways to delete, and who may
use each", carrying the rejected `DirectoryCreation::Created` design and why (so nobody proposes it again).

**Tests — TEST-FIRST, this is the phase's whole point.**
1. **Red first, and it must actually be red today.** Reach into `copy_serial`'s post-loop with a fixture where a
   directory source's dest root leaks into `last_dest_path` (simulate the dropped-future window directly if you can't
   provoke it, e.g. by a unit test on the post-loop sweep function with a directory path in `partials_to_clean`) and
   assert the user's pre-existing dest-only file survives. On today's code the recursive delete takes it. After the
   split, the sweep calls `delete_written_file`, which fails benignly on a non-empty directory.
2. Each `TreeRemoval` variant's call site keeps its existing behavior: file→folder Overwrite still clears the dest
   folder (`conflict` tests), the move source sweep still spares skipped children
   (`move_merge_tests.rs` — must stay green untouched), archive move-out still removes remote originals.
3. `remove_tree` still names the leaf that refused (the existing `..._reports_the_leaf_that_refused` test, renamed).

**Checks.** `pnpm check --fast` throughout; **one full `pnpm check`** at the end of the phase.

**File ownership.** `transfer/volume/cleanup.rs`, `transfer/volume/copy.rs`, `transfer/volume/conflict.rs`,
`transfer/volume/move.rs`, `transfer/volume/mod.rs`, `archive_edit/copy_into.rs`, `transfer/volume/copy_tests.rs`
(the four renamed tests), and the two docs above.

**DONE when.** `grep -rn "recursive" transfer/volume/cleanup.rs` shows exactly one recursive function, no cleanup or
rollback call site can reach it, and every call that can names its authorization in the type.

---

# Phase 3 — The coverage grid and the checks

Branch `safety-p3-grid-and-checks`, off `main`. Nothing here changes production behavior; it can merge before or after
Phases 1-2 with one stated exception (M3.4 wants Phase 1's `SourceHint` derive already gone, or it annotates one extra
site).

Milestones M3.1 → M3.2 → M3.3 are sequential (each builds on the previous). **M3.4 (the checks) is file-disjoint from
all of them — `scripts/check/**` versus `apps/desktop/**` — and is the one place a second agent can safely run in
parallel**, with the single caveat that its annotation pass touches `file_system/**` and `crates/cmdr-fs/**` derives.
Give the annotation pass to M3.4 and forbid M3.1-M3.3 from touching any `#[derive(...)]` line.

## M3.1: one shared safety oracle, extracted from the two that already exist

**Scope.** Pull the fixture builder + assertion helper out of `move_merge_tests.rs` and `merge_tests.rs` into
`transfer/volume/safety_oracle.rs` (a `#[path]` sibling, `pub(super)`), and re-point both existing suites at it. No new
assertions yet — this is a pure extraction whose proof is that both suites stay green.

**The oracle, stated once.** For a finished operation: (i) **no byte the user didn't approve is gone from either
side** — every source file's content is readable from the source or the destination; (ii) **every byte they did approve
is at the destination**; (iii) **every dest-only file the source didn't shadow is untouched**. `move_merge_tests.rs`'s
`assert_move_merge_preserved_everything` is already (i) + (iii); (ii) is the addition.

**Intention.** One oracle, three op-shaped drivers. The brief asked for ONE parametrized body over all four axes; see
the pushback below for why that's the wrong shape and what replaces it.

**Landmines.** The existing helpers compare CONTENTS, not paths, because Rename relocates files. Keep that. Don't
"simplify" `collect_contents` into a path assertion.

**Tests.** The two existing suites, unchanged, green.
**Checks.** `pnpm check --fast rust-tests`.
**File ownership.** `transfer/volume/safety_oracle.rs` (new), `transfer/volume/merge_tests.rs`,
`transfer/volume/move_merge_tests.rs`, `transfer/volume/mod.rs` (module decl).

## M3.2: a volume that lies, as a first-class fault class

**Scope.** Extend, don't add. Two layers:
- `crates/cmdr-fs/src/volume/in_memory.rs` already has `with_delete_failing`, `set_reported_size`, `set_modified_at`.
  Add the metadata lies the grid needs: `set_reported_type(path, is_directory)` (report a directory as a file and vice
  versa) and `set_stat_failing(path)` (`is_directory` / `get_metadata` return `IoError`).
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
**File ownership.** `crates/cmdr-fs/src/volume/in_memory.rs`, `crates/cmdr-fs/src/volume/in_memory_test.rs`,
`transfer/volume/faulty_volume_test_support.rs` (new), and the two existing support files it absorbs from.

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

**Tier A — exhaustive, the blast-radius cell.** `{copy, move, delete}` × item kind `dir-onto-an-existing-dir (merge)`
× cache state `{miss, hit-with-per-path, hit-without-per-path}` × outcome `{clean, fail-mid-op, cancel-mid-op}` =
**27 cells**. Every one of these must exist and pass. This is the exact intersection the bug lived in: a directory,
merging into the user's folder, with a half-populated cache, interrupted.

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
- Nine of Tier A's cells are delete, which has no destination — for those, (ii) of the oracle degenerates and (i)
  becomes "nothing outside the requested set is gone". Say so in the driver rather than letting the helper quietly
  assert nothing.
- Table-drive the cells; ❌ don't generate names with a macro that makes a failure un-greppable.
- Watch the per-test duration budget (`docs/testing.md` § Caps are not runtimes). 39 in-memory cells should be well
  under a second each.

**Tests.** This milestone IS the tests. Written after the oracle exists (M3.1) — the TDD discipline for this effort
lives in Phases 1 and 2, where the production changes are; a coverage grid written test-first against unchanged code
would be red for the wrong reason.

**Checks.** `pnpm check rust-tests`, then **the phase's one full `pnpm check`**.
**File ownership.** `transfer/volume/safety_grid_tests.rs` (new), `transfer/volume/mod.rs`.

## M3.4: three real-SMB cells, and three new checks

Two independent deliverables in one milestone because they share nothing with M3.1-M3.3 and can run alongside them.

### (a) The real SMB cells

Three `smb_integration_*` tests in `apps/desktop/src-tauri/src/file_system/volume/backends/smb_transfer_semantics_test.rs`
(which already holds `smb_integration_same_share_move_merges_with_no_folder_prompt` and friends, and which
`desktop-rust-integration-tests` already brings the SMB stack up for via `scripts/check/smb_orchestrator.go`):

1. **The production bug, on the wire.** A LOCAL directory copied onto a same-named directory that already exists on the
   share, with a cache hit whose `per_path` is empty, forced to fail mid-copy. Assert the share's pre-existing files
   survive and the source is intact.
2. **The same for the cross-volume MOVE.**
3. **An SMB delete consuming a LOCAL preview's cache**, asserting it deletes the requested tree and nothing else.

These three are the cells where the in-memory doubles genuinely can't stand in: a real share publishes bytes at the
write path, has real failure timing, and has a `create_directory` that really does signal collisions.

**Landmine.** Read `apps/desktop/test/smb-servers/README.md` and the orchestrator's lease model before running these
locally; a manual `start.sh` alongside a `pnpm check` run is fine (both take leases), a manual `docker compose down` is
not.

### (b) `desktop-rust-no-hand-rolled-fixture` — build it, it has teeth

Ban struct-literal construction of an allowlisted set of cross-boundary types inside `*_tests.rs` and `#[cfg(test)]`
blocks. Allowlist: `CachedScanResult`, `SourceHint`, `VolumePreflight`. Sibling of `desktop-rust-fixed-temp-dir` and
`desktop-rust-test-sleep`: same `ScannerRoots` + `isRustTestPath` + `advanceTestModRegion` + `directiveTracker` shape,
opt-out `// allowed-hand-rolled-fixture: <why>`.

**This one is regex-feasible and it lands with real findings**, which is the whole difference from item (d) below: a
struct literal NAMES ITS TYPE at the construction site (`CachedScanResult {`), so no type inference is needed. Current
violations: 2 in `copy_tests.rs`, 1 in `copy_source_hint_tests.rs`, 1 in `delete_volume_reuse_tests.rs`, 1 in
`scan_preview.rs`'s inline test, 2 in `commands/file_system/volume_copy.rs`'s tests. Seven sites on day one — all of
which Phase 1's M1.3 converts, so the check ships green and STAYS green by construction.

**Landmines.** Don't flag the named constructors' own bodies (they're in production files, so the `#[cfg(test)]`
scoping already excludes them). Don't flag `Type::from_x(...)`. Handle a multi-line literal (the type name and the
brace are on the same line in every current instance, but assert that in the check's own test).

### (c) `desktop-rust-derive-default-justified` — build it, it's the real enforcement

Any `#[derive(..., Default, ...)]` on a struct or enum under `apps/desktop/src-tauri/src/file_system/**` or
`crates/cmdr-fs/**` must carry a `// DEFAULT-OK: <why the zero value is a truthful claim>` line in the comment block
immediately above (same block-comment handling `desktop-rust-fixed-temp-dir` already implements).

**Current population: 26 derives** in those two trees (14 under `write_operations/`, 7 in `cmdr-fs`, the rest in
`listing/` and `sync_status/`). Several are `#[cfg(test)]` host stubs in `crates/cmdr-fs/src/volume/host/*`; per the
decision above they are OUT of jurisdiction (a test stub's zero value is a test's problem), so implement that scoping
rather than annotating them.

**This is what makes M1.5's `Default` removal a rule rather than a one-off.** Annotating 20-odd sites is the
milestone's real work, and each annotation is a small honest thought: `ListingProgress`'s zero really is "nothing
enumerated yet"; `SortColumn`'s really is a preference default. If an annotation is hard to write, that derive is the
next bug.

### (d) `hint-unwrap-or-default` — NOT BUILT (decision recorded)

Four reasons, in order of weight:

1. **The regex genre cannot see the value type of a map lookup, and that's the entire predicate.**
   `source_hints.get(p).copied().unwrap_or_default()` and `counts.get(k).copied().unwrap_or_default()` are the same
   token stream. The type name never appears at the call site, so a denylist of type names has nothing to match on.
   There is no version of this check in this genre that distinguishes the dangerous case from the fine one.
2. **It ships with zero findings and no way to stay honest.** After `7046e9dbb`, the exact shape has zero instances
   repo-wide. Grepping all of `apps/desktop/src-tauri/src` + `crates` for `.get(...)` on a line with
   `unwrap_or_default()` returns exactly four sites: a JSON row (`row.get("text").and_then(Value::as_str)`), a regex
   capture (`caps.get(0)`), and two `OnceCell<Settings>` reads. All four are truthful defaults on types that aren't
   filesystem facts. A check that can't tell those from the real thing and has nothing real to find is a check that
   will be quietly disabled the first time it cries wolf.
3. **The compiler already does this exactly.** Remove `Default` from the fact-carrying type (M1.5) and
   `unwrap_or_default()` on it is a compile error naming the precise call site — no false positives, no allowlist, no
   maintenance. `desktop-rust-derive-default-justified` is what keeps that true for the NEXT fact-carrying type someone
   writes. Two mechanisms for one rule, one exact and one guessy, is the opposite of elegance (#2).
4. **The fallback form isn't worth it either.** The only honest regex version is name-scoped: flag `unwrap_or_default()`
   / `unwrap_or(false)` where the receiver identifier is in a hand-maintained list (`source_hints`, `per_path`,
   `by_path`, `parent_hint`, `hint`) under `file_system/**`. That's ~20 lines with a knowable FP set — but its entire
   value is subsumed by (3), and its list rots the moment someone names a map `hints_by_source`.

**If David wants it anyway**, build the name-scoped form, keep the list in the check file with a comment saying it's a
belt over the compiler's braces, and expect it to find nothing.

**Landmines for (b) and (c).**
- Register both in `registry.go`'s `AllChecks` with the `desktop-rust-` ID prefix the convention uses
  (`desktop-rust-no-hand-rolled-fixture`, `desktop-rust-derive-default-justified`), nicknames without it.
- **Declare `Inputs`** (`rustInputs`) or `TestEveryCheckDeclaresInputs` fails the suite.
- **Wire both into `.github/workflows/ci.yml`** or `ci-coverage` fails both ways.
- Each needs its own `_test.go`.
- Take roots from `ScannerRoots(ctx.RootDir, "<check-id>")`, ❌ never a hardcoded source path
  (`workspace-member-coverage` enforces it).
- `IsFast: true` for both (they're line scanners).
- After authoring: `pnpm check go-vet staticcheck` and update `scripts/check/checks/DETAILS.md` § "Apps and check
  counts".

**Docs owned.** `scripts/check/checks/DETAILS.md` (the two new checks + the count), one line each in
`scripts/check/checks/CLAUDE.md`'s module map if the genre list enumerates them, and `docs/testing.md` § "When you add
X, also add Y" if the fixture rule belongs there.

**Tests.** Go `_test.go` per check (positive, negative, directive-honored, orphan-directive-fails). The three SMB
cells are the tests for (a).

**Checks.** `pnpm check go-vet staticcheck`, `pnpm check ci-coverage workspace-member-coverage`,
`pnpm check desktop-rust-integration-tests` for the SMB cells. **`pnpm check --include-slow` runs once, at the very
end of Phase 3**, and closes the effort.

**File ownership.** `scripts/check/checks/desktop-rust-no-hand-rolled-fixture.go` (+ `_test.go`),
`scripts/check/checks/desktop-rust-derive-default-justified.go` (+ `_test.go`), `scripts/check/checks/registry.go`,
`scripts/check/checks/DETAILS.md`, `.github/workflows/ci.yml`, every `#[derive]` line under `file_system/**` and
`crates/cmdr-fs/**` that needs an annotation, and
`apps/desktop/src-tauri/src/file_system/volume/backends/smb_transfer_semantics_test.rs`.

---

## Sequencing and the execution model

Each phase is a subagent on its own worktree, branched off `main`. Ownership boundaries above are exhaustive: two
agents never write the same file.

- **P1 → P2 → P3** is the recommended merge order, but only P3(c)'s annotation pass has a real dependency (it wants
  P1's `SourceHint` derive already removed; if P1 hasn't landed it annotates one extra site and P1's removal deletes
  the annotation with the derive — a one-line conflict at worst).
- **P1 and P2 are file-disjoint** (`scan*.rs` + `delete/` + `preflight/conflict/move_same` versus
  `cleanup/copy/move/mod` + `archive_edit`) with one overlap: `conflict.rs`, which P1.M5 edits at line 80 and P2.M1
  edits at line 439. **Recommendation: run them sequentially anyway.** They're the same subsystem and the second agent
  benefits from reading the first's diff. We're not in a hurry.
- **P3.M4 is the only safe parallel slot** — `scripts/check/**` shares nothing with `apps/desktop/**` except the
  annotation pass, which is `#[derive]` lines nobody else may touch.

**Check budget.** `--fast` freely inside every milestone. **One full `pnpm check` per phase** (at P1.M5, P2.M1, and
P3.M3). **`--include-slow` once, at the very end of P3.** An agent that needs more for confidence should take it and
say why in its report. ❌ Never `git push`.

**Per-milestone ritual** (`AGENTS.md` § Workflow): step back and ask whether it's solid AND elegant AND documented
before calling it done.

## Invariants register

The properties every milestone must leave true. An end-of-phase conformance pass reads this list.

1. A missing `source_hints` entry means UNKNOWN, never "file", and the RESOLVED answer drives the cleanup/ledger
   branch. ❌ No probe where a hint EXISTS.
2. No type carrying a filesystem fact has a `Default`.
3. Cleanup and rollback for a DIRECTORY source are per-FILE, never the dir root — structurally, not by convention.
4. A merge never deletes or overwrites a dest file the source doesn't shadow.
5. A MOVE's source sweep spares every child the merge skipped.
6. A `preview_id` doesn't authorize acting on a path set; the cached sources must match the operation's.
7. A completed preview always carries `per_path`, whichever walk produced it.
8. A failure carries the path it happened ON; a directory sweep names the first child that refused.
9. Dir-vs-dir is never a conflict; only files prompt.
10. `Volume::delete` never recurses.
11. Every check declares `Inputs` and is wired into CI.
12. The `file-length` and `claude-md-length` allowlists only ever shrink.

## Open questions for David

All four were decided provisionally so execution could proceed overnight; see § "Decisions taken on David's behalf".
Each is cheap to reverse.

1. **M1.2's cache binding** is an addition, not on the original list. It's the highest-value thing the planning pass
   found and it touches six call sites plus every test that seeds the cache. Decided: KEEP.
2. **`hint-unwrap-or-default`**: decided NOT to build (M3.4d). Confirm, or ask for the name-scoped fallback.
3. **M1.6's `PerSource` enum** (replacing the empty-`Vec`-means-two-things field): decided to DEFER as a documented
   contract plus a named follow-up.
4. **`crates/cmdr-fs/src/volume/host/*`'s `#[cfg(test)]` derives**: decided OUT of
   `desktop-rust-derive-default-justified`'s jurisdiction.
