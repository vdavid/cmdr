# Scoped incremental walk

**SHIPPED 2026-07-29.** Make an importance incremental rescore cost O(touched) instead of O(dirs), by reading only the
changed subtrees out of the index instead of walking the whole thing. The durable version of everything below now lives
in `crates/cmdr-index/src/importance/scheduler/DETAILS.md` § The scoped walk; this stays until the next specs wipe as
the reasoning trail.

Area docs: `crates/cmdr-index/src/importance/scheduler/DETAILS.md` (the canonical home for the mechanism once this
ships). Evidence: `docs/notes/idle-memory-profile-2026-07-28.md`.

## The problem

`run_incremental_blocking` calls `walk_index_folders`, which reads **every** directory and **every** file row of the
volume, folds each folder's aggregate, and runs two whole-tree propagations. Only then does `incremental_rescore` filter
to the touched subset. The targeted write that follows is sub-millisecond (binary `path_folded` PK).

Measured 2026-07-27 (warm cache, five alternating runs, `cargo run -p index-query --bin importance-measure`):

- root index, 611,699 folders: walk ~5.5 s
- NAS index, 391,563 folders: walk ~6.4 s

So the walk is essentially the whole cost of an incremental, and it is why the pass needs a 60-second throttle
(`INCREMENTAL_THROTTLE_WINDOW`).

## Why it was deferred

Two whole-tree propagations in `walk_index_folders` cross the subtree boundary:

- `has_marker_below` (upward): a `.git` deep in a tree raises every ancestor above it, including ancestors outside any
  walked subtree.
- `under_floored_ancestor` (downward): a self-flooring folder floors its whole subtree, and the flooring ancestor can
  sit outside the walked subtree.

They are not equally hard, and separating them is what makes this tractable.

## Design

### 1. `under_floored_ancestor` needs no walk at all

`self_floors(path, name, home)` is pure path math. A folder's ancestors are exactly the prefixes of its own absolute
path, and each ancestor's name is that prefix's last component. So

```
under_floored_ancestor(f) = any_ancestor_self_floors(f.path, home)
```

which `classify.rs` already implements (`floors_by_path` calls it). This is **identical** to what the full walk's
`propagate_floor_to_descendants` computes, not an approximation:

- The full walk seeds from every tree row except the root sentinel, then walks parent pointers. Every strict ancestor of
  `f` is a directory in the index (the index is a tree), its path is a prefix of `f`'s path, and `DirTree::name_at`
  equals `leaf_name` of that prefix (a name can't contain `/`).
- The full walk stops at `ROOT_ID`; `any_ancestor_self_floors` stops at `pos == 0`. Same boundary.

So the scoped walk drops the seed-and-propagate pass entirely and classifies each folder from its own reconstructed
path. No boundary case, no fallback.

### 2. `has_marker_below` is exact inside a walked subtree

`propagate_marker_to_ancestors` sets `has_marker_below(x) = ∃ strict descendant d of x with has_direct_marker(d)`.

For a folder `f` inside a walked subtree rooted at origin `C`, every descendant of `f` is also inside that subtree
(subtrees are downward-closed). So the scoped walk computes `has_marker_below(f)` exactly, from the same propagation
code, over the scoped tree.

### 3. Ancestors outside the walked subtrees

Let `A` be a strict ancestor of origin `C`, outside every walked subtree. Which of `A`'s signals can move because of a
change inside `subtree(C)`?

- `modified_at(A)`: no. A directory's mtime changes only when its own listing changes, and then `A` is itself an origin
  (`dir-changed` carries origin dirs).
- `children` (`file_count`, `distinct_extension_count`, `has_direct_marker`): direct children only, so same argument.
- `under_floored_ancestor(A)`: depends on `A`'s own ancestor chain. A rename up there makes the renamed folder's
  **parent** an origin, which puts `A` inside that origin's subtree, not outside it.
- `has_marker_below(A)`: **yes.** A marker created or deleted anywhere in `subtree(C)` flips it.

So `has_marker_below` is the only cross-boundary signal, and it decomposes cleanly:

```
has_marker_below(A) = markerOutside(A, {C_i}) OR OR_i M(C_i)
M(C) = has_direct_marker(C) OR has_marker_below(C)   -- "subtree(C) contains a marker"
```

`markerOutside` is untouched by anything inside the subtrees. So **the ancestors' `has_marker_below` changes if and only
if some `M(C_i)` changes.**

`M(C)` is exactly the `has_project_marker` field `signals_for_dir` computes and the store persists
(`has_project_marker = children.has_direct_marker || has_marker_below`). So the previous `M(C)` is readable straight off
`C`'s stored row, and the new one comes out of the scoped walk.

**The guard:** if every origin's `M` is unchanged, no ancestor outside the walked subtrees can have changed. If any
origin's `M` flipped, or an origin has no stored row to compare against, this pass **falls back to the full walk**,
which recomputes everything exactly the way it does today.

Creating or deleting a project marker is rare, so the expensive path is rare, and correctness comes from construction
rather than from an argument about likelihood.

#### An origin with no stored row

`sanitize_incremental_batch` already drops floored paths, so an origin always earns a row when it is walked. A missing
row therefore means "this folder did not exist at the last pass that covered it" (or the store has never had a full
pass). We cannot tell those apart, so a missing row falls back to the full walk.

In practice this is rarer than it sounds: a brand-new directory makes its **parent's** listing change, so the parent is
an origin too, and origin de-duplication (below) drops the new child in favour of the parent, which does have a row.

### 4. Strict ancestors are no longer rescored on the scoped path

`incremental_rescore` currently writes rows for `touched ∪ in_changed_subtree`, where `touched` is each origin's capped
ancestor chain. On the scoped path the walk holds only the subtrees, so the rows written are exactly
`in_changed_subtree`.

**Decision/Why the accepted lossiness.** A strict ancestor's stored row goes stale in exactly two ways once we stop
rewriting it every pass:

- Its **recency term** stops being recomputed against a fresh `now_secs`. Every other folder on the volume already ages
  that way between full passes, so this makes the store _more_ uniform, not less: today an ancestor of a churny
  directory gets its score decayed every 60 s while its siblings keep an older, higher `now_secs`.
- A **visit** recorded since its row was written folds in later (at the next full pass, or the next pass that actually
  covers it). Visits already lag a pass today.

`has_project_marker`, the one signal that genuinely propagates upward, is covered by the guard in §3, so it is never
silently wrong. Importance is advisory derived data and the full pass is the backstop, which is the same reasoning the
batch gate's floor filter already rests on (`DETAILS.md` § The batch gate) — this extends that decision rather than
contradicting it.

### 5. Scoping a multi-origin batch

A batch carries many origins. The rule, in order:

1. **De-duplicate nested origins.** If `P` and `P/x` are both in the batch, drop `P/x`: `subtree(P/x) ⊆ subtree(P)`, so
   the walked folder set and the cleared region are both unchanged. This keeps clear and insert on the same slice (the
   deduped slice is used for both) and stops a wide batch from walking the same rows twice.
2. **Walk each surviving origin's subtree separately**, into one shared scoped `DirTree`. One walk over their common
   ancestor is wrong: unrelated origins can share only `/`, which would re-walk the volume.
3. **Bail out to the full walk past a crossover.** Two named constants, both in `scoped_walk.rs`:
   - `SCOPED_WALK_MAX_ORIGINS` — past this many deduped origins, the per-origin resolution overhead plus the risk of
     overlapping breadth stops being worth it.
   - `SCOPED_WALK_MAX_DIRS` — the scoped walk counts directories as it descends and abandons the moment it passes this,
     so a batch that turns out to cover most of the volume costs a bounded probe and then takes the full walk.

   The second is the load-bearing one: it is checked _during_ the descent, so no batch can make the scoped path cost
   more than a small fraction of a full walk before it gives up.

### 6. Reading the subtree out of the index

Per surviving origin:

- Resolve the origin path to an entry id by descending `resolve_component` from `ROOT_ID` (indexed point queries,
  O(depth)). An origin that does not resolve is a folder deleted between publish and pass: it contributes no folders,
  and the clear still removes its rows, which is what the full walk does today too.
- Read the ancestor chain rows (`id`, `parent_id`, `name`) upward from the origin, so paths reconstruct from the index's
  **own** names. This matters: `resolve_component` matches on the folded name, so the batch's spelling of a path can
  differ in case from the index's, and the stored `path` column must stay byte-identical to what a full pass writes.
- Descend level by level with a batched `WHERE parent_id IN (…) AND is_directory = 1` query, and fold the file children
  with a batched `WHERE parent_id IN (…) AND is_directory = 0` query. Both are served by the `(parent_id, name_folded)`
  index.

The result is a `WalkedFolders` with the same invariants the full walk produces (tree rows in ascending id order, folder
records in ascending `dir_index` order), so every downstream consumer is unchanged.

## What it measured

Measured 2026-07-29 with `cargo run --release -p index-query --bin importance-diff`, against read-only copies of the
real `index-root.db` (611,699 folders) and `index-smb-…naspi.db` (391,563 folders):

- **Scoped walk per origin**: median 165 µs (root, 764 sampled origins; mean 352 µs, max 12.2 ms), median 98 µs (NAS,
  200 sampled origins; mean 488 µs, max 47.9 ms). Against a full walk's 5.5 s / 6.4 s. Folders read per origin: median
  1, max 1,044 (root) / 6,026 (NAS).
- **Disagreements with the oracle**: zero, across 964 origins of the two indexes (7,297 + 2,087 rows compared) — same
  paths, same scores, same signal blobs. No sampled origin fell back.
- **Fallback probe** (an origin whose subtree blows `SCOPED_WALK_MAX_DIRS`): 15–390 ms before it gives up, against the
  5.5 s full walk it then runs. Origins that fall back on the root index are the ones sitting above most of a dev tree
  (`~`, a repo root with `node_modules` + `target` + `.git`, `/Applications`).

## Correctness property (the safety net)

The claim the differential harness pins, over real and synthetic indexes:

> For the same index, home, and origin set, the scoped walk yields exactly the folders the full walk yields restricted
> to `is_in_changed_subtree`, with identical `path`, `modified_at`, `children`, `has_marker_below`, and
> `under_floored_ancestor`.

Row equality follows, because `incremental_rescore` is a pure function of those plus `now_secs`, the weights, and the
optional-signal maps. Scores drift with the wall clock (the recency signal), so both sides take a fixed `now_secs`.

The property tests run every scenario through **both** strategies (`WalkStrategy::Scoped` and `WalkStrategy::FullOnly`)
against a real `IndexStore` DB and assert the resulting store contents match: a marker created inside a subtree, a
marker deleted, a folder renamed to `node_modules`, renamed away again, a change under an already-floored ancestor, a
change at the volume root, a batch spanning unrelated subtrees, and an origin deleted between publish and pass.

## Invariants this must not break

- Incremental writes at the CURRENT generation, never bumps it, never escalates to a full pass from a `/` batch.
- The clear and the insert stay on the SAME (deduped) `changed_paths` slice.
- A floored folder gets no row; floor beats marker.
- The full walk stays as both the fallback path and the oracle; `walk_memory_tests.rs` keeps guarding its memory shape.
- Typed classification only: the fallback reason is an enum, never a message.

## The throttle

`INCREMENTAL_THROTTLE_WINDOW` (60 s) exists because each incremental paid a full walk. Once a typical pass is
milliseconds, the throttle is no longer paying for the walk; it only paces the store write and the
`notify_recompute_completed` weight reload in `search::volumes`, which is itself O(all weights) and is the remaining
per-pass cost.

**Recommendation, not taken here:** relaxing the window is a separate decision, and it should wait until the weight
reload is incremental too. Lowering it now would trade a cheap walk for a frequent full weight-map rebuild (161,094
weights on a dev machine, `docs/notes/idle-memory-profile-2026-07-28.md`).

## Two things found along the way

1. **A real bug in the first cut, caught by the differential.** A batch whose origins were all deleted between the
   publish and the pass produces an EMPTY scoped walk, and `run_incremental_blocking`'s `folders.is_empty()` early
   return then skipped the CLEAR — every deleted folder's weight stayed until the next full pass. The guard is gone,
   with a `❌ don't reintroduce` note; a pass is also announced now whether or not it wrote a row, since it cleared
   either way.
2. **A pre-existing gap, NOT fixed here.** The subtree clear folds the path (`path_folded` is the PK) while
   `is_in_changed_subtree` / `touched_folder_set` compare bytes, so an origin spelled in a different case than the index
   holds it clears rows that nothing re-adds. Both walks lose the row identically, so this is neither caused nor made
   worse by the scoped walk. The fix would be to canonicalize each origin against the index (resolve, then rebuild the
   path from the index's own names) before either walk, which the scoped walk already does internally for its rows — ≤
   32 point queries per pass. Worth doing, but it changes the full-walk oracle's behaviour, so it belongs in its own
   change. Pinned meanwhile as a differential-only scenario.
