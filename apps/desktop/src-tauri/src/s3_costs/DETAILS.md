# S3 cost estimates: details

Must-knows: `CLAUDE.md`. The estimator, the table's schema, and the math: `crates/cmdr-s3/DETAILS.md` § "Cost
estimates". The product decision: `docs/specs/s3-support-plan.md` § "Product decisions".

## The flow

1. A setup dialog's scan preview settles. The S3 backend's walk kept every object's size and `LastModified`
   (`ScanSource::keeps_files`), which ride on `BatchScanResult::files` into the scan cache
   (`CachedScanResult::keeping_files`). A local walk's own per-file list gives sizes.
2. The dialog calls `estimate_operation_cost` with the preview id, the operation, and both volume ids.
3. `estimate` downcasts each end to `S3Volume`; neither is S3 means no estimate, before the cache is touched.
4. `plan` builds the workloads, `price_source::current` hands over the table, and `Estimate::of` prices each.
   The line items go to the debug log (`RUST_LOG=cmdr_lib::s3_costs=debug`); the dialog gets one amount per provider.

## What each operation plans

The planner counts every request the engine sends, LISTs and HEADs included, one `Workload` method per engine step
(`crates/cmdr-s3/DETAILS.md` § "Cost estimates"). It needs the shape of the selection, not only its totals, so
`ScanCostFacts` carries `selected_folders` and `selected_file_sizes` beside the files: each selected item is stat'd
and its name probed at the destination, and only what lands inside a folder the operation makes skips the
no-overwrite check. Pinned request for request by `backend_suites/s3_engine_integration_test.rs::
the_engine_sends_what_the_estimate_counts` on both fixtures.

- **The source side** (copy or move off S3): `stat_selection` per selected item, and `list_folder` twice per listing
  page (the scan reads each folder, the walk reads it again).
- **Copy**: same account and buckets the provider copies between → on the one account, `copy_on_server_fresh` per file
  inside a selected folder and `copy_on_server` per selected file. Otherwise a `download` per file on an S3 source,
  and `upload_fresh` / `upload` the same way on an S3 destination. Either way the destination side is
  `open_destination`, a `probe_name` per selected item, and a `make_folder` per folder.
- **Overwrites** (a copy's or a move's, `CostEstimateRequest.clashes`): the dialog sends the conflict check's file
  clashes and its policy; `plan::overwritten` decides which the policy overwrites the way the transfer does (strictly
  smaller, strictly older), and each one is `replace_object` at the destination plus, for an upload, `upload_over`. A
  server-side copy replaces in one request, so only `replace_object`.
- **Move**: the copy, plus `check_move_within` for a move within one account, then the source sweep: `sweep_folder`
  per folder (one `DeleteObjects` per level) with a `swept_object` per file inside, and a `delete_object` per selected
  file. This is also F2's rename by move: the prefilled Move dialog runs the same scan. Before that, `estimate_rename`
  prices the rename editor's own tally (`Volume::tally_subtree`, at most 101 files, the renamed entry as the one
  selected item) as a same-account move, and any amount that doesn't round to zero (`rounds_to_zero`, the cost line's
  half-a-cent rule) sends the rename to the Move dialog.
- **Delete**: `stat_selection` per selected item, a `list_folder` per listing page, a `delete_object` per file
  (batched a thousand to a `DeleteObjects`, the way the volume delete sends them), and a `delete_folder` per folder.
- **Approximate, on purpose**: listing pages are one per folder plus one per thousand files; the destination exists
  and no selected folder clashes with one there; source folders carry markers.
- **No per-file list** (a scan answered from a cached listing): the bytes spread evenly over the file count, undated.
  Part counts drift a little; early deletion can't be priced, so it isn't.

## Price table refresh

**Decision**: one fetch per install per day at most, in the background, cached as the server's JSON verbatim in the
app data dir (`s3-prices.json`, temp file then rename). **Why**: prices move a few times a year, and the estimate must
never wait on the network. A refused table (a newer schema, a validation failure) or a failed fetch logs a warning and
keeps the table in hand. `LAST_ATTEMPT` keeps a failing server from being asked once per dialog.

**Decision**: dev, E2E, CI, and capture runs (`prod_instance::NON_PROD_ENV_VARS`) never fetch and price from the
bundled copy. **Why**: tests mustn't depend on the network, and every worktree's dev build would otherwise ask the
production server. The fetch itself is tested against a mock server (`price_source_tests.rs`).

The fetch rides `server_request::send(Egress::S3PriceList, …)`. No managed-policy key turns it off (it's on the
"traffic no key turns off" list in `../managed_policy/DETAILS.md`): it carries no user data, only fetches a public table.
There's no "stay offline" setting in the app today; if one lands, `fetching_allowed` is where it goes.

## Known gaps

- **Only the overwrites the dialog's conflict check saw are priced**: its one destination listing finds clashes at the
  top level, so a file inside a folder that merges isn't known until the operation writes it. Stop asks per clash, so
  no overwrite is assumed under it, and a skipped clash's copy is still counted.
- AWS prices are US East's for every region.
