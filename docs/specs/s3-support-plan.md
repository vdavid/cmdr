# S3 support: any S3-compatible bucket as a volume

Tracks [#119](https://github.com/vdavid/cmdr/issues/119). Why: people keep files in AWS S3, Cloudflare R2, Backblaze B2,
Wasabi, and Hetzner Object Storage, and today Cmdr can't reach any of them. This adds `crates/cmdr-s3`, one backend for
every S3-compatible service, on the rails SFTP and WebDAV laid down (account → place → pin, the sign-in sheet, the
reconnect cycle, the transfer engine, the conformance cells).

Evidence this plan stands on, read before touching the matching milestone:

- `docs/notes/s3/provider-research.md`: per-provider capabilities, limits, quirks, and list prices, each sourced.
- `docs/notes/s3/library-and-fixture-audit.md`: why we write our own signer, and why VersityGW + Garage are the
  fixtures.

## Product decisions (David, 2026-10-01)

- **No OAuth.** Every provider authenticates with an access key ID + secret, signed SigV4. `~/.aws` profiles and SSO are
  out of scope for now.
- **The volume is the account; buckets are its places.** The account root lists buckets when the key may
  (`ListBuckets`); a bucket-scoped key (R2 non-admin tokens, B2 keys without `listAllBucketNames`, Hetzner outside the
  key's project) can't, so the user can always type a bucket name, and a typed bucket becomes a saved place like an SFTP
  path. This is `apps/desktop/src/lib/servers/DETAILS.md` § "The model" with bucket = place.
- **Connect form**: presets for AWS (+ region), R2 (+ account ID), B2 (+ region), Wasabi (+ region), and Hetzner (+
  location), plus "Other S3-compatible" with a raw endpoint URL and a path-style toggle.
- **Folders are prefixes.** Creating an empty folder writes a zero-byte `name/` marker object (the AWS console's and
  rclone's convention). A folder with no marker vanishes when its last object goes; that's S3, and fine.
- **Modified dates**: S3 only stores upload time. We write the source's mtime into `x-amz-meta-mtime` (rclone's key and
  format, so the two tools agree) and show it when present, else `LastModified`. ❗ A listing (`ListObjectsV2`) carries
  no user metadata, so a folder's rows show `LastModified` and only a stat (one HEAD: Get info, a single entry) shows
  the mtime; reading it per row would cost one billed request per file.
- **No indexing and no thumbnails on S3 volumes** for now: every request costs money.
- **Space**: `get_space_info` answers `VolumeError::NotSupported`.
- **Delete is permanent** (S3 has no trash), and the delete dialog says so. When the bucket keeps versions (AWS/R2
  versioning on, and B2 by default), it says the bytes stay billed.
- **Cost estimates** before costly operations, at list prices: "About
  $0.02 at AWS list prices", with an (i) explaining
  free tiers, discounts, and minimums. Hidden when it rounds to $0.00.
  ❌ None on rollbacks: those just run. The price table is served by `apps/api-server` so a price change needs no
  release.
- **No-overwrite races**: where a provider lacks `If-None-Match: *`, we check then write, and when a clash is noticed
  afterwards we tell the user plainly what happened and what survived.
- **Overwrites never risk the original (decided 2026-10-01, the safer option)**: a server that publishes a PUT cut off
  mid-body (VersityGW does) would lose the original to a cancelled in-place overwrite. So "refuses a short body" is a
  provider allowlist (AWS, R2, B2, on evidence; `crates/cmdr-s3/DETAILS.md` § "Providers"), and off it, an overwrite of
  an existing object goes as a multipart upload, one part for a small file, which publishes nothing until its completion
  (a temp key would bill Wasabi 90 days per overwrite). A write to a free name still goes as one PUT.
- **Share link**: "Copy share link" mints a presigned GET URL, seven days by default (the SigV4 maximum), with one hour
  and one day as the other choices. Free and offline: it's a signature, not a request.
- **Archived objects** (AWS Glacier Flexible Retrieval / Deep Archive): an "archived" badge in the listing and a typed
  error with a clear sentence on read. Restore is later.
- **Versioning UI** ("Show versions", restore an old version) is later.
- One tier: nothing is gated.

## Architecture

`crates/cmdr-s3` in the shape of `crates/cmdr-webdav` (read its `CLAUDE.md` first): no `tauri`, `reqwest` confined to a
few modules, typed errors, the operations as the liveness detector, the shared-client instance model.

- **Our own SigV4 and XML layer, no new crates** (`library-and-fixture-audit.md` has the why: `rusty-s3` presigns
  everything and R2 refuses presigned `POST`; `aws-sdk-s3` brings its own HTTP stack and ~238k lines). Header auth with
  `x-amz-content-sha256: UNSIGNED-PAYLOAD` for every call, query auth only for share links. Built on `hmac` + `sha2` +
  `quick-xml`, already in `Cargo.lock`. Fallback if the signer fights us: AWS's standalone `aws-sigv4`.
- ❗ **No checksum headers by default.** Wasabi rejects `CRC64NVME`, and R2/B2 accept only subsets. Integrity comes from
  `Content-MD5` where the API requires it (`DeleteObjects`) and from comparing sizes and ETags after a write.
- **A provider profile**, chosen by the preset and refined by what the server answers: addressing style, region rule,
  conditional-write support per operation (AWS: Put/Complete/Copy; R2: Put only, Copy via
  `cf-copy-destination-if-none-match`; B2: none; Wasabi, Hetzner: probe), cross-bucket server-side copy (Hetzner:
  same-bucket only), equal-size multipart parts (R2 demands it, so we always do it), Unicode normalization (R2 stores
  keys NFC), price table id.
- ❗ **Conditional writes are an allowlist, ❌ never a probe.** A server can IGNORE `If-None-Match: *` and answer 200
  while overwriting: Garage does on Put, Complete, and Copy, and VersityGW does on Copy
  (`apps/desktop/test/s3-servers/README.md`, observed 2026-10-01). So a success proves nothing. Only an operation a
  provider is documented AND verified (M8, real accounts) to enforce takes the conditional path; everything else, "Other
  S3-compatible" included, checks then writes. A `501 NotImplemented` on an allowlisted one downgrades it to
  check-then-write for the session and logs once.
- **Paths**: volume root `/` lists buckets; `/<bucket>/<key...>`. Keys are encoded per segment.

### The rename capability (the cross-cutting change)

Every `Volume::rename` caller assumes one cheap server-side call: F2 (`rename_managed`, an instant op with a 5 s
frontend wait), same-volume move (`transfer/volume/move_same.rs`, "a whole subtree moves with one rename"), bulk rename
(`write_operations/rename/bulk.rs`), Ask Cmdr's rename proposals, and the MCP rename tool. On S3 a folder rename is one
copy and one delete per object, and a big file's is a slow server-side copy.

- **A new `Volume` capability**, `Volume::rename_work(path)` → `RenameWork::{OneCall, CopyThenDelete}`, says whether
  renaming a given entry is one call. Default: yes, with no I/O. S3: no for a folder, and no for an object past the
  multipart-copy threshold, which is the part floor (64 MiB, `copies_whole`). A typed answer, ❌ never inferred from a
  backend kind.
- **Every caller routes on it**: an entry that can't rename in one call goes through the transfer engine as a
  same-volume move with server-side copy, with the scan preview, progress, pause, cancel, conflicts, and journaling. A
  rename's new name rides the move as a per-source target name. Bulk rename and Ask Cmdr route through the same
  background move: a batch with any copying row runs as one move, in the executor's dependency order, conflicts skipped;
  a swap (`a ↔ b`) is left out, since a move onto a folder still there would merge. The operation log's undo routes the
  same way.
- **F2 keeps its inline editor.** On Enter, `check_rename_validity` also reports the routed cost (`by_move`: files and
  bytes), counting with a bounded listing that stops one past 100. Small (≤100 files): `rename_file` starts a background
  move with the progress chip, no dialog. Bigger: open the Move dialog prefilled with the new name, showing the counts
  from its scan. M7 adds the estimate to `by_move` and to the small rule ("and the estimate rounds to $0.00").
- **Order**: per top-level source, copy everything, then delete; the source sweep deletes each folder level's files in
  one `Volume::delete_files` call (`DeleteObjects`, batches of 1,000). The worst state after a crash is duplicates, ❌
  never loss; the operation log offers to undo.

### Server-side copy and multipart

- Copies within one account (one endpoint and key id, any of its places) never touch the Mac: `Volume::copy_on_server`,
  `CopyObject` up to the part floor, `UploadPartCopy` above it (even under 5 GB), so progress advances per part and
  pause lands between parts. A provider that copies within one bucket only (Spaces) streams a cross-bucket copy instead,
  and GCS, which has no `UploadPartCopy`, copies whole in one `CopyObject`. Each part is pinned to the source's ETag; a
  source without `x-amz-meta-mtime` has its `Last-Modified` written as the copy's mtime.
- **Part size**: one size per upload (R2 requires equal parts), at least 64 MiB and at least `size / 10,000`. A tail
  under 5 MiB stays its own last part on every preset (R2 refuses a last part LARGER than the rest), and folds into the
  part before it only on "Other", since Garage refuses an `UploadPartCopy` source that small even as the last part.
  **Concurrency**: 16 parts for server-side copies and four for uploads on every provider measured, AIMD back-off on
  `SlowDown` / 503 / 429 (`crates/cmdr-s3/DETAILS.md` § "Verified providers").
- **Cancel** aborts the multipart upload. **Connect** (and a reconnect) aborts our own unfinished uploads, because
  they're invisible and billed forever. ❗ `ListMultipartUploads` returns no initiation metadata, so nothing on the
  server marks an upload as Cmdr's: each upload's bucket, key, and upload ID are recorded locally when it starts (a file
  the backend keeps in the host's state directory, not the operation log: `crates/cmdr-s3/DETAILS.md` § "Unfinished
  uploads") and cleared when it completes or aborts, and the sweep aborts what's left. ❌ Never abort uploads we didn't
  record: another tool's upload may be live.
- **A copy can fail inside a `200 OK`** on AWS: always parse the body.

## Milestones

Each one ends green on `pnpm check` with its docs updated. Tiers follow
`apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Building a new volume".

1. **M1, the protocol layer** (crate only, no app): signer against AWS's published SigV4 vectors, request builders, XML
   parsers and builders, typed `S3Error` from `<Error><Code>`, the provider profile. Unit tests only.
2. **M2, fixtures**: `apps/desktop/test/s3-servers/` with VersityGW (primary) and Garage (no conditional writes), a
   `start.sh` / `stop.sh` on random high ports bound to 127.0.0.1, wired into the check runner's fixture lane.
3. **M3, connect and browse (tier 1)**: params + credential store key, connect probe and its refusal table, buckets at
   the root, `ListObjectsV2` listings feeding `on_progress`, metadata, connection state through `noting`, the servers
   hub (presets, account → bucket places, typed bucket names), the sign-in sheet's S3 variant.
4. **M4, read (tier 2a)**: ranged streaming GET, S3 → anywhere copies, archived badge + typed error, "Copy share link".
5. **M5, write (tier 2b)**: streaming PUT, multipart upload with equal parts, `create_file` with conditional write or
   check-then-write, folder markers, non-recursive `delete`, mtime metadata, the startup abort sweep, every applicable
   `cmdr_fs::volume::conformance` cell against both fixtures.
6. **M6, the rename capability and server-side copy**: the trait capability, routing in all five callers, multipart copy
   with progress, pause, cancel, F2's small/big split.
7. **M7, cost estimates**: price table endpoint in `apps/api-server`, the estimator in Rust (requests by class, egress,
   minimum duration and minimum object size), the estimate line + (i) in the Move, Copy, and Delete dialogs and the F2
   dialog path. Shipped as `crates/cmdr-s3/src/cost/` (table + estimator, `DETAILS.md` § "Cost estimates"),
   `/s3-prices/v1`, and `apps/desktop/src-tauri/src/s3_costs/` (the planner, from the dialog's own scan; its
   `DETAILS.md` § "Known gaps" lists what shows no estimate yet). The estimate is one-time cost only: requests,
   downloads, and early deletion; ongoing storage isn't an operation's cost.
8. **M8, live providers and polish**: real accounts (keys via `secret`), throughput and throttling measured per provider
   and the concurrency numbers set from them, conditional-write behavior confirmed on Wasabi and Hetzner, an AWS account
   root reaching a bucket in another region (tested against a fake AWS only, `crates/cmdr-s3/DETAILS.md` § "Providers"),
   friendly errors, delete-dialog versioning notes, docs (`C+D.md`, `docs/architecture.md`, capability matrix), and the
   i18n brief for the translator agent.
   - [x] Live suite and runner (`apps/desktop/test/s3-servers/live.sh`, the `live_` cells in `cmdr-s3`), run against R2,
         Hetzner, GCS, and Spaces on 2026-10-02; findings in `crates/cmdr-s3/DETAILS.md` § "Verified providers".
   - [x] Allowlists from evidence: conditional writes per operation, `refuses_short_body`, cross-bucket copy, the
         short-tail rule (R2 refuses a larger last part), GCS's missing `UploadPartCopy`. Two transport bugs the
         fixtures hid (an empty write without `Content-Length`) fixed.
   - [x] Concurrency measured: 16 copy parts and four upload parts stay right for every provider tested.
   - [x] GCS and Spaces presets, with prices.
   - [ ] Wasabi conditional writes, B2's short-body entry, and AWS region routing on a real account: no accounts.
   - [ ] Friendly errors, delete-dialog versioning notes, the capability matrix, and the i18n brief.
