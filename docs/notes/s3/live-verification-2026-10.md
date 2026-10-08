# S3 live verification, 2026-10-02

Every `cmdr-s3` live cell run against all seven named presets, to check each provider's profile against what the real
service does. AWS, B2, and Wasabi had never been run before (their entries came from docs). The distilled, current
findings live in `crates/cmdr-s3/DETAILS.md` § "Verified providers"; this note keeps the per-cell evidence behind them.

## Setup

- **Runner**: `apps/desktop/test/s3-servers/live.sh`, variables from `live-env.sh` beside it (sops store through
  `secret`). Run from a private snapshot of the commit under test with its own `CARGO_TARGET_DIR`, because two sibling
  agents were editing the shared worktree at the same time.
- **Buckets** (kept; the campaign isn't over):
  - AWS: `cmdr-s3-test-58fb74`, `cmdr-s3-test-58fb74-2` (`eu-north-1`), `cmdr-s3-test-58fb74-usw2` (`us-west-2`), as the
    IAM user `claude-agent`.
  - B2: `cmdr-s3-test-58fb74`, `cmdr-s3-test-58fb74-2` (`eu-central-003`, private). Application key
    `cmdr-s3-test-58fb74` reaches both (`B2_S3_TEST_KEY_ID` / `B2_S3_TEST_SECRET_ACCESS_KEY`); key
    `cmdr-s3-test-58fb74-scoped` reaches `-2` only (`B2_S3_SCOPED_KEY_ID` / `B2_S3_SCOPED_SECRET_ACCESS_KEY`). The
    master key doesn't work on B2's S3 API.
  - Wasabi: `cmdr-s3-test-58fb74`, `cmdr-s3-test-58fb74-2` (`eu-central-1`, trial account, root key).
  - Hetzner: `cmdr-s3-test-58fb74`, `cmdr-s3-test-58fb74-2` (`nbg1`), replacing the deleted `cmdr-s3-test-d0e600`.
  - R2 `cmdr-s3-test`, GCS and Spaces from the secret store: one bucket each, unchanged.
- ❗ **HTTP/1.1 only**: `cmdr-s3` built alone (as `live.sh` builds it) has no `reqwest` `http2` feature, so these cells
  never see HTTP/2-only provider behavior. Inside the app build the feature is on (`genai` enables it) and the client
  negotiates HTTP/2: GCS reset every app request with `PROTOCOL_ERROR` because of an explicit `host` header beside
  `:authority`, which only the app-level live suite caught (fixed in 47a2bd97f).
- **Throughput is skewed**: two other live runners shared the uplink. The numbers below are for orientation only.
- **Cost**: well under the $0.50-per-provider budget. Each provider took roughly 1.3 GB of uploads plus server-side
  copies per full run; B2 and Wasabi ran a second time on three cells. Wasabi bills its ~1.5 GB for 90 days (trial).

## Outcome per provider

Every full run passed (20 cells per provider plus the sweep), apart from the two failures under "Bugs found".

### AWS (`eu-north-1`)

- Conditional writes: `If-None-Match` enforced (412, old object kept) on Put, Complete, and Copy; 200 on a free key;
  R2's copy header ignored. Matches the doc-based allowlist.
- Cut-off PUT: refused over an object (original kept) and on a free key (nothing published).
- Multipart: equal parts, larger last part, smaller last part, larger first part all land; a 4 MiB part before the last
  is `EntityTooSmall`.
- `UploadPartCopy`: ranged, whole, and a 1 MiB source as the last part all land; a wrong `x-amz-copy-source-if-match`
  is 412. Copies across buckets work (`CopyObject` and `UploadPartCopy`).
- `DeleteObjects`: three keys, 1,000 absent keys fine; 1,001 is `MalformedXML`.
- `ListMultipartUploads` lists an upload at once; abort 204, a second abort 204, a part after it `NoSuchUpload`.
- Awkward names round-trip; `encoding-type=url` echoed; NFD and NFC are two objects; `x-amz-meta-mtime` verbatim; a
  wrong crc32 is `BadDigest`.
- Flows: one PUT, `CreateNew` refused (`AlreadyExists`), overwrite, date kept, 11 and 12 MiB in 5 MiB parts, 70 MiB at
  the 64 MiB floor, copies (one request, 70 MiB in parts, 11 MiB with a tail), rename, folder move, seven-day share
  link, empty folder: all fine.
- Throughput: 64 MiB upload at 2 / 4 / 8 parts 22 / 24 / 24 MiB/s; 140 MiB copy at 4 / 8 / 16 in flight 0.96 / 0.63 /
  0.60 s, 0.92 s in 64 MiB parts.

### Backblaze B2 (`eu-central-003`)

- Conditional writes: `501 NotImplemented` on Put, Complete, and Copy, old object kept (check-then-write is right).
- Cut-off PUT: refused both ways, in two runs. Confirms the SO-based `refuses_short_body` entry live.
- ❗ `x-amz-copy-source-if-match`: a wrong pin is 412 and the current one copies (every part of the 70 MiB and 11 MiB
  copies carries it), in two runs. **Added to `enforces_copy_source_pin`.**
- Multipart: every shape lands but the 4 MiB part before the last (`EntityTooSmall`). `UploadPartCopy` every shape
  lands. Cross-bucket copies work.
- `DeleteObjects` of 1,001 keys is `InvalidRequest`. A second abort is `NoSuchUpload`.
- NFD and NFC two objects; wrong crc32 `BadDigest`; flows all fine.
- `ListBuckets` lists three buckets with the test key, one of them (`cmdr-s3-test`) outside its scope: the
  `listAllBucketNames` capability names every bucket.
- Throughput: upload 16 / 21 / 22 MiB/s; copy 5.8 / 3.1 / 2.5 s, 2.5 s in 64 MiB parts.

### Wasabi (`eu-central-1`)

- ❗ Conditional writes: `If-None-Match` IGNORED on all three (200 and overwritten). Check-then-write is right; this is
  the case the allowlist exists for.
- Cut-off PUT: refused both ways, in two runs. **Added to `refuses_short_body`.**
- `x-amz-copy-source-if-match` ignored (the part copied), in two runs: stays off the pin list.
- Multipart and `UploadPartCopy`: every shape lands but the 4 MiB part before the last. Cross-bucket copies work.
- `DeleteObjects` of 1,001 keys is accepted (200). The builder caps at 1,000 anyway.
- NFD and NFC two objects; wrong crc32 `BadDigest`; flows all fine.
- Throughput: upload 14 / 14 / 12 MiB/s; copy 1.31 / 0.86 / 0.61 s, 1.65 s in 64 MiB parts.

### Hetzner (`nbg1`)

- Same as the earlier campaign: `If-None-Match` enforced on Put only; pin ignored; cut-off PUT refused; any part sizes;
  `DeleteObjects` of 1,001 a bodyless 400; NFD and NFC two objects; wrong crc32 ignored.
- New: cross-bucket `CopyObject` and `UploadPartCopy` work between the two new buckets.
- "Other" view of Hetzner: an overwrite goes as one part; cancelled before completion, the original survives.
- Throughput: upload 28 / 23 / 25 MiB/s; copy 1.06 / 0.65 / 0.61 s.

### R2, GCS, Spaces

- Every finding matches `DETAILS.md` § "Verified providers" from the earlier campaign: R2 enforces Put / Complete / its
  copy header and the pin, refuses a larger last part, and stores keys NFC; GCS ignores every precondition, has no
  `UploadPartCopy`, and refuses `x-goog-if-generation-match` beside SigV4 headers (`ExcessHeaderValues`); Spaces
  enforces Put only and ignores the pin.
- Throughput: R2 upload 20 / 19 / 18 MiB/s, copy 4.2 / 2.5 / 1.8 s; GCS upload 8 / 10 / 16 MiB/s, copy 0.8 / 0.7 / 0.7
  s; Spaces upload 11 / 23 / 22 MiB/s, copy 0.7 / 0.5 / 0.4 s.

## Allowlist changes

- **B2 → `enforces_copy_source_pin`**: 412 on a stale pin AND a successful pinned multipart copy, in two separate runs.
  Effect: B2 multipart copies skip the HEAD before the completion.
- **Wasabi → `refuses_short_body`**: a cut-off PUT kept the original AND published nothing on a free key, in two
  separate runs. Effect: a Wasabi overwrite goes as one PUT instead of a one-part multipart upload. Only "Other" is off
  the list now.
- **No removals**: every allowlisted entry held on every provider. AWS's doc-based entries (all three conditional
  writes, the pin, short body) are now live-verified.
- `profile_test.rs` was red first for both, then green; `cost_test.rs` moved its off-list overwrite case to "Other".

## AWS region routing

`live_connect_test.rs::live_connect_aws_routes_each_bucket_to_its_region`, from an `eu-north-1` account root:

- The root lists all three buckets; `ListBuckets` names each region.
- The `us-west-2` bucket lists, takes a write, and reads it back.
- A fresh root whose first request is a stat of the far object (learned from the redirect) gets it right; so does one
  whose first request is a write (`HeadBucket` first).
- A share link to the far object fetches unsigned (200); a server-side copy from `eu-north-1` into `us-west-2` works.
- The `us-west-2` bucket opened as a place is `WrongRegion { region: Some("us-west-2") }`.

No user-visible error anywhere.

## Connect refusals

`live_connect_test.rs::live_connect_refusals`, after the 401 fix:

- **Wrong secret**: `KeysRejected` everywhere on both the bucket and the account root, except R2 (`AccessDenied` on the
  bucket, `BucketListRefused` on the root): R2 answers `ListBuckets` from its bucket-scoped key with `AccessDenied`
  whatever the secret, and the `HeadBucket` that follows has no body.
- **Wrong key id**: `KeysRejected` everywhere, both places.
- **Missing bucket**: `NoSuchBucket` everywhere but R2 (`AccessDenied`, its 403 for a bucket it doesn't know).
- **Bucket through another region's endpoint**: AWS `us-east-1` → `WrongRegion { eu-north-1 }`; Wasabi `us-east-1` →
  `WrongRegion { eu-central-1 }`; Hetzner `fsn1` → `NoSuchBucket`; Spaces `nyc3` → `NoSuchBucket`; B2 `us-west-004` →
  `KeysRejected` (B2 keys live in one region).
- **Account root through another region**: AWS and Wasabi connect; Hetzner connects and lists none; Spaces
  `BucketListRefused`; B2 `KeysRejected`.
- **B2 key scoped to `-2`**: opens `-2`, `AccessDenied` on the other bucket, `BucketListRefused` on the account root.

## Bugs found

- **Fixed, 8df7fb335**: an unknown key id on R2 answers `401` `<Code>Unauthorized</Code>` (bodyless on a HEAD), which
  the probe didn't know: a bucket place came out `NotAnS3Endpoint` and the root `Transport("Unauthorized (HTTP 401)")`.
  Any 401 is now `KeysRejected` at connect and `PermissionDenied` mid-session. TDD red first in `refusal_test.rs` and
  `errors_test.rs`; live-verified after.
- **Fixed, a78a84e65**: `live_cleanup_removes_every_leftover` swept all of `cmdr-live/`, and with three runners on R2's
  one bucket it deleted a running flow's objects mid-cell (copy `NotFound`, rename `NotFound`, share link 404). It now
  removes only objects and uploads older than three hours; the R2 flow passes alone and alongside.

## Open questions and recommendations

- ❗ **A Wasabi account root can't open a bucket in another region.** Reached through `us-east-1` it lists the
  `eu-central-1` buckets, but listing, writing, and deleting in one fail as `IoError` "PermanentRedirect (HTTP 301)".
  Wasabi answers exactly as AWS does (`301` with `x-amz-bucket-region` and `Location`,
  `400 AuthorizationHeaderMalformed` with `<Region>`; curl, 2026-10-02), so `routing.rs` would work unchanged. The fix:
  a per-provider regional endpoint in `ProviderProfile::reroute` (`s3.<region>.wasabisys.com`) and `route_each_bucket`
  gated on "the profile routes" rather than `ProviderKind::Aws`. Not changed here: `routing_is_for_aws_only` and its
  profile twin pin AWS-only as a deliberate decision. Small, a clear win.
- **R2's wrong secret reads as the ambiguous refusal.** A bucket place could tell it apart: `ListObjectsV2` with
  `max-keys=1` answers `SignatureDoesNotMatch` with a body on R2, where `HeadBucket` has none. One request more per R2
  connect. A tradeoff: better words for a typo vs a request.
- **Hetzner and Spaces answer a bucket in another location with `NoSuchBucket`**, and B2 another region with
  `KeysRejected`. True to what the servers say, but a user who picked the wrong location reads "no such bucket". A
  location hint in the words is a UX call.
- **Cost drift (reported to the sibling building the sent-vs-counted cell)**: off the pin allowlist, a multipart
  server-side copy sends one more `HeadObject` before completing (`server_copy.rs::source_unchanged`), but
  `Workload::copy_on_server` counts `2 + checks` either way.
- **A B2 key with `listAllBucketNames` lists buckets outside its scope**, so an account root shows a bucket it can't
  open. Unverified what opening it says (likely `AccessDenied`).
- **Still unverified**: a cross-bucket copy on R2, GCS, and Spaces (each key reaches one bucket).

## Outcome: Wasabi routes per bucket

The first open question above is resolved: a Wasabi account root now routes each bucket to its own region.

- **The region signal** (curl and `live.sh`, 2026-10-02): Wasabi gives all of them. `x-amz-bucket-region` rides on every
  answer, 200s included, and on every 301; `ListBuckets` carries `<BucketRegion>`; a request signed for the wrong region
  is `400 AuthorizationHeaderMalformed` with `<Region>`; `GetBucketLocation` answers from any regional endpoint.
  `routing.rs` already learns from the first three, so it needed no change.
- **The change**: routing is a profile capability, an allowlist like the others. A preset whose endpoint is per region
  and whose answers name the region carries a `RegionalHost` template (`s3.<region>.amazonaws.com`,
  `s3.<region>.wasabisys.com`); its own endpoint and every reroute are built from it, and `route_each_bucket` asks
  `ProviderProfile::routes_by_region`. Every other preset has none, so nothing there is ever re-routed (asserted for R2,
  B2, Hetzner, GCS, Spaces, and "Other"). TDD red first on the Wasabi assertions.
- **New bucket**: Wasabi `cmdr-s3-test-58fb74-euw1` in `eu-west-1` (`CMDR_S3_LIVE_WASABI_FAR_BUCKET` / `_FAR_REGION` in
  `live-env.sh`), kept.
- **Live** (`live_connect_routes_each_bucket_to_its_region`, now run for every provider with a `_FAR_BUCKET`): from an
  `eu-central-1` root, the `eu-west-1` bucket is listed, written, read back, stat'd cold from a redirect, written cold
  after a `HeadBucket`, and share-linked (200). As a place it's `WrongRegion { eu-west-1 }`. A root through `us-east-1`
  now opens the `eu-central-1` buckets too. AWS's run is unchanged and green.
- ❗ **A server-side copy across Wasabi regions is refused**: `400 NotImplemented`, "Operation not supported across
  regions". Cmdr reads it as `NotSupported`, so the engine streams the bytes instead; nothing reports a copy that didn't
  land. The live cell now fails if a copy answers `Ok` with nothing at the destination.

## Outcome: the crate speaks HTTP/2 like the app

The "HTTP/1.1 only" caveat in § Setup no longer holds.

- **Why they differed**: `cmdr-s3` declared reqwest with `rustls` and `stream` only. The app build also pulls `genai`,
  which turns on reqwest's `http2` (plus `gzip`, `charset`, and `system-proxy`), and Cargo unifies features only across
  what's built together. So the app negotiated HTTP/2 while the crate alone, `live.sh` included, spoke HTTP/1.1.
- **The change**: `cmdr-s3/Cargo.toml` declares `http2` itself. `transport_test.rs::the_client_is_built_with_http2`
  calls `ClientBuilder::http2_prior_knowledge`, which exists only with the feature, so dropping it fails to compile
  (seen red first). A test-only record of each answer's HTTP version backs `live_connect_speaks_http2_where_offered`.
- **Who offers HTTP/2** (that cell and `curl --http2`, 2026-10-02): Hetzner, GCS, and Spaces negotiate HTTP/2; R2, AWS,
  B2, and Wasabi answer over HTTP/1.1 only, so for them nothing changes.
- **Rerun over HTTP/2** (every live cell, the siblings' hostile cells included; B2 not rerun, see below):
  - GCS 33 of 33, Hetzner and Wasabi 33 of 33, R2 32 of 32, AWS green apart from the version cell's first, wrong blanket
    assertion (S3 doesn't offer HTTP/2), since fixed.
  - Spaces 32 of 33: one rename of the key `%20literal.txt` hit `ConnectionTimeout` in the full run and passed when
    rerun alone, with three runners loading the link. Not reproducible, so not a finding.
  - No provider-specific HTTP/2 problem surfaced, so no behavior changed.
- **B2: not rerun, download cap.** Every B2 read and copy answered `403 AccessDenied` "Cannot download file, download
  bandwidth or transaction (Class B) cap exceeded" after a day of three live runners (mostly a 1,005-object rename). B2
  doesn't negotiate HTTP/2 anyway. The cap resets at 00:00 UTC or on B2's Caps & Alerts page. Note for the app: Cmdr
  reads that refusal as `PermissionDenied`, which says nothing about a cap.
- **Still reaching only the app build**: reqwest's `gzip` (an automatic `Accept-Encoding: gzip` and transparent
  decompression), `charset`, and `system-proxy`, all from `genai`. `gzip` deserves a look: an object stored with
  `Content-Encoding: gzip` would come back decompressed in the app (its length no longer the object's), and not in these
  tests. `cmdr-webdav` has the same HTTP/2 gap as `cmdr-s3` had.
- **Also fixed on the way, f1a9969ed**: 865680063 left `batch.rs` tripping clippy's `while_immutable_condition`, which
  failed every session's clippy lane.

## Outcome: GCS refuses an occupied key atomically

`live-hostile` saw two simultaneous `CreateNew` writes to one key both succeed on GCS, B2, and Wasabi (about one round
in three), losing one writer's bytes: check-then-write's blind window. GCS closes it now for one-request writes.

- **The precondition** (curl, two separate runs, 2026-10-02): `x-goog-if-generation-match: 0` is
  `412 PreconditionFailed` with the old bytes kept over an occupied key, and 200 on a free key, on PUT and `CopyObject`.
  On `CompleteMultipartUpload` it's ignored (200, object replaced); on the multipart initiate it's refused
  (`400 NotImplemented`). So Put and Copy are enforceable, Complete isn't.
- **The catch**: GCS refuses its own headers beside any `x-amz-*` one (`400 ExcessHeaderValues`, on a free key too), so
  the precondition can't ride an AWS-signed request. Signed GCS's way (`GOOG4-HMAC-SHA256`, `x-goog-date`,
  `x-goog-content-sha256`, `x-goog-meta-*`, `x-goog-copy-source`) it works, and `x-goog-meta-mtime` reads back as
  `x-amz-meta-mtime`.
- **The change**: a typed mode, `NoOverwrite::GoogGenerationMatch`, listed for GCS's Put and Copy (Complete stays
  check-then-write). `ops::guarded` adds the header and marks the request `Dialect::Goog`; `sigv4::sign` then signs it
  as GOOG4 and spells every `x-amz-*` header `x-goog-*`. Everything else stays SigV4. TDD red first in
  `profile_test.rs`, `ops_test.rs`, and `sigv4_test.rs` (the GOOG4 signature pinned against one computed independently
  with `openssl`). A GCS server-side copy now skips its no-overwrite HEAD (`cost_test.rs` updated).
- **Live**: `live_conditional_writes_per_operation` now asks through the builders and asserts the mode both ways (412
  kept, 200 landed, Put and Copy). `live_hostile_races` on GCS, run twice: two racing one-PUT `CreateNew`s left one
  `AlreadyExists` and the winner's bytes in six of six rounds. The flow cell is green, and the date is kept.
- **Accepted risk, documented in `DETAILS.md`**: B2 and Wasabi have no enforced precondition (B2 answers 501, Wasabi
  ignores `If-None-Match`), so they stay check-then-write, and so does every multipart completion except AWS's and R2's.

## Outcome: compressed objects copy as stored

The `gzip` open question above, reproduced and fixed (`508c777a7` for S3, `68cac12d8` for WebDAV).

- **Reproduced** (`live_encoded_objects_read_back_verbatim`: 33 B of text stored as gzip 51 B, br 35 B, zstd 46 B, and
  deflate 39 B, each with its `Content-Encoding`, on R2, GCS, Spaces, AWS, Wasabi, and Hetzner; B2 skipped for its cap):
  - The crate as `live.sh` built it (no decoders): every object verbatim, except ❗ GCS gzip: 33 B back and no size,
    because GCS decompresses server-side for a client that doesn't send `Accept-Encoding: gzip` (its "decompressive
    transcoding", which also ignores `Range`).
  - The client the app ships (the crate with the app's decoders on): every object on every provider came back as the 33
    decoded bytes, and the stat had no size (reqwest drops `Content-Length` when it decodes, on a HEAD too). The app has
    `gzip` today, so any gzip-stored object downloaded decompressed in the app.
- **Fixed**:
  - Both clients turn every decoder off (`no_gzip`, `no_brotli`, `no_deflate`, `no_zstd`), and both crates' test builds
    turn every decoder on through a dev-dependency, so the fake-server tests (red first) and the live cell read through
    the app's client.
  - GCS: every GET and HEAD sends `Accept-Encoding: gzip`, added after signing because GCS's front end rewrites it
    before checking the signature (`SignatureDoesNotMatch` when signed, curl). A HEAD there still has no
    `Content-Length` for such an object, only `x-goog-stored-content-length`, which every size read now falls back to.
  - After: 24 of 24 objects verbatim with the right stat size on six providers; GCS's flow and sizes cells (ranged reads
    included) green.
- **WebDAV** had the same decoder gap and the same missing `http2`; both fixed with a fake-server test, red first. Its
  suites after the change, all green:
  - `cmdr-webdav`'s Docker cells on the `core` stack (`cargo test -p cmdr-webdav -- --ignored`): 40 of 40.
  - `pnpm check desktop-rust-webdav-nextcloud` (the sabre/dav cells): OK.
  - The app crate's `webdav_integration_*` cells, which run through the client as the app builds it: 65 of 65. Under
    `cargo nextest` every one first hit the 8 s cap at load average 12 (one passes alone in 0.89 s, so it was
    starvation); in one `cargo test` process 63 passed and two wiring cells failed on shared process state, and both
    passed alone. The full `desktop-rust-integration-tests` lane wasn't run: it brings up every fixture stack.

## Outcome: GCS-written metadata reads back under Cmdr's names

The question behind trusting `GoogGenerationMatch`: an object written GOOG4-signed carries `x-goog-meta-mtime` and
`x-goog-meta-cmdr-write`. If an S3-signed HEAD answered under those names, the stat would lose the date, the landing
check would misjudge Cmdr's own write as another writer's, and the cut-off cleanup would never see its token.

- **Live** (`live_flow_test.rs::live_create_new_metadata_reads_back_under_the_names_cmdr_reads`, R2, GCS, Spaces, AWS,
  Wasabi, and Hetzner, 2026-10-02): after a `CreateNew` through the volume (GOOG4-signed on GCS), the HEAD's metadata
  headers were `x-amz-meta-mtime` and `x-amz-meta-cmdr-write` on every provider, GCS included, and the stat kept the
  date. The curl probes earlier showed the same for a GOOG4 copy with `REPLACE`.
- **Pinned**: `sigv4_test.rs::metadata_written_gcs_s_way_is_spelled_x_goog_and_read_back_as_x_amz` pins both spellings
  (out as `x-goog-meta-*`, read as `MTIME_HEADER` / `WRITE_TOKEN_HEADER`). No reader change was needed. A listing
  carries no user metadata on any provider, so it isn't involved.

## Outcome: B2 rerun within its free caps

The B2 cells the daily cap stopped (§ "Outcome: the crate speaks HTTP/2 like the app"), rerun on 2026-10-03 from just
after 00:00 UTC, one cell at a time, against a budget of about 2,000 Class B requests (every GET and HEAD) and 700 MB
downloaded, under B2's free 2,500 and 1 GB a day. Class C (every LIST, `ListMultipartUploads`, `HeadBucket`) is free up
to 2,500 a day too, so it was counted alongside. Counts from the request tally (`RUST_LOG=s3_sent=trace`), seeding and
cleanup included; the hostile cell has no logger, so its numbers are estimated from its shape.

- **`live_hostile_sizes`**: passed (`live-hostile-2026-10.md` § "B2's daily cap"). ~80 Class B, ~20 Class C, ~300 MiB
  down, with B2's big object cut to 200 MiB (`c8ce363e4`).
- **Renames**, run twice: 5 of 5 on the second run. 1,115 Class B, 183 Class C, ~98 MiB down over both.
- **Merges and moves**: 14 of 15, then the failed flow passed alone. 820 Class B, 970 Class C, ~31 MiB down.
- **Safety**: five flows, 5 of 5, the new upload pause path among them. 45 Class B, 84 Class C, ~20 MiB down.
- **Dropped for the budget**: the other seven safety flows and the cross-provider streams (`live-engine-2026-10.md`).
- **Totals**: about 2,060 Class B (estimated 1,820 beforehand; the merges cost 656, not 400), about 1,260 Class C, and
  about 450 MiB downloaded. No cap was reached, and no object stayed under `cmdr-live/` in either bucket.

Found on the way, both B2 answering a PUT `500 InternalError` ("internal incident"), twice in about 1,200 PUTs:

- **The live seeder retried only a 503** (`5d72f86f1`): it now sends anything `S3Error::is_retryable` calls transient
  again.
- **A one-PUT write failed its file on a fault** (`551303ee4`): parts, server-side copies, and batch deletes already
  went again on a throttle or fault, a single PUT didn't. It now goes again after 1 s and 2 s, and before each resend
  `landed_whole` checks whether the fault published ours anyway. Red first in `put_retry_test.rs`.
- **The engine runner can rerun single flows** (`450f869da`, `CMDR_S3_LIVE_FLOWS`), so a capped account pays about 100
  Class B for one flow's rerun instead of a whole cell's 650.
