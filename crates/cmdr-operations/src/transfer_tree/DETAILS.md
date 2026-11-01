# Transfer tree: details

Read this before any non-trivial work here. Must-knows: `CLAUDE.md`. What the tree promises the person watching it, and
the frontend half: `apps/desktop/src/lib/file-operations/transfer-tree/DETAILS.md`. Issue:
[#382](https://github.com/vdavid/cmdr/issues/382).

**Budget for a million-file job**: ≤ 64 B per node in memory (below), freed when its dialog closes after the operation
settles (at most 10 minutes later); ≤ 200 rows per view answer and ≤ 2,000 hand toggles each way per request; O(depth)
per file boundary and O(|expansion| + 200) per view. **Why it's held this tight**: volume scans never kept a per-file
record before this tree, so the tree is the one thing a huge remote copy holds that it didn't, and the local engine's
own file list already costs ~4× it.

## Where a tree comes from

Every transfer's scan builds one, and the operation installs it on `WriteOperationState::transfer_tree`
(`TransferTreeSlot::install`, once) as soon as the scan is done:

- **Local copy and cross-FS move**: `ScanResult::order_by_transfer_tree` (`../scan_cache.rs`). The preview's tree when a
  `ScanPreviewPurpose::Transfer` preview built one (`../scan_preview/local.rs`, after the walk and off it), else built
  right there from the walk's `files` and `dirs`. Either way each `FileInfo` learns its `tree_node`, and `files` is
  sorted into the tree's order.
- **Volume copy and cross-volume move**: `scan_volume_sources` (`../transfer/volume/preflight.rs`) hangs a
  `TreeRecorder` on the scan's `ScanBoundary` and hands the tree back on `VolumePreflight::tree`, which the copy or move
  installs with where it lands. A cached preview's tree is taken as is; a cached preview without one (a delete's, never
  consumed here today) degrades to `from_per_path`: each source a file or an opaque folder.
- **Volume previews**: `../scan_preview/volume.rs` records both branches: the backend's batch scan through the boundary,
  and the fresh-listing oracle's walk (`../scan.rs::scan_subtree_with_oracle`) directly. A cached top-level child the
  oracle can't name reaches the recorder as a subtree, since that branch never walks it.
- **Compress**: `from_archive_plan` over `FreshPlan::entries`, whose names are already relative to the selection's
  parent (`../archive_edit/fresh_compress.rs`). `from_archive_plan` also returns each entry's node, in plan order.
- **Same-FS and same-volume moves** (renames): `from_sources`, one row per selected item and nothing below a folder
  (`../transfer/move_op/same_fs.rs`, `../transfer/volume/move_same.rs`).

**Decision: a rename move's rows are its items, not a tree of their contents.** **Why**: a rename takes a whole folder
in one call and walks nothing, so the item is the unit of work, the same unit the operation's own counts use (a same-FS
move's status counts items). A scanned tree would show a folder's 312 files at 0/312 and then all at once, which claims
a per-file progress that never happened, and scanning only to draw rows would cost the walk a rename exists to skip.
Each row flips done (or skipped) as its rename lands, and a folder row has no children to expand. A same-volume move's
items already moved into their destination (`already_in_place`) are no rows: they never reach the driver.

## The arena

- `Node` (16 B): `parent`, a name as `(offset, len)` into one shared `String`, `kind`, a leaf `state`, and a `slot` into
  `file_sizes` (files) or `folders` (`FolderAgg`, 40 B: the child range plus file counts and bytes below it).
- **Breadth-first ids** make a folder's scanned children `first_child..first_child + child_count`, so a child's position
  is a subtraction and the arena carries no child list. Discovered children live in `extra_children`.
- `heap_bytes()` counts capacity. 47.0 B/node at 1,000,002 nodes of 23-byte names (verified 2026-10-11, the budget test,
  debug build): 16 for the node, 8 for the size, the name, and the folders' share. The same tree filled by discovery
  under one opaque source, as a backend that doesn't walk at scan time fills it, is 52.4 B/node (verified 2026-10-13,
  `budget_tests.rs`, debug build): growing vectors over-allocate, and each discovered child costs its parent's
  `extra_children` an id.

### Building

`TreeBuilder` takes nodes in any order. `finish` groups children by parent, sorts each group by `TreeOrder`, numbers
breadth-first, then sums totals in one descending pass (every child's id is above its parent's). `PathTree` (behind
`TreeRecorder` and `from_local_scan`) maps paths to nodes: the root is the sources' longest common ancestor, each source
is a top-level node labelled by its path relative to it (`photos/2024/IMG_1.jpg`, no synthesized folder rows), and a
path whose folder hasn't arrived yet creates the missing folders on the way up to a known one. Only folders are in its
path map, so it stays small for a million-file scan. A source the scan never reported stays a 0-byte file row.

### Order

`TreeOrder` (`order.rs`): folders first, then files, each by the panes' collator (`crates/cmdr-collation/src/lib.rs`,
handed over by the host's `NameOrder` once per walk; `TreeOrder::active`), with the collator's raw-bytes tiebreak so the
order is total. `sort` builds one key per item; `compare` is the one-off form. `TreeOrder::bytewise` (test builds only)
keeps the shape without the collator, for the budget cells, where building a million keys in an unoptimized build is the
whole test budget.

The drivers start files in this order too (`../transfer/DETAILS.md` § "Processing order"), through three helpers:
`TreeModel::tree_order_ranks` (each node's place in a depth-first walk, for the local engine to sort its flat file
list), `sort_sources` (the selection, ranked by the same labels its top-level rows show), and `TreeOrder::sort` itself
for a walker's level.

## Mutations and folder state

One call per file boundary: `begin` (with the driver's live byte counter), `complete`, `withdraw` (stopped without
landing: back to waiting), `skip`, `fail` (keeps the typed `WriteOperationError`; a folder can fail too, and counts
itself), `folder_created` (only an EMPTY folder reads it), `discover`. A second report on a settled file is ignored, so
nothing counts twice. Whole-subtree calls, one per subtree: `complete_subtree` and `skip_subtree` settle every file
below (plus what an opaque folder's scan counted and its walk never named), and `empty_folders_created` finishes every
empty folder at once.

Folder state is derived at read time, never stored: failed if any file below failed; else, once every file is settled
and nothing below is in flight, skipped if all were skipped, done otherwise; else active if anything below is in flight
or landed; else waiting. An empty folder is done once created. "Nothing in flight" matters for an opaque folder whose
scan counted fewer files than its walk finds: its counts read complete while the newcomer is still landing.

## The view (`view.rs`)

`flatten(model, request)` answers one window of at most 200 rows:

1. **Effective expansion**: the folders holding the focus files (the active ones, or between two files the last one
   started, so a folder doesn't flicker shut in the gap), plus the user's `expanded`, minus their `collapsed` (each
   capped at 2,000). Ids are mapped to row keys first.
2. **Open rows**: expanded rows whose every ancestor row is expanded. Each gets a listing size (its children, plus every
   open child's size), computed deepest first.
3. **Locate** the window's first row by descending through the open children's sizes, then **walk forward** in tree
   order with a stack of `(parent row, position)`.

Rows are keyed by the deepest folder of a compact chain (`row_key`), labelled with the chain's names joined by `/`. A
row's `pos_in_set` / `set_size` come from its parent's child count, for `aria-posinset` / `aria-setsize`. A folder's
`bytes_done` adds the in-flight bytes of every active file below it (`in_flight_by_folder`, one pass over the active set
per request).

`Follow { lead }` puts the focus row `lead` rows from the top, clamped so the window fills; `At { offset }` clamps to
the row count. `focus_row` is the topmost focus file's row, or the shut folder hiding it.

`ancestors` holds the open rows the descent to the window's first row passed through (each a whole row, plus its flat
index), outermost first. A row's parent is either an earlier row in the window or one of these, so a view has every
row's full relative path (its tooltip), its parent's index (the Left key), and the live counts of the folders it pins
over its scrolled-past rows (the sticky headers), without a second request. At most one row per depth level.

## The view command

`get_transfer_tree_view(operation_id, request)` (`../tree_view.rs`, its own leaf because it looks the operation up in
the manager and the state registry, which sit above this module) answers:

- `Err(UnknownOperation)`: no operation by that id (never existed, or settled and its kept tree was released or
  expired).
- `Err(NotATransfer)`: an operation type that never builds a tree (delete, trash, rename, an edit inside an existing
  archive).
- `Ok` with `ready: false`: queued, scanning, or a tree a bug turned off. Every other field is empty.
- `Ok` with `ready: true`: the window. `axis` is the bottleneck the operation's last `write-progress` carried
  (`../eta.rs`), `Bytes` before there was one, so every folder bar fills along the same axis the header's ETA follows.

No event of its own: a view calls it on mount and on each `write-progress` tick, which keeps the backend stateless per
view.

**A settled operation's tree outlives it, until its dialog lets go** (`../finished_trees.rs`). The manager hands each
tree over as the op settles, BEFORE it drops the record, and the command asks the kept trees first, so a view never
falls between the two. `release_transfer_tree` (the progress dialog's teardown) drops it, and a 10-minute TTL catches a
release that never came. **Why**: a view pulls at most every 200 ms, so a transfer faster than that (a server-side copy
of 36 files, #702) settled before the pull that would show its end, and the done dialog kept rows that said every file
was still waiting. The dialog pulls once more on `write-settled` to fetch them. Every finished tree is kept, viewed or
not: whether a view ever asked can't decide it, because the fastest transfers settle before their first pull. The
command is `async` so it never answers on the main thread: a window is microseconds, but it takes the tree's lock, which
a subtree-at-once report holds for as long as the subtree is big.

The other reader is `cmdr://state`: each running or paused operation row asks for
`TransferTreeViewRequest::following_the_work()` (no toggles, `Follow { lead: 0 }`, `MAX_LIMIT` rows) and renders its
active files grouped by folder (`tools/resources/DETAILS.md`, the `activeFolders:` paragraph).

## How drivers report (`report.rs`)

A driver holds a `TreeReporter` (`TransferTreeSlot::reporter`), cloned freely, silent when the operation has no tree.

- **A file's row is a `TreeLeaf`**, opened with `reporter.leaf(node)` (active), settled by `complete` / `skip`, and put
  back to waiting by `Drop` when it was neither (a failure, a cancel, a retry that gave up). Its byte counter is an
  `Arc<AtomicU64>` the driver advances per chunk with a plain store; the view reads it.
- **The volume engines' `LeafProgress` owns its `TreeLeaf`** (`../transfer/transfer_driver/progress.rs`), and the leaf's
  high-water mark IS the row's byte counter, so the bar and the row can't disagree. `SourceProgress` carries the
  top-level source's row (`for_source` looks it up by path); a top-level file's leaf is that row.
- **The merge walker resolves rows per level** (`../transfer/volume/merge.rs`): one `TreeLevel` per listed folder,
  children by name, built once (one lock) from the level's listing and dropped with the level. Every child the scan
  never saw (an opaque folder's contents, a file that appeared since) joins the tree as the level is built (`discover`),
  before any child lands: discovered one by one as the walker reached them, a newcomer last in tree order let its folder
  read done before it was even found. An opaque folder's totals already count its contents, so `discover` adds a found
  file's totals only up to the nearest opaque ancestor. A name whose kind changed since the scan gets no row. The walker
  asks by the scan's kind (`merges_as_a_directory`): a link the scan counted as one file, which a stat-following
  backend's walk recurses through, keeps its file row, and that level completes it. A level that finished marks its
  folder in place (only an empty one reads it). The sequential extractor's plan pass records each member's row for its
  data pass (`PlannedWrite::row`).
- **The local engine** opens a `TreeLeaf` per `FileInfo::tree_node` around `copy_single_item`, which settles it with its
  verdict in the per-file milestone (before that event goes out, so a view pulled on it already shows the row) and
  advances its bytes from the chunked-copy callback. The scanned-dirs pass finishes every empty folder at once. A file
  without a node (none today) is a no-op.
- **Subtree-at-once paths**: the async driver's bulk pre-skip and top-level Skip, the concurrent driver's pre-skip and
  Skip, and the local copy's bulk pre-skip call `skip_whole` on the source's row; a rename move's landed item calls
  `complete_whole`; a merge child that is a FOLDER answered Skip goes through `SourceProgress::skip_leaf`, which skips
  its whole subtree.
- **Cross-FS and cross-volume moves**: the tree tracks the COPY phase; the source sweep after it is a phase of the
  header, and the rows stay as the copy left them.
- **Compress** (`../archive_edit/fresh_rows.rs::CompressRows`): the producer writes plan entries one at a time in plan
  order, and every progress tick says how many it finished, so the entry in flight is the `entries_done`th. The observer
  derives rows from the ticks alone; `produce` speaks no tree vocabulary. One active row at a time. A remote
  destination's upload (step 2 of 2) leaves the rows done.
- **Failures**: the two gateways every transfer's `write-error` goes through, `transfer_sides::transfer_stop_event`
  (local) and `volume/transfer_error.rs::write_error_event_from` (volume), call `TransferTreeSlot::note_failure`, which
  marks the row at the error's typed `item_path` (an `OriginalsKeptAside`'s cause's), found by walking down from its
  source by name. A destination-side failure (a full disk, a folder that won't take a write) names where the file was
  landing, so the tree also knows where its sources land (`TreeLanding`, handed over with the tree at `install`): the
  destination folder (plus a cross-FS move's staging folder, which its copy phase writes into first), where each source
  lands under its own name, or under the name the user gave a single source's copy. The first component past the landing
  folder picks the source, the rest walks down. A path the tree never held marks nothing: a landing renamed on a clash,
  the destination folder itself, a name two sources share. A local staged write names its target, never the
  `.cmdr-tmp-*` temp (`overwrite::stage_and_land_file` via `WriteOperationError::on_item_instead`), which the user never
  saw either. The row shows while the error is on screen; keeping it past settle is #261's. A compress lands in one
  archive (`TreeLanding::nowhere`), so a compress failure marks no row.

### Observation only, structurally

A tree bug must never fail, slow, or alter a transfer:

- Every report returns `()` (or a row id that only feeds later reports), so no driver can branch on the tree.
- Every report is a no-op without a tree or a row: `TreeReporter::none`, a `None` node, a kind mismatch.
- `TransferTree::guarded` wraps every read and write in `catch_unwind`: a panic (a bad id indexing the arena, say) is
  logged, still reaches the crash reporter's hook as the bug it is, and turns the tree off for the rest of the
  operation; the driver that reported never sees it. Views then answer `ready: false`. Pinned by
  `report_tests.rs::a_report_that_panics_turns_the_tree_off_and_the_driver_carries_on`.
- Building is guarded the same way, by `built` in `mod.rs`: every public builder answers `Option`, a `TreeRecorder` that
  panics on an entry goes quiet (the scan walks on) and finishes to `None`, `sort_sources` keeps the given order, and
  `ScanResult::order_by_transfer_tree` leaves the local file list in walk order with no nodes (it reorders nothing until
  every rank is known). `TransferTreeSlot::install(None)` installs nothing, so views answer `ready: false` and every
  report is a no-op: the transfer runs exactly as it would with no tree. Test builds inject the bug by name
  (`test_support::TREE_BUG_NAME`, which `TreeBuilder::child` panics on); the local copy, the volume copy, the compress,
  and the preview each pin it (`*tree_that_fails_to_build*`).
- One uncontended mutex per file boundary, never per chunk (below).

### What a report costs

One file's `begin` + `complete` (an `Arc` allocation, two lock acquisitions, an O(depth) walk up): 88 ns median, 35 ns
best, against 251 µs per file for the whole local copy (2,000 × 4 KB files, 501 ms median), so 0.04% of the
many-small-files case where per-file overhead is all there is (verified 2026-10-12, release build,
`report_tests.rs::tree_boundary_cost_bench` and `../transfer/copy/copy_tests.rs::local_copy_bench_many_small_files`, on
a machine under load average ~190, so the copy figure is noisy; the earlier baseline in `../transfer/DETAILS.md` §
"Local copies stage" read 394–400 ms on a quieter one).
