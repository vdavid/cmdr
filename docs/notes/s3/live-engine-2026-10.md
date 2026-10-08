# S3 transfer-engine flows on live providers, 2026-10-02

The app's transfer engine (copy, move, rename, delete, cancel, pause, rollback, conflicts) driven against the seven real
provider accounts through the same entry points the IPC commands call (`copy_between_volumes`, `start_rename_by_move`,
`start_renames`, `delete_files_start`, `cancel_write_operation`, `rollback_operation` + `execute_rollback`). The
distilled, current facts live in `crates/cmdr-s3/DETAILS.md`; this note keeps the evidence.

## Setup

- **One body, two servers.** Every S3 engine scenario takes a `cmdr_s3::volume::testing::S3Target` (a Docker fixture or
  a live account from `testing::live`). The Docker cells (`s3_integration_*` in `backend_suites/s3_transfer_*`,
  `s3_rename_integration_test.rs`, `s3_engine_integration_test.rs`) and the live cells (`s3_live_engine_test.rs`, run by
  `apps/desktop/test/s3-servers/live-engine.sh`) share them. S3 now also drives the shared semantics, safety, and
  move-drift scenarios, which only SFTP, SMB, and WebDAV ran before: 56 new fixture cells, all green on VersityGW and
  Garage.
- **Accounts**: the `live-env.sh` set (R2, Hetzner `nbg1`, GCS, Spaces `fra1`, AWS `eu-north-1`, B2 `eu-central-003`,
  Wasabi `eu-central-1`). Second buckets exist on AWS, B2, Wasabi, and Hetzner only. Two sibling agents ran live suites
  on the same accounts and uplink at the same time, so the timings are for orientation.
- **Cost**: per provider about 450 MB uploaded (the 140 MiB multipart copy, a 65 MiB download source, a 70 MiB cost-cell
  upload and its same-bucket copy, a 50 MiB cross-provider tree) and ~10,000 small requests, most of them from the
  1,005-object cells. AWS egress ~~200 MB (~~$0.02), Wasabi's 90-day minimum on ~0.5 GB well under a cent, Glacier and
  Deep Archive minimums on two 6-byte objects negligible. Every flow removed its own `cmdr-live/<run>/` prefix; one run
  killed by a timeout stranded 391 objects on GCS, removed by hand afterwards.

## Outcomes, flow by provider

"ok" means every assertion held, bytes checked by SHA-256 at both ends. Times are wall clock including seeding and
cleanup.

- **Byte path** (copy off, 65 MiB off, seeded tree off, onto with one PUT and a 140 MiB multipart carrying the mtime,
  tree onto, written tree off, cancel mid-upload, an Overwrite answer, a Skip answer in a pre-existing folder, awkward
  names both ways): ok on all seven. GCS was 0 of 10 before `47a2bd97f` (below); after it, 10 of 10, with one transient
  seeding failure in an earlier run.
- **Between buckets** (copy keeps the date, a cross-bucket copy runs on the server, a bucket-bound copy streams): ok on
  AWS, B2, Wasabi, and Hetzner. Skipped on R2, GCS, and Spaces: no second bucket. Hetzner copies across buckets on the
  server; the rename suite's comment naming it as the bucket-bound provider was wrong (Spaces is) and is fixed.
- **Renames through the engine** (a folder of 1,005 objects, a 17 MiB file by multipart copy, pause then cancel, pause
  then resume, a reviewed batch with a folder):
  - AWS, Spaces, Wasabi, GCS: ok.
  - R2: ok; the 1,005-object rename takes 168 s, past the suite's old 120 s wait (now stretched on live runs).
  - Hetzner: ok in five of six runs; once the 1,005-object rename failed with `DestinationExists` on a fresh destination
    key (`renamed/f0053.txt`). Diagnosed and fixed: § "The lead's decisions", item 4.
  - B2 (2026-10-03, within the caps): 5 of 5 on the second run. The first lost the 1,005-object flow to B2 answering one
    seeding PUT `500 InternalError`; the seeder now sends a faulted PUT again (`5d72f86f1`). The 1,005-object rename
    took 383 s, the slowest of any provider. Measured per run: 1,060 HEAD and 3 GET (Class B), 81 LIST plus 22 other
    Class C, ~49 MiB down.
- **Merges and moves** (merge under Skip, Overwrite, Rename (keep both), OverwriteSmaller; a move-merge onto that spares
  what it skipped; a folder moved onto and off; a file saved over or added mid-move off; same-bucket move-merge, folder
  move, and tree copy; a missing nested destination; a 6 MiB odd-length file; 40 files at full concurrency): 15 of 15 on
  R2, AWS, Spaces, Wasabi, Hetzner, and GCS (GCS's first run lost the network mid-cell, open question 3; the rerun was
  clean). B2 (2026-10-03): 14 of 15, about 50 s a flow; "same-bucket folder move" failed when B2 answered a scenario's
  seeding write (a one-PUT upload through the volume) `500 InternalError`, the single-PUT gap fixed in `551303ee4`, and
  passed rerun alone (`CMDR_S3_LIVE_FLOWS`). Measured: 512 HEAD and 144 GET, 753 LIST plus 60 other Class C, ~30 MiB
  down; the one-flow rerun 116 HEAD and 48 GET.
- **Safety, cancel, rollback, delete** (a failed merge copy or move onto the user's folder, a delete bound to a
  local-shaped preview, a recursive delete that takes exactly the selection, an unknown source type, cancel
  mid-download, cancel between multipart parts, a cut-off Overwrite keeps the original, pause and resume between parts,
  roll back a finished copy, cancel with rollback mid-tree, delete a folder of 1,005 objects):
  - R2, AWS, Spaces, Hetzner: 12 of 12.
  - Wasabi: 12 of 12 on the rerun; the first run's 1,005-object delete stopped at `f0530.txt` with `DeviceDisconnected`
    (one transport blip ended the whole delete; now retried, § "The lead's decisions", item 1).
  - GCS: 11 of 11; the 1,005-object delete didn't finish inside the remaining seven minutes (614 objects gone).
  - B2 (2026-10-03): 5 of 5 run, chosen for the budget: cancel between multipart parts, a cut-off Overwrite, pause and
    resume between parts (the new pause path, `9eacb516a`), roll back a finished copy, and delete a folder of 1,005
    objects (246 s, nearly all of it seeding). Measured: 40 HEAD and 5 GET, 60 LIST plus 24 other Class C. Not run on
    B2, for the Class B budget: the failed merge copy and move, the two delete-scope flows, the unknown source type,
    cancel mid-download, and cancel with rollback mid-tree.
  - After every cancel: no object at the name, no unfinished upload on the server, no open record in the upload ledger.
    After every rollback: nothing left but the destination folder the copy itself made.
- **A name taken mid-upload** (informational, the documented blind window): another writer's file survived on R2, AWS,
  GCS, Spaces, and Hetzner. On Wasabi our upload replaced it silently with the copy reporting success: the
  check-then-write window `crates/cmdr-s3/DETAILS.md` § "No-overwrite writes" accepts. Whether Wasabi honours
  `If-None-Match` on PUT (which would close it) is for the profile owner to verify.
- **Between providers** (a 50 MiB file plus a nested tree, streamed): ok for R2 → GCS, GCS → Spaces, Spaces → AWS, AWS →
  Wasabi, Wasabi → R2, Hetzner → AWS, and AWS → Hetzner. B2: not run on 2026-10-03, dropped for the Class B budget (the
  stream through the Mac is the same upload and download path B2's byte-path flows cover).
- **Archived objects** (AWS, `GLACIER` and `DEEP_ARCHIVE`, uploaded with the storage class): ok. The listing marks each
  `in_cold_storage`, a read answers `VolumeError::ColdStorage`, and a copy stops at once with
  `WriteOperationError::SourceInColdStorage`, nothing landing locally.
- **Requests against the estimate**: below.

## Bugs found and fixed

1. **Every GCS request failed in the app** (`47a2bd97f`). The app's `reqwest` speaks HTTP/2 (`genai` turns the feature
   on; `cmdr-s3` alone built HTTP/1.1, which is why the crate's own live cells never saw it), and `sigv4::sign` added an
   explicit `host` header beside HTTP/2's `:authority`. Google's front end resets such a stream with `PROTOCOL_ERROR`,
   so every connect answered `Unreachable`. Isolated with a plain `reqwest` probe in the same binary: a GET to
   `storage.googleapis.com` passed, the same GET with a `host` header failed the same way, and Cloudflare tolerated it
   (which is why R2 never showed it; R2 also offers HTTP/1.1 only). Fix: the host is signed as the request URL spells it
   and travels in the URL alone. Red first: `sigv4_test.rs::the_signed_request_carries_no_explicit_host_header` and
   `the_host_is_signed_as_the_url_spells_it`. The crate now declares `http2` itself (a sibling's `69b2330b5`).
2. **The estimate missed a HEAD on multipart server-side copies off the pin allowlist** (`6e9ad525e`, reported by
   live-providers from the tally). Hetzner, Spaces, Wasabi, and "Other" HEAD the source once more before completing
   (`source_unchanged`); `Workload::copy_on_server` now counts it. Red first:
   `cost_test.rs:: a_multipart_copy_off_the_pin_allowlist_heads_its_source_again`.
3. **Test infrastructure**: the gated source served the same bytes for every path (now per file, `gated_files`); live
   seeding at 32 concurrent PUTs drew `503 SlowDown` from Hetzner (now eight on a live account, a 503 sent again).

## The lead's decisions, carried out

Measured with the request tally (`testing::take_sent_requests`, `RUST_LOG=s3_sent=trace` for the sequence) on VersityGW,
whose "Other" profile is check-then-write everywhere, then rerun live on the six uncapped providers.

1. **The volume delete batches** (`80905b4b3`). New capability `Volume::delete_batch_size` (S3: 1,000); the walker sends
   the scanned files through `delete_files` in chunks, pausing and checking Cancel between chunks; `cmdr-s3` sends a
   throttled, faulted, or cut-off `DeleteObjects` again after 1 s and 2 s. A 1,005-object folder: 3,021 requests (1,009
   LIST, 1,007 HEAD, 1,005 DELETE) → 8 (the top-level probe, two listing pages, two `DeleteObjects`, the folder's own
   removal). Live, seeding included: AWS 74 s → 6 s, Spaces 119 → 15 s, Hetzner 139 → 9 s, Wasabi 155 → 29 s, R2 343 →
   47 s, GCS about 11 minutes → 35 s.
2. **The needless HEADs are gone where the folder's creation already proved what they asked** (`375a0f853`). Where each
   remaining HEAD and LIST comes from, per operation (folder `F` of `n` files with subfolders, into an existing
   destination `D`; check-then-write profile):
   - The scan: a HEAD per selected item, plus a capped LIST for a folder (`get_metadata`: the HEAD answers nothing, the
     LIST finds the folder); one full LIST per folder page.
   - The walk: one full LIST per source folder page again (the merge walker re-lists each level it copies; the scan's
     listing isn't handed down). Redundant, kept: removing it means threading the scan's listings into the walker, an
     engine refactor with no data-safety stake. Candidate for later.
   - Readying `D`: two capped LISTs (`create_directory_all` finding it) and one full LIST (the stale-temp reap). A move
     or rename within the bucket adds three capped LISTs (its source and destination checks).
   - Each selected name at `D`: a HEAD and a capped LIST (free-name probe).
   - Each folder made (`create_directory`): a capped LIST and a HEAD proving the name free, a capped LIST proving the
     parent, the marker PUT, then a HEAD and a capped LIST.
   - Each file: copy = the source HEAD + `CopyObject` + the verifying HEAD; upload = PUT + the verifying HEAD. The
     per-object no-overwrite HEAD now goes only where it protects data: a write into a folder that already existed (a
     merge, or files selected straight into `D`). Inside a folder this operation made, the typed fact
     `WriteMode::CreateNewInFreshFolder` (handed down by the merge walker only for a level its own `create_directory`
     made) skips it, and a one-request copy there skips its verifying HEAD too (item 5); the accepted window is in
     `crates/cmdr-s3/DETAILS.md` § "No-overwrite writes".
   - A move's sweep, per folder: a HEAD and a capped LIST (its kind), a full LIST, one `DeleteObjects`, a capped LIST
     and the marker's delete.
   - Kept for data safety: the per-object no-overwrite HEAD in a merge on check-then-write providers (Hetzner, Spaces,
     Wasabi, B2, GCS for completions, "Other"), every verifying HEAD outside a fresh folder, the folder-creation checks,
     and the sweep's kind check.
   - (b) stays undone: the source HEAD of a one-request copy is what restates its date and headers (`REPLACE`), and a
     listing carries neither. Item 5 has the `COPY` attempt and why it was reverted.
3. **The estimate is exact** (`5f0e99c14`). `Workload` gained one method per engine step above; `s3_costs/plan.rs`
   composes them from the selection's shape (`ScanCostFacts::selected_folders` / `selected_file_sizes`). Ten operations
   (upload, download, same-bucket copy, move within, rename, move off, delete, and two selected files uploaded, copied,
   and deleted) match request kind for request kind on both fixtures (`the_engine_sends_what_the_estimate_counts`) and
   live on R2, GCS, Spaces, AWS, Wasabi, and Hetzner (`s3_live_engine_sends_what_the_estimate_counts`). Also fixed on
   the way (`6e9ad525e`): a multipart copy off the pin allowlist sends one more source HEAD, now counted. Approximate on
   purpose, stated in `plan.rs`: listing pages (one per folder plus one per thousand files), one sweep batch per folder
   level, an existing destination with no folder clash, source folders with markers.
4. **Hetzner's `DestinationExists`: confirmed and fixed** (`5182dc1ac`). Reproduced over `fake_s3.rs`: a `CopyObject`
   the server applied while its answer was lost failed as `DeviceDisconnected`; `try_server_side_copy` fell back to
   streaming, and the streamed write's no-overwrite check refused the name the copy itself had taken. Every one-request
   copy goes as `REPLACE` with the source's restated metadata beside its own write token, and a lost answer or a server
   fault asks `landed_whole` (one HEAD, never a delete).
5. **The HEAD saved is the fresh-folder verify** (`9a5c9ff0e`). A first try dropped the source HEAD instead (`COPY` with
   the listing's size and ETag, a pin, and a proof by the result's ETag; `4e0793d65`, `513572868`), reverted in
   `3eb26ea2f` and `d75a0c80c`: under `COPY` a source without its own `x-amz-meta-mtime` (most data other tools
   uploaded) takes the copy's time as its date, and `a_source_without_an_mtime_keeps_its_date` guarantees it doesn't.
   What landed instead: under `CreateNewInFreshFolder` a one-request copy skips its verifying HEAD, since the folder's
   creation already covers the one thing it detects (another writer at a name in a folder that didn't exist moments
   ago), inside the same accepted window. A lost answer still HEADs, only then. B2's 1,005-object rename to a new
   folder: about 1,005 HEADs (the sources') plus LISTs, from 2,016; the fixture cell bounds it at 1,025. What the try
   measured, kept as `live_copy_object_etags`:
   - a single-part source's copy keeps its ETag on all six; a multipart source's keeps it on Hetzner and Spaces and gets
     a fresh one on AWS, R2, and Wasabi, so an ETag proof of a lost answer would fail there (the token proof doesn't);
   - a wrong `x-amz-copy-source-if-match` on `CopyObject` is `412` on AWS, R2, GCS, and Hetzner, ignored on Spaces and
     Wasabi; ❗ GCS answers `400 InvalidArgument` to a pin naming a multipart ETag, and a single copy now pins its
     HEAD's ETag (`ef2fbee05`, `412` → `SourceChanged`) with GCS's multipart-ETag sources unpinned
     (`refuses_multipart_copy_pin`). A create-only GCS copy carries the pin as `x-goog-copy-source-if-match`: right 200,
     wrong 412, multipart 200 (`live_create_only_copy_with_a_source_pin`). Hetzner and Spaces weren't run: their test
     buckets were gone.
6. **The walker's second listing**: not a contained refactor (the scan keeps no names; handing listings down crosses
   `cmdr-fs`'s walk, the scan cache, and the merge walker, and the walker's own listing is what sees a file added after
   the scan). Filed as issue #356 with the options.

Live reruns after item 5, on R2, GCS, Spaces, AWS, Wasabi, and Hetzner: renames 30 of 30, safety all ok (two Hetzner
cells timed out connecting once and passed on rerun), merges and moves 90 of 90, cross-bucket 9 of 9 (where a second
bucket exists), requests against the estimate 6 of 6. B2 was rerun separately on 2026-10-03 (§ "Outcomes").

## Still open for the lead

1. **The walker re-lists every source folder the scan already listed** (one full LIST per folder per copy or move):
   issue #356 (item 6).
2. **Small-object throughput.** The 1,005-object rename now: AWS 30 s, Spaces 45 s, Hetzner 48 s, Wasabi 74 s, GCS 189
   s, R2 202 s. Per-request latency dominates; how many objects a folder rename copies at once is the lever.
3. **IPv6 without a route.** On this network (ULA addresses only) GCS twice failed mid-run with "Network is unreachable
   (os error 51)", every later connect in that process too, while `curl` falls back to IPv4. Worth checking how
   `hyper-util`'s happy eyeballs handles an address list that fails instantly.
4. **Done (`ce80865de`): the fixture lane queues.** All 191 S3 fixture cells (94 app, 97 crate) run four at a time on
   nextest's `s3-fixture` group with a 30 s cap; at full parallelism the whole lane went from every cell killed at 8 s
   to 191 of 191 green in 174 s. If that wall time matters more than headroom, `max-threads` is the knob.
5. **Second buckets** on R2, GCS, and Spaces would let the cross-bucket flows run there too.
6. **B2's daily Class B cap**: rerun on 2026-10-03 within the caps (renames, merges and moves, and five safety flows;
   above), the totals in `live-verification-2026-10.md` § "Outcome: B2 rerun within its free caps". Once the cap was
   used up (by the 1,005-object cells' HEADs), every B2 GET answered `403` with `<Code>AccessDenied</Code>` ("Cannot
   download file, download bandwidth or transaction (Class B) cap exceeded", read with the AWS CLI) and every HEAD a
   bodyless `403`; LIST, PUT, and `DeleteObjects` kept working. Cmdr maps both to `VolumeError::PermissionDenied` (a
   stat, a read, a delete's fallback HEAD); in a transfer that's `WriteOperationError::PermissionDenied` with
   `refusal: Unclassified` and `side: Source`, which the dialog words as a permission problem, now adding that it may be
   a usage cap (a sibling's `16ee8a3b8`). With items 1, 2, and 5 the 1,005-object delete sends 2 HEADs instead of 1,007
   and the rename about 1,005 instead of 3,021.
7. **A name taken mid-upload on Wasabi** is overwritten silently (the documented check-then-write window); whether
   Wasabi honours `If-None-Match` on PUT is the profile owner's to verify.
