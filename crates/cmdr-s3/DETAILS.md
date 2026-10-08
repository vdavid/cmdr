# cmdr-s3 details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions. Provider facts and their sources:
`docs/notes/s3/provider-research.md`; why there's no S3 library underneath:
`docs/notes/s3/library-and-fixture-audit.md`.

## Where the crate stands

Connect, browse, read, write, and server-side copy work: the transport, the connect probe, and a `Volume` that lists,
stats, streams, scans for a copy, uploads (one PUT or in parts), copies within the account without the bytes leaving the
server, makes folders, deletes one node or a batch, and renames one small file. A folder or a big object answers
`RenameWork::CopyThenDelete`, and the app routes such a rename through its transfer engine as a move
(`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Renames that run as moves"). `cost/` prices a
planned operation at list prices (§ "Cost estimates").

Every dependency was already in `Cargo.lock` when the crate was built. Check `cargo tree -d` before adding one.

## The model: one volume per place

The ACCOUNT is the endpoint plus the access key id; it owns the secret (store service `s3+<scheme>://<host>:<port>`,
scoped by the key id, so every bucket under one key shares it). A PLACE is a bucket under it, or the account root, whose
children are the buckets (`apps/desktop/src/lib/servers/DETAILS.md` § "The model", bucket = place). Each place is its
own volume with its own id (`cmdr_fs::volume::s3_volume_id(host, port, key id, bucket)`), because a pin, a tab, and a
switcher row each key on a place.

- **App paths are the account's**, `s3://<key id>@<host>:<port>/<bucket>/<key>`, and a place's root hangs under that
  prefix (`/` or `/<bucket>`). So one object has one app spelling whichever place reached it, and a bucket place refuses
  another bucket's path (`RemoteRoot`'s containment check).
- **Two places of one account each hold their own client.** Sharing one is an optimisation for later; nothing depends on
  it.
- **The account root needs `ListBuckets`.** A bucket-scoped key (R2 non-admin tokens, B2 keys without
  `listAllBucketNames`) gets a typed `BucketListRefused` there, and its way in is a bucket place.

## Connecting

`connect_s3_volume` reads the secret from the `CredentialStore` (nothing stored is `NeedsCredentials`), builds an
`S3Client` (`user_agent("Cmdr")`, 10 s connect timeout, no `read_timeout`, redirects off, plus a pool-free twin for the
silence probe), and probes. On success it records the PII-free `s3_connected` with one property, `provider`
(`S3Provider::kind_name`).

**Decision: the crate declares reqwest's `http2` itself** (`Cargo.toml`), though the app gets it anyway through `genai`.
**Why**: features unify only across what's built together, so `cmdr-s3` alone (its tests, the live suite) spoke HTTP/1.1
while the app negotiated HTTP/2, and GCS reset every app request over a header only HTTP/2 refuses, invisible to the
live suite. `transport_test.rs::the_client_is_built_with_http2` fails to compile without the feature;
`live_connect_test.rs::live_connect_speaks_http2_where_offered` pins each provider's negotiated version. The app's other
unified reqwest features (`charset`, `system-proxy`, from `genai`) still reach only the app build.

**Decision: every response decoder is off in the client builder** (`no_gzip`, `no_brotli`, `no_deflate`, `no_zstd`).
**Why**: a file manager copies bytes, it doesn't decode them. The app's reqwest has `gzip` unified in (through `genai`),
so an object stored with `Content-Encoding: gzip` (web assets often are) downloaded decompressed in the app, and its
HEAD lost `Content-Length`, so a stat had no size: every provider, every encoding the decoders know (live, the crate
built with the app's decoders, 2026-10-02). The test build turns every decoder on through a dev-dependency, so
`transport_test.rs::an_encoded_object_reads_back_as_its_stored_bytes` (a fake answering `Content-Encoding: gzip`) and
`live_flow_test.rs::live_encoded_objects_read_back_verbatim` (gzip, br, zstd, and deflate objects on every provider)
read through the client the app ships.

**The probe is `ListBuckets` first, then `HeadBucket` for a bucket place.** `ListBuckets` goes first even for a bucket
because its error BODY is the only thing that can tell a wrong secret from a key without rights; a HEAD has no body. The
table (`refusal.rs`, one cell per row in `refusal_test.rs`):

- `ListBuckets` 2xx with a `ListAllMyBucketsResult`: the keys work; a bucket place goes on to `HeadBucket`.
- `SignatureDoesNotMatch` / `InvalidAccessKeyId`, or any 401: `KeysRejected`, whatever the place. R2 answers a key id it
  doesn't know with `401` `<Code>Unauthorized</Code>`, bodyless on a HEAD (verified on R2, live.sh, 2026-10-02).
- `RequestTimeTooSkewed`: `ClockSkewed` (this Mac's clock is off by more than 15 minutes).
- 5xx or a throttle: `Transport`.
- No S3 `<Error>` body (an HTML page, a bare 404): `NotAnS3Endpoint`.
- `AccessDenied` (or any other S3 error) on the account root: `BucketListRefused`; on a bucket place: fall back to
  `HeadBucket`, which a bucket-scoped key passes.
- `HeadBucket` 404: `NoSuchBucket`. 401: `KeysRejected`. 403: `AccessDenied` (can't tell a wrong key from no rights). A
  redirect, or any answer carrying `x-amz-bucket-region`: `WrongRegion { region }`.
- Transport failures: `TimedOut`, `CertificateUntrusted` (an `InvalidData` `io::Error` in the source chain), or
  `Unreachable`.

**Gotcha: Garage answers a wrong secret with `AccessDenied`** (VersityGW with `SignatureDoesNotMatch`; fixture README).
So `AccessDenied` is never `KeysRejected`, and the words for `BucketListRefused` and `AccessDenied` ask about both the
keys and the rights. `integration_test.rs` pins both servers' answers.

## Listing and stat

- **The account root**: `ListBuckets`, every page (AWS paginates past 10,000), buckets as folders carrying their
  creation date as `created_at`.
- **A folder**: `ListObjectsV2` with `prefix=<key>/` and `delimiter=/`, every page, `on_progress` after each page with
  the running tally (never per entry), cancel checked between pages. `listing.rs` turns a page into children:
  `CommonPrefixes` are folders; the folder's own marker (the key `<key>/` itself) is left out; a key with a `/` past the
  prefix (a child's marker, or a server that ignored the delimiter) names a folder, once; an empty, `.`, or `..` name is
  left out (unaddressable). `settle` then works on every page at once (a file and its folder can straddle a page break).
- **A file beside a folder of its name lists as `<name> (file)`.** S3 allows `notes` beside `notes/…`, but a pane row is
  one path: the frontend keys rows, selection, the listing cache, and every operation by `path`, and Svelte throws on a
  duplicate key. Showing both under one name would need a row whose name isn't its path's last segment, which the app
  assumes everywhere (copy destinations, rename, breadcrumbs), so the file takes the suffix in its name AND its path.
  The volume remembers each such file's real path (`beside_folders`, refreshed by every listing of its folder, forgotten
  when the file is deleted or renamed), and `paths.rs::resolve` maps `…/notes (file)` back to the key `notes` and marks
  `…/notes` as ❗ the folder ONLY: stat, read, share link, delete, rename, and `rename_work` on the folder row never
  fall through to the file (else the engine's delete of a folder whose last child just went would delete the file of its
  name). The suffix is a path segment, so it's untranslated. If a real `notes (file)` already holds the name, the
  shadowed file stays unlisted. VersityGW (POSIX-backed) can't hold both at all; Garage can, and `beside_folder_test.rs`
  drives every row operation there.
- **A missing folder is `NotFound`**: S3 has no folders, so a prefix with no keys at all (its marker included) doesn't
  exist, and a listing that saw nothing says so rather than showing an empty folder.
- **`get_metadata`**: the account root without a request; a bucket by `HeadBucket`; a key by `HeadObject`, and when that
  finds no object, one `ListObjectsV2` capped at one key under `<key>/` decides folder or `NotFound`.
- **Dates**: a HEAD's `x-amz-meta-mtime` (rclone's key and format, the source file's own mtime) wins over
  `Last-Modified` (the upload time). ❗ **A listing shows `LastModified`**: `ListObjectsV2` carries no user metadata,
  and a HEAD per child to fetch it would cost a request per file. So a file Cmdr or rclone uploaded shows its upload
  time in the pane and its own mtime in Get info. Decision: cost over consistency, because every request is billed.
- **Errors** (`src/volume/errors.rs`): not found (`NoSuchKey`, `NoSuchBucket`, a bodyless 404) is `NotFound(path)`; a
  refusal (`AccessDenied`, keys that stopped working, a bodyless 403, any 401) is `PermissionDenied { path }`; an
  archived object (`InvalidObjectState`) is `ColdStorage(path)`; `NotImplemented` / 405 is `NotSupported`; the rest is
  `IoError` carrying `<Code> (HTTP nnn)` for the logs.
- **Space**: `NotSupported`, and no poll interval. S3 has no capacity, and "bytes used" is a listing of every key.

## Reading

`streams.rs`, modelled on `crates/cmdr-webdav/src/volume/streams.rs`:

- **One GET per stream, pulled a chunk at a time** through `S3Client::open`, which returns the answer with its body
  still on the wire (`Opened`). ❌ No `.timeout()` on the request: only the headers wait is bounded (`QUERY_BUDGET`),
  and the body gets `REQUEST_BUDGET` of idle time per chunk, never a total, so a multi-GB download has no ceiling. Every
  chunk counts as `heard` for the silence watch. Peak memory per stream is one socket read.
- **A read from an offset asks `Range: bytes=<offset>-`.** A 206 names the full length in `Content-Range`, so a resumed
  stream's `total_size` stays the whole object. ❗ A 200 to a ranged GET means the server ignored `Range`; the stream
  skips `offset` bytes locally (`judge_get`, unit-tested). A 416 (at or past the end) is an empty read. Both fixtures
  answer 206 with the exact window (verified on VersityGW v1.8.0 and Garage v2.4.1, `read_test.rs`, 2026-10-01).
- **`read_range`** asks for exactly `[offset, offset + len)` and drops the response once the window is full, so a server
  that ignored the range doesn't stream the rest of the object. It backs remote-archive browsing.
- **A refused GET reads its `<Error>` body** (bounded by `REQUEST_BUDGET`) and goes through `map_s3_error`. The account
  root and a bucket's top answer `IsADirectory` without a request.
- **The copy scan** (`scan.rs`) hands `cmdr_fs::volume::scan_walk` the backend's own stat and listing: one listing per
  folder. A recursive `ListObjectsV2` (no delimiter, 1,000 keys per request whatever the nesting) would bill fewer
  requests for a deep tree; it's not done because the copy that follows lists each folder again anyway.

## Share links

"Copy share link" is `Volume::share_link` (`share_link.rs`): a presigned GET (`S3Client::share_link`, over
`ops::share_link`) for one key, signed offline with the account's keys, so it's free, instant, and sends nothing (❌ no
`noting`). The expiry is a `ShareLinkExpiry` (one hour, one day, or seven days, S3's SigV4 ceiling). The account root
and a bucket's top answer `IsADirectory`; a key that's really a prefix gets a link that answers 404, since telling the
two apart would cost a request and the UI only offers it on a file row. No live client means `DeviceDisconnected`: the
credentials live on the client. Both fixtures serve the link to a plain unsigned `reqwest::get` (`read_test.rs`,
verified on VersityGW v1.8.0 and Garage v2.4.1, 2026-10-01). The app's `copy_share_link` command writes it to the
clipboard in Rust and returns only the outcome, so the URL never reaches IPC or a frontend log.

## Connection state and reconnect

The WebDAV model, nearly line for line (`crates/cmdr-webdav/DETAILS.md` § "The reconnect model" and § "Silent or slow"
have the reasoning): `Connected | Disconnected | NeedsCredentials`, transitions only through `emit_if_changed`, the
first `DeviceDisconnected` flips the state once and starts a 2/5/15/30/60/120 s backoff when "reconnect automatically"
is on, one unattended probe out of the store, and a refusal (`KeysRejected`, `AccessDenied`, `BucketListRefused`)
latches `NeedsCredentials` until a person signs in. `reconnect_with_credentials(username, password)` takes the access
key id as the username and refuses any other key (`NotSupported`): another key is another account. `sign_in_prompt` is
`SignInShape::AccessKeys`. The silence watch is the shared `cmdr_fs::volume::liveness`, probing with an unsigned HEAD on
the endpoint through the pool-free client.

## Which side a test lives on

Unit cells: the refusal table, the listing rules, path splitting, the error map, the state machine, the switch, and how
a GET's answer is read (`streams_test.rs`), all without a server. Docker cells (`#[ignore]`d, run by the shared fixture
lane through `package(cmdr-s3)`): every `integration_test.rs`, `read_test.rs`, and `write_test.rs` cell runs against
BOTH fixtures, because they disagree on a wrong secret and on preconditions; `conformance_test.rs` runs every shared
`cmdr_fs::volume::conformance` assertion that applies (all but the unknown-length refusal, which is for backends that
can't take one, and the link one: S3 has no links), on the check-then-write path on both servers; `write_test.rs` also
proves the header path against VersityGW (`S3Volume::trust_conditional_writes`, testing only); the app's
`file_system/write_operations/backend_suites/s3_transfer_integration_test.rs` copies onto, off, and between buckets
through the transfer engine, byte for byte, plus the shared network scenarios (cancel, an answered Overwrite in place,
awkward names); `connection_drop_test.rs` cuts a `TcpProxy` in front of VersityGW, ❌ never the container. Seeding goes
through `volume::testing::seed`, this crate's own builders, so a cell about the write path never seeds through the code
it tests. `copy_test.rs` covers server-side copy (whole and in parts, the date kept, across buckets and refused where
the profile forbids it, cancel, pause between parts, no-overwrite) and `batch_test.rs` the batch delete past 1,000 keys,
the capped tally, and `rename_work`, both on both fixtures; the app's `backend_suites/s3_rename_integration_test.rs`
drives renames that run as moves end to end. Multipart cells cut 5 MiB parts (`S3Volume::set_part_floor`, testing only)
except one per fixture at the production 64 MiB. **Live cells** (`live_protocol_test.rs`, `live_flow_test.rs`,
`live_connect_test.rs`, over `live_support.rs`) run the same questions against real accounts on all seven named presets,
only through `apps/desktop/test/s3-servers/live.sh` (`CMDR_S3_LIVE=1` plus each account's variables; without them every
cell skips silently, so the lanes never reach an account). Each protocol cell also asserts the profile against what it
saw, so an allowlist trusting a header a provider ignores fails the run; § "Verified providers" holds the findings. The
1,005-key paging prefix (`cmdr-seed-paging-1005/`) and the 65 MiB object (`cmdr-seed-large-65mib/blob.bin`, `seed_once`)
are seeded once per fixture and kept; every other cell works under a `scratch_prefix` of its own, since the stack's
objects persist across runs. The fixtures expire everything under `cmdr-test-` on their own (VersityGW after two hours,
Garage after a day; `apps/desktop/test/s3-servers/README.md`), so ❗ a key meant to outlive a run goes under
`cmdr-seed-`, ❌ never `cmdr-test-`.

**One scenario, either server: `testing::S3Target`.** A fixture or a live account (`testing::live`, the one list of
accounts both runners read), with the same seeding, probing, unfinished-upload listing, and prefix cleanup on both; the
fixture free functions delegate to it. The app's S3 engine suites take an `S3Target`, so their Docker cells and
`backend_suites/s3_live_engine_test.rs` (run by `apps/desktop/test/s3-servers/live-engine.sh`) share every body: the
byte path, renames, the shared semantics and safety scenarios, a cancel between parts, a cut-off overwrite, rollback, a
1,005-object delete, copies between providers, archived objects on AWS, and the request counts below. A live flow
removes everything its run wrote (`S3Target::clean_run`). Findings: `docs/notes/s3/live-engine-2026-10.md`.

**Counting requests.** Under the `testing` feature `S3Client` tallies every signed request by S3 operation
(`testing::take_sent_requests`), and `Workload::counted_requests` gives the estimate's counts by the same names, so a
cell compares what the engine sent with what `s3_costs` estimates (`s3_engine_integration_test.rs`). The requests that
move bytes match on both fixtures and every live provider; the engine's HEADs and LISTs around them, and the volume
delete's per-object requests, don't (the note has the numbers).

## The public surface is capped

Root re-exports: 7 items (`S3ConnectionParams`, `S3Provider`, `InvalidProvider`, `S3ConnectError`, `S3Volume`,
`UnattendedReconnect`, `connect_s3_volume`) plus `pub mod volume` and `pub mod cost`, which the check counts as an
eighth and ninth. Public modules: 2 (`volume`, `cost`), plus `volume::testing` under the `testing` feature.
`index-crate-isolation` pins it at exactly 9 / 2 / 15: `cmdr-webdav`'s shape plus the provider preset the host maps its
saved entry onto, the refusal for a preset that can't make an endpoint, and the cost estimator (`PriceTable`,
`PriceTableError`, `Workload`, `Estimate`, `LineItem`, `S3Volume::cost_workload`, `S3Volume::copies_on_server_from`).

**Decision**: the estimator lives here, not in the app. **Why**: how many requests a write sends (a verifying HEAD, the
part plan, the no-overwrite HEAD) is this crate's knowledge, and counting it beside the write paths keeps the two from
drifting. `PriceTableError` is opaque (a `Display` for the log) because nothing downstream branches on why a served
table was refused: the app keeps the copy it has.

## Signing

- **Header auth for every API call.** `sigv4::sign` adds `x-amz-date`, `x-amz-content-sha256`, `Authorization`, and an
  explicit `host` (so the transport can't spell an IPv6 literal or a default port differently from what was signed). It
  signs every header in the request. The transport may add headers afterwards (`user-agent`, `content-length`); those go
  unsigned, which SigV4 allows.
- **The payload hash follows the body** (`PayloadHash::for_body`): `Body::Streamed` signs `UNSIGNED-PAYLOAD` so the
  bytes are read once; `Body::Bytes` (the XML bodies) and `Body::Empty` sign their real SHA-256. The spec said
  `UNSIGNED-PAYLOAD` everywhere. Hashing what's already in memory costs nothing, and it's what a strict server (or one
  on plain `http://`) expects.
- **GCS's own dialect, for one header** (`request::Dialect::Goog`, set by `ops::guarded` for
  `NoOverwrite::GoogGenerationMatch`): `GOOG4-HMAC-SHA256`, a `<date>/auto/storage/goog4_request` scope keyed from
  `GOOG4<secret>`, `x-goog-date` / `x-goog-content-sha256`, and every `x-amz-*` header spelled `x-goog-*`. The same
  canonical request and HMAC chain as SigV4. Only a create-only GCS Put or Copy goes out this way; everything else stays
  SigV4 (verified on GCS, `sigv4_test.rs` against an independently computed signature and live.sh, 2026-10-02). ❗ The
  metadata written as `x-goog-meta-mtime` / `x-goog-meta-cmdr-write` comes back on an S3-signed HEAD as
  `x-amz-meta-mtime` / `x-amz-meta-cmdr-write`, the names the stat, the landing check, and the cut-off cleanup read, so
  readers need no per-provider spelling (`live_create_new_metadata_reads_back_under_the_names_cmdr_reads`, every
  provider, 2026-10-02; pinned in `sigv4_test.rs`).
- **Query auth only for share links** (`sigv4::presign`, `ops::share_link`): signs `host` alone, expiry 1 s to 604,800 s
  (seven days, S3's SigV4 ceiling). A signature in a URL ends up in every log that prints the URL, `reqwest::Error`'s
  `Display` included, so API calls never use it.
- **Encoding** (`encoding.rs`): RFC 3986 unreserved bytes pass, everything else is uppercase `%XX`, a space is `%20`.
  Keys are encoded per segment and only once (S3's rule; other AWS services encode twice), so the wire path IS the
  canonical path. Query pairs are sorted by encoded name then value; a valueless parameter is signed `uploads=` and sent
  bare (`?uploads`), the way the AWS SDKs do it.
- **Verified against AWS's published vectors** (`sigv4_test.rs`): GET Object, PUT Object, GET lifecycle, List Objects,
  and the presigned GET, byte for byte (cross-checked against `s3s-project/s3s` and `durch/rust-s3` on 2026-10-01; the
  AWS pages now redirect).

**Gotcha: a dot segment can't travel.** The `url` crate (and so `reqwest`) resolves `.` and `..` path segments, and
WHATWG counts `%2E%2E` as `..` too, so a key like `a/../b` would address `b`. `encode_key` refuses it with
`KeyError::DotSegment`; a delete must never land on a different object. Such keys exist only if another tool wrote them;
they can't be reached over HTTP through this stack.

## Providers

`ProviderProfile::from_preset` turns the connect form's preset into everything a request needs:

- **AWS**: `s3.<region>.amazonaws.com`, virtual-hosted. Put, Complete, and Copy all take `If-None-Match: *` (docs and
  live).
- **R2**: `<account>.r2.cloudflarestorage.com`, region `auto`, path style. Put and Complete take `If-None-Match`; Copy
  takes `cf-copy-destination-if-none-match` (it ignores `If-None-Match`). Keys composed NFC before they leave
  (`nfc_keys`), because R2 stores them NFC and an NFD key would otherwise collide with its twin while our own
  comparisons said they differ. Jurisdictional endpoints (EU, FedRAMP) aren't offered yet.
- **B2**: `s3.<region>.backblazeb2.com`, path style. No conditional writes (`501 NotImplemented` on all three, live).
- **Wasabi**: `s3.<region>.wasabisys.com`, path style (Wasabi's recommendation). Check-then-write: it ignores
  `If-None-Match` on all three writes (live).
- **Hetzner**: `<location>.your-objectstorage.com`, region = location, path style. Put takes `If-None-Match`; Complete
  and Copy check then write.
- **GCS**: `storage.googleapis.com` for every bucket, region `auto`, path style (a GCS bucket name may hold dots and
  underscores). GCS ignores `If-None-Match`; Put and Copy refuse an occupied key with its own create-only
  `x-goog-if-generation-match: 0` (`NoOverwrite::GoogGenerationMatch`), which GCS refuses beside any `x-amz-*` header
  (400 `ExcessHeaderValues`), so such a request goes out in GCS's own dialect (§ "Signing"). Complete checks then
  writes: GCS's completion ignores the precondition and its initiate refuses it (`400 NotImplemented`). No
  `UploadPartCopy` (`copies_in_parts` is false; § "Server-side copy"). ❗ **GCS transcodes**: a read without
  `Accept-Encoding: gzip` gets a gzip-stored object decompressed (33 B for 51 stored, `Range` ignored, no
  `Content-Length`), so every GET and HEAD to GCS carries that header (`transcodes_gzip`), added AFTER signing: GCS's
  front end rewrites it before checking the signature (`SignatureDoesNotMatch` when signed). A HEAD there still sends no
  `Content-Length` for such an object, only `x-goog-stored-content-length`, which `Answer::object_length` falls back to
  (verified on GCS, live.sh and curl, 2026-10-02).
- **Spaces**: `<region>.digitaloceanspaces.com`, region = region, path style. Put takes `If-None-Match`; Complete and
  Copy check then write. `cross_bucket_copy()` is false: Spaces documents no cross-cluster copy, and two buckets of one
  region may sit on two clusters, so the builders refuse a cross-bucket copy with `BuildError::CrossBucketCopy` before
  sending it and `copy_on_server` answers `NotSupported`, so the engine streams it. `forbid_cross_bucket_copy` (testing
  only) makes a fixture behave that way.
- **Other**: the given `http(s)://host[:port]` (nothing after it), region default `us-east-1`, the path-style toggle as
  given except that an IP endpoint is always path style. Check-then-write, and a short tail folds (§ "Multipart").

**Addressing.** Path style everywhere but AWS: one host for every bucket means one connection pool and one TLS
certificate. On AWS (where path style is deprecated, no date set) a bucket that isn't a plain DNS label (3–63 of
`a–z 0–9 -`, no dots) still goes by path: a dot breaks the `*.s3.<region>.amazonaws.com` wildcard certificate.

**Host parts are validated.** A region, location, or account ID must be `a–z 0–9 -`, so a typed `x.evil.com/` can't
redirect requests (and the signature) to another host.

**An AWS or Wasabi account root routes each bucket to its own region (`routing.rs`).** One account holds buckets in many
regions, and a request sent to (and signed for) the wrong one answers `301 PermanentRedirect` (`307` while a new
bucket's DNS settles; `400 AuthorizationHeaderMalformed` when only the signature's region is off). The client keeps a
per-bucket region map (`BucketRegions`, dies with the client) and sends a known bucket's requests to that region's
endpoint, signed for it (`ProviderProfile::reroute`: only the host changes, built from the preset's own `RegionalHost`
template, `s3.<region>.amazonaws.com` or `s3.<region>.wasabisys.com`). It learns from `ListBuckets`' `BucketRegion`,
from `x-amz-bucket-region` on any answer, and from a 400's `<Region>`; a misrouted answer goes once more, to the named
region. One that names none (a bodyless redirect) asks `HeadBucket`, which carries the header on every status. An
upload's body can't be sent twice, so `upload` asks `HeadBucket` first when nothing has named the bucket yet; a share
link does the same so it isn't signed for the wrong region. So the cost is at most one redirect or one `HeadBucket` per
bucket per session, which the cost estimate ignores.

- ❗ **A bucket place doesn't route**: its connect probe's `WrongRegion` refusal names the region to use instead
  (`route_each_bucket` is called for the account root only).
- ❗ **An allowlist** (`ProviderProfile::routes_by_region`, set by a preset's `regional_host`): only a provider whose
  endpoint is per region AND whose answers name the bucket's region gets one, and without one nothing is ever re-routed
  (`only_the_routing_allowlist_reroutes…`, `routing_is_for_the_routing_allowlist_only`). Wasabi qualifies:
  `x-amz-bucket-region` on every answer (200s included), `<BucketRegion>` in `ListBuckets`, `<Region>` on a 400, and
  `GetBucketLocation` from any regional endpoint (verified on Wasabi `eu-central-1` / `eu-west-1` / `us-east-1`, live.sh
  and curl, 2026-10-02). Hetzner and Spaces don't: they keep each location's buckets apart (another location's root
  lists none, its bucket place is `NoSuchBucket`); B2's keys live in one region (another answers `KeysRejected`).
- **A server-side copy across Wasabi regions is refused** (`400 NotImplemented`, "Operation not supported across
  regions"), which `map_s3_error` reads as `NotSupported`, so the engine streams it. AWS copies across regions.
- **Verified live** (`live_connect_test.rs::live_connect_routes_each_bucket_to_its_region`: an AWS `eu-north-1` root
  reaching a `us-west-2` bucket and a Wasabi `eu-central-1` root reaching an `eu-west-1` one, listed upfront, learned
  from a redirect, asked before an upload, and a share link, 2026-10-02), and against a fake AWS
  (`transport_routing_test.rs`: one local server behind every `*.amazonaws.com` host) for the paths a live account can't
  force.

**A short body is refused only where we have evidence (`refuses_short_body`, an allowlist).** S3's contract is that a
PUT whose body ends before its `Content-Length` publishes nothing and keeps the old object; VersityGW breaks it and
stores what arrived (fixture README). Trusted, each on evidence:

- **AWS**: documents `IncompleteBody` (400): "You did not provide the number of bytes specified by the Content-Length
  HTTP header" (https://docs.aws.amazon.com/AmazonS3/latest/developerguide/ErrorResponses.html).
- **R2**: documents error 10013 / `IncompleteBody` (400): "Request body terminated before expected `Content-Length`"
  (https://developers.cloudflare.com/r2/api/error-codes/).
- **Every named preset, live** (`live_a_cut_off_put_publishes_nothing`, 2026-10-02; B2 and Wasabi in two runs each): a
  PUT promising 4 MiB and cut off after 2 MiB, over an object and on a free key, kept the original and published
  nothing, looked at 2 s and 17 s later. Neither B2 nor Wasabi documents it.

Only "Other" (VersityGW, MinIO, Garage, anything) is off it, Garage included though it refuses too (fixture README):
it's reached as "Other", and the list is per preset. Off the list, an overwrite of an existing object goes as a
multipart upload (§ "Overwrites in parts").

**Conditional writes are an allowlist, ❌ never a probe.** A server can ignore `If-None-Match: *` and answer 200 while
overwriting: Garage does on Put, Complete, and Copy, VersityGW on Copy (`apps/desktop/test/s3-servers/README.md`,
observed 2026-10-01), and so do Wasabi and GCS live. A success proves nothing, so only an operation seen enforcing
carries a header (AWS's three; R2's Put, Complete, and its Copy header; Hetzner's and Spaces' Put; GCS's Put and Copy,
by its generation precondition); every other cell is `CheckThenWrite`. Every entry is verified live (§ "Verified
providers"). ❗ **Check-then-write has a blind window**: two `CreateNew`s racing on one key can both pass their HEAD,
and the later write replaces the earlier (seen live on GCS before its precondition, B2, and Wasabi, about one round in
three, `live_hostile_races`). An accepted risk where the provider offers no enforced precondition (B2, Wasabi, every
provider's multipart completion but AWS's and R2's). A `501 NotImplemented` (`S3Error::is_not_implemented`) on an
allowlisted operation means the caller should call `ProviderProfile::downgrade(op)`: that one operation becomes
check-then-write for the session, and the first call logs. The cells are atomics because one profile serves every
concurrent operation.

## Verified providers

Live against all seven named presets on 2026-10-02 (`apps/desktop/test/s3-servers/live.sh`, every `live_` cell; R2,
Hetzner `nbg1`, GCS `us-central1` through HMAC keys, Spaces `fra1`, AWS `eu-north-1` plus a `us-west-2` bucket, B2
`eu-central-003`, Wasabi `eu-central-1`). Re-run it before changing an allowlist; an addition needs both directions seen
in two runs. Full per-cell findings: `docs/notes/s3/live-verification-2026-10.md`.

- **Everyone**: a cut-off PUT publishes nothing; a cut-off part racing an abort leaves no upload and no object; a part
  sent after an abort is `NoSuchUpload`; `DeleteObjects` takes 1,000 keys and refuses 1,001 (`MalformedXML`, GCS
  `InvalidMultiObjectDeleteRequest`, B2 `InvalidRequest`, Hetzner a bodyless 400; Wasabi takes it), so GCS needs no
  per-object fallback; a delimited `ListObjectsV2` echoes `encoding-type=url` and round-trips `a b+c.txt` and `x + y/`;
  `x-amz-meta-mtime` comes back verbatim (GCS adds `x-goog-metageneration` beside it); a seven-day presigned GET fetches
  unsigned; a part under 5 MiB before the last is `EntityTooSmall`; a wrong secret is `SignatureDoesNotMatch` on
  `ListObjectsV2`, and `HeadBucket` answers a bodyless 403. The SDKs' `x-amz-checksum-crc32` and `-crc64nvme` are
  accepted everywhere, and a WRONG crc32 is `BadDigest` on AWS, R2, B2, and Wasabi (Hetzner, GCS, and Spaces ignore it);
  we send none. Where one key reaches two buckets (AWS, B2, Wasabi, Hetzner), `CopyObject` and `UploadPartCopy` work
  between them.
- **A one-request `CopyObject` and its ETag** (`live_copy_pin_test.rs::live_copy_object_etags`, 2026-10-02, six
  providers, B2 not run): a single-part source's copy keeps its ETag everywhere (Spaces answers it unquoted). A
  multipart source's copy keeps its `-N` ETag on Hetzner and Spaces, and gets a fresh whole-object ETag on AWS, R2, and
  Wasabi. A wrong `x-amz-copy-source-if-match` is `412` on AWS, R2, GCS, and Hetzner, and ignored on Spaces and Wasabi.
  ❗ GCS answers `400 InvalidArgument` to a pin naming a multipart ETag, quoted or not, hence
  `refuses_multipart_copy_pin`. A create-only GCS copy goes in GCS's dialect, the pin spelled
  `x-goog-copy-source-if-match`: the right single-part pin lands, a wrong one is `412`, and a multipart ETag sent
  unpinned lands (`live_create_only_copy_with_a_source_pin`, R2, GCS, AWS, and Wasabi, 2026-10-02; Hetzner and Spaces
  not run, their test buckets were gone). These multipart results are why a lost answer is proven by the write token,
  not the ETag (§ "Server-side copy").
- **Connect refusals** (`live_connect_test.rs::live_connect_refusals`): a wrong secret or key id is `KeysRejected` on
  the bucket and the account root everywhere, except R2's wrong secret (`AccessDenied` / `BucketListRefused`: its
  bucket-scoped key gets `AccessDenied` on `ListBuckets` whatever the secret); a missing bucket is `NoSuchBucket`
  everywhere but R2 (`AccessDenied`); a bucket through another region's endpoint is `WrongRegion { region }` on AWS and
  Wasabi, `NoSuchBucket` on Hetzner and Spaces, `KeysRejected` on B2; a B2 key scoped to one bucket opens it, gets
  `AccessDenied` on another, and `BucketListRefused` on the account root.
- **Two gotchas the fixtures hid**: GCS answers `411` to a bodyless POST (`CreateMultipartUpload`), and R2, Hetzner, and
  GCS to a zero-byte PUT (a folder marker), when it carries no `Content-Length`; hyper sends none for an empty body, so
  `S3Client::send` adds `content-length: 0` (`transport_test.rs`).
- **HTTP versions**: Hetzner, GCS, and Spaces negotiate HTTP/2; R2, AWS, B2, and Wasabi offer HTTP/1.1 only (verified
  with `live_connect_speaks_http2_where_offered` and `curl --http2`, 2026-10-02). Every cell passes over HTTP/2 on the
  three that offer it.
- **R2**: `If-None-Match` enforced on Put and Complete, ignored on Copy, `cf-copy-destination-if-none-match` enforced;
  ❗ a last part LARGER than the rest is `InvalidPart` (equal parts and a smaller last part land), hence
  `ShortTail::Keep`; `UploadPartCopy` takes a 1 MiB source as the last part and enforces `x-amz-copy-source-if-match`;
  an NFD key reads back through its NFC twin and one PUT replaces the other. With a bucket-scoped key: `ListBuckets` is
  `AccessDenied` (so the account root is `BucketListRefused`, wrong secret or not), a missing bucket is a 403 (connect
  says `AccessDenied`, never `NoSuchBucket`). `ListMultipartUploads` under a prefix lists an unfinished upload there (a
  SIGKILLed child's, in every `live_hostile_crash_recovery` run, 2026-10-02), so an abort's confirming listing is a real
  confirmation on R2 too; a second abort of an aborted upload answers a success, so only a listing or a refused part
  proves it gone. R2's default lifecycle rule aborts unfinished uploads after seven days.
- **Hetzner**: `If-None-Match` enforced on Put only; parts of any sizes land; `UploadPartCopy` ignores
  `x-amz-copy-source-if-match`; ❗ `CopyObject` and `UploadPartCopy` work between two buckets of one location (the
  research note's "within one bucket only" didn't hold); the key may `ListBuckets`; NFD and NFC are two objects.
- **GCS**: `If-None-Match` ignored on all three writes; ❗ `x-goog-if-generation-match: 0` signed in GCS's dialect is
  412 over an occupied key (old bytes kept) and 200 on a free one, on Put and Copy, in two runs, ignored on Complete and
  refused on the multipart initiate; with it, two racing one-PUT `CreateNew`s leave one `AlreadyExists` every round (six
  of six); ❗ no `UploadPartCopy` (400 `NotImplemented`), so copies go whole (a 140 MiB `CopyObject` took about 1.5 s);
  multipart uploads of any part sizes land; `DeleteObjects` works; NFD and NFC are two objects; `ListBuckets` refused
  for a bucket-scoped service account.
- **Spaces**: `If-None-Match` enforced on Put only; parts of any sizes land; `UploadPartCopy` ignores
  `x-amz-copy-source-if-match`; NFD and NFC are two objects; the key is bucket-scoped (`ListBuckets` and `CreateBucket`
  refused), so a cross-bucket copy is unverified and stays off (§ "Providers").
- **AWS**: every doc-based entry held: `If-None-Match` enforced on Put, Complete, and Copy; `x-amz-copy-source-if-match`
  enforced (412); parts of any sizes land; `UploadPartCopy` takes a 1 MiB source as the last part; copies work across
  buckets and regions; NFD and NFC are two objects; a second abort answers 204. `ListBuckets` names each bucket's
  region, and region routing works end to end (§ "Providers").
- **B2**: `501 NotImplemented` to `If-None-Match` on all three writes, old object kept; ❗ `x-amz-copy-source-if-match`
  enforced (412 on a stale pin, the current one copies, two runs), hence `enforces_copy_source_pin`; parts of any sizes
  land; copies work across buckets; NFD and NFC are two objects; a second abort is `NoSuchUpload`. The master key
  doesn't work on the S3 API: an application key does.
- **Wasabi**: `If-None-Match` ignored on all three writes (200 and overwritten); `x-amz-copy-source-if-match` ignored;
  parts of any sizes land; copies work across buckets of one region and are refused across regions; `DeleteObjects`
  takes 1,001 keys; NFD and NFC are two objects. `ListBuckets` names each bucket's region, and an account root routes
  each bucket to it (§ "Providers").
- **Throughput** (`live_throughput_by_part_width`, a ~250 Mbit/s uplink from Stockholm): a 140 MiB server-side copy in 8
  MiB parts at 4 / 8 / 16 in flight took R2 3.3 / 2.6 / 1.5 s, Hetzner 1.2 / 1.0 / 0.6 s, Spaces 0.7 / 0.5 / 0.4 s, with
  no throttle surfacing as an error (AIMD halvings aren't counted), so 16 stays everyone's copy width. A 64 MiB upload
  at 2 / 4 / 8 parts ran 24 / 26 / 23 MiB/s on R2, 30 / 31 / 31 on Hetzner, 27 / 27 / 21 on Spaces, and 7 / 13 / 18 on
  GCS (far away, so latency-bound); four stays the upload width, since eight 64 MiB buffers is 512 MiB. AWS, B2, and
  Wasabi (2026-10-02, with two other live runners sharing the link, so skewed) agree: copies at 16 in flight were the
  fastest or level, and a 64 MiB upload at 2 / 4 / 8 parts ran 22 / 24 / 24 MiB/s on AWS.
- **Hetzner reached as "Other"** (`live_an_overwrite_off_the_allowlist_goes_in_one_part`, 2026-10-02): an overwrite goes
  as a one-part multipart upload (ETag `…-1`); one cancelled right before its completion kept the original byte for byte
  and left no upload.
- **Names** (`live_hostile_names_round_trip`, 2026-10-02): NFC and NFD, emoji, RTL, leading and trailing spaces, a
  trailing dot, `...`, `%`, `+`, `#`, `?`, `&`, `\`, quotes, and a 1,024-byte key round-trip everywhere. ❗ GCS refuses
  a key holding CR or LF (`400 InvalidObjectName`; bodyless to a HEAD), and B2 any control character, a tab included
  (`400 InvalidRequest`, B2's catch-all, so the code can't say why). So the profile carries them
  (`ProviderProfile::refused_key_chars`) and every write path (upload, New File, New Folder, a rename's or a server
  copy's destination) answers `VolumeError::InvalidName` before a request goes out (`writes.rs::refuse_unstorable`,
  `refused_name_test.rs`); GCS's `InvalidObjectName` maps to it too, as a backstop. Every hostile cell's findings
  (names, sizes, cancels, crashes, races, scale): `docs/notes/s3/live-hostile-2026-10.md`.
- **Unverified, and why**: a cross-bucket copy on R2, GCS, and Spaces (each key reaches one bucket).
- ❗ **A copy's ETag pin is ignored on Hetzner, Spaces, and Wasabi**, so a source replaced mid-copy could be stitched
  from two versions there; AWS, R2, and B2 refuse the part. Hence `enforces_copy_source_pin` (AWS, R2, B2) and the HEAD
  before the completion everywhere else (§ "Server-side copy").

## Writing

**Every write goes to its final key, and the transfer engine stages nothing here.** S3 publishes an object only when its
PUT or `CompleteMultipartUpload` finishes, and a replaced object stays readable until then, so the volume answers
`publishes_writes_whole` and the engine writes final keys and takes a file→file Overwrite in place
(`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Whole-publish destinations").
Decision/Why: a `.cmdr-tmp-*` temp would cost a landing rename, which on S3 is a server-side copy plus a delete (twice
the requests, a single copy fails past 5 GB), and buys nothing the protocol doesn't already give. `write_is_single_shot`
stays `false`: the server holds nothing until the request's last byte, yet a multipart upload spans many requests.

- **Shape** (`writes.rs::shape_for`): one PUT when `plan_parts` gives a single part (so up to 69 MiB, a 64 MiB part plus
  a folded tail), a multipart upload otherwise. A stream of unknown length goes in parts of the floor size (64 MiB, so
  at most 625 GiB), and one that ends inside its first part goes out as one PUT from the buffer.
- **Every request body is in memory** (`upload_body.rs::buffered_body`): a PUT reads its whole object first
  (`writes.rs::read_whole`), a part its part. So a source that ends short, runs long, or fails ends the write before a
  byte goes out (S3 never sees a truncated prefix), and any request can be sent again whole (a pause, a part's retry).
  Peak memory per one-PUT write is its object, at most 69 MiB, within the four part buffers a multipart upload holds.
  Decision/Why not stream a PUT straight from the source (one piece of read-ahead): a pause longer than R2's ~15 s idle
  limit fails such a PUT, and its bytes can't be sent again because the source can't be re-read (§ "Pause"). The cost is
  the overlap of source read and upload inside one file, which matters only for a slow source, and the engine's
  concurrent files hide most of it.
- ❗ **A PUT's last piece waits for a go-ahead** from the upload, which asks the progress callback, where a Cancel
  arrives: with only the 200 ms tick, a cancel landing between ticks lost to a fast finish and published the object
  (found by the shared `a_cancelled_upload_leaves_nothing_behind` scenario). ❗ A cancel after the last piece is
  released is too late to stop the publish: the PUT waits for its answer and reports the file it finished. Dropping the
  request there reported `Cancelled` over an already replaced object, and the cut-off cleanup then found our token on it
  and deleted it, losing the original AND the new bytes (verified on R2, `live_hostile_cancel_uploads`, 2026-10-02;
  pinned by `late_cancel_test.rs`, a fake S3 that commits then answers slowly). An empty body has no last piece, so its
  only Cancel check is before the request.
- ❗ **A cut-off PUT is cleaned up after**, because not every server keeps S3's promise to publish nothing short of
  `Content-Length`: VersityGW stores whatever arrived before the connection dropped (fixture README). Every PUT carries
  a token of its own (`x-amz-meta-cmdr-write`, `metadata::write_token`), and a PUT that was cancelled or cut off HEADs
  its key (again after 150 and 300 ms, since the server stores the body only once it notices the drop) and deletes the
  object ONLY when it carries that token AND is short (`writes.rs::settle_cut_off_put`): anything else there is the
  original or another writer's. ❗ Ours at the full size is the write having LANDED (the server published, then the link
  died before the answer), so the write reports it as written; deleting it there lost the original and the new bytes
  alike on an overwrite. A failed `CompleteMultipartUpload` asks `landed_whole` (one HEAD, never a delete) before
  calling the write failed (`late_cancel_test.rs`, a fake that commits and then hangs up). That covers a write to a FREE
  name; an overwrite of an existing object on a provider not trusted to refuse a short body never goes as a PUT at all
  (§ "Overwrites in parts"). The token is visible as user metadata and harmless to other tools.
- **Multipart** (`multipart_upload.rs`): up to the profile's `upload_concurrency()` (4) parts in flight, and a part is
  read from the source only when a slot is free, so at most four part buffers exist (256 MiB at the floor). Parts are
  buffered at all because a failed one is sent again after 1, 2, then 4 s on a throttle (`SlowDown`, 503, 429), a server
  fault, or a transport failure. The source is read with progress and cancel still answered after every piece and every
  200 ms tick while one is pending (`PartReader::pull_with`, under both the part fill and the look-ahead past the last
  part), so a source stalled mid-part or right at its end can't hold a Cancel; a one-PUT write fills the same way
  (`read_whole`). ❌ a `next_chunk` is dropped half-read only when that ends the upload. A known length is a promise: a
  part that comes up short, or bytes left after the last part, fail the upload. Cancel is checked once more right before
  `CompleteMultipartUpload`, which is what publishes.
- **Verification**: a HEAD after every write (`verify_landing`, `judge_landing`) compares the size and the ETag with
  what the write answered. It costs one cheap request per file and feeds the pane patch that follows (`take_written`),
  so `notify_mutation` doesn't pay a second one. ETags aren't compared with an MD5 of the bytes: under SSE-KMS and for
  multipart they aren't one.
- **A throttled or faulted single PUT goes again** after 1 s, then 2 s (`MAX_PUT_RETRIES`), its body still in memory. B2
  answered two of about 1,200 PUTs in one live run with `500 InternalError` ("internal incident"), each failing a file a
  resend would have landed (verified on B2 `eu-central-003`, `live-engine.sh b2`, 2026-10-03). A fault can still have
  published, and the resend's no-overwrite check would then refuse our own object, so before each resend `landed_whole`
  asks whether ours is at the key whole (one HEAD, only on a fault). Pinned by `put_retry_test.rs`.

## Pause

The engine's pause parks the source stream between chunks, which stops a backend that sends each chunk as it reads it.
An upload here buffers first (§ "Writing"), and a local source drains into those buffers in milliseconds, so that park
never reached the bytes in flight: a 75 MB upload to R2 or GCS paused at 20% ran to 100% before it showed as paused
(David's QA, 2026-10-02). So the write takes the operation's own Cancel and pause from the source,
`VolumeReadStream::stop_signal` (a `cmdr_fs::volume::ScanStop`), and honours it per request (`upload_body.rs`
`PauseHold`, carried on `writes.rs::Progress`):

- **No request starts while paused**: a PUT, each part attempt, and `CompleteMultipartUpload` park first (and answer
  `Cancelled` if the operation stopped instead).
- **A body in flight checks before every 1 MiB piece.** A pause shorter than `writes::PAUSE_HOLD` (5 s) stalls the
  request and costs nothing. One that outlasts it drops the request (`Halt::SetAside`) and sends it again whole on
  resume: a part from its buffer without counting an attempt, a PUT from its object after settling the cut-off one
  (`settle_cut_off_put`, again right before the resend: a server that keeps cut-off bodies may store ours only after
  draining the dropped connection, and left there it would make the resend's own `If-None-Match: *` refuse the name). A
  cancel while parked ends the write as `Cancelled`, the multipart upload aborted.
- **Decision/Why the hold, rather than stalling the request for the whole pause or dropping it at once.** A stalled body
  dies at the provider's idle timeout: R2 dropped a PUT whose body went silent after about 15 s, AWS answered
  `400 RequestTimeout` after about 55 s, and GCS waited out 200 s (verified live, a 2 MiB PUT stalled 10–200 s mid-body,
  2026-10-03), so a long pause would turn into a failed file. Dropping at once would resend up to four 64 MiB parts
  after every brief pause. The hold keeps a short pause free and stays well under R2's limit.
- **What a long pause costs**: the in-flight parts' bytes (up to four parts) or the PUT's bytes go again, plus one
  billed request each. The upload ID stays valid, so the parts already done stay done.

Pinned by `pause_test.rs` (a fake S3 reading at a slow rate: parts and a PUT paused before they go out and mid-body,
resumed whole, and a cancel while paused) and `upload_body_test.rs` (the hold on a paused clock). The engine side:
`write_operations/transfer/DETAILS.md` § "Pause reaches between chunks".

## Overwrites in parts

`writes.rs::overwrites_in_parts`. Decision (the safer option, David's): on a provider off the `refuses_short_body`
allowlist, a `CreateOrReplace` write whose key holds an object (one HEAD to find out) never goes as a PUT, because a PUT
there that's cancelled or cut off publishes the truncated bytes over the original (VersityGW does: the fixture cell
`write_test.rs::a_cancelled_overwrite_keeps_the_original` failed that way before this path existed). It goes as a
multipart upload instead, a one-part one for a file that would have been one PUT (`PartPlan::whole`; S3 takes an only
part of any size), and a stream of unknown length that ends inside its first part does the same
(`ShortStream::OnePart`).

- **Why it's safe**: S3 publishes a multipart object only at `CompleteMultipartUpload`, a request with no body to cut
  short. A cancel or a failure before it aborts the upload (the ledger and the sweep cover a crash, § "Unfinished
  uploads"), and the original stays whole. The cancel check right before the completion is the last moment a Cancel
  lands.
- **An empty file stays one PUT**: it has no body to cut short.
- **Cost**: the HEAD, plus Create and Complete beside the one part (`Workload::upload_over`). Nothing is written beside
  the key, so a minimum storage duration bills nothing extra. Decision/Why not a temp key copied over the original: it
  costs a copy and a delete on every overwrite, and on Wasabi 90 days of the temp's bytes (its minimum storage
  duration). A write to a FREE name keeps going as one PUT (there's no original to lose; a cut-off one is removed by its
  token).
- **Progress** stays at zero while the one part fills from the source, then moves as it goes out. A source that stalls
  mid-part or at its end still answers Cancel every progress tick (`PartReader::pull_with`).

## No-overwrite writes

`ops::put_object`, `complete_multipart_upload`, and `copy_object` take `Overwrite::{Replace, Refuse}` and return
`Built { request, check_first }`. `check_first` is `true` exactly when `Refuse` was asked and the profile has no header
for that operation right now. Making the flag part of the return type is what keeps the check from being forgotten.

- **The header path** (AWS's Put and Complete, R2's Put): the server refuses atomically, 412 → `AlreadyExists`
  (`map_s3_error`, since the only precondition Cmdr sends is a no-overwrite one). A 501 downgrades that operation for
  the session; a refused Complete is sent again the other way (the parts are still there), a refused PUT fails that
  file.
- **Check-then-write** (everything else, both fixtures included): a HEAD before the write, again right before
  `CompleteMultipartUpload` (catching a writer that took the name during a long upload), and the HEAD after the write:
  another writer's ETag there under `CreateNew` is `AlreadyExists`, their object kept, ours gone. ❗ One window stays
  blind: a writer whose object lands between our last check and our own write's completion is overwritten by ours, and
  nothing short of bucket versioning can see it. That's the residual risk the product decision accepts ("tell the user
  plainly when we notice").
- **A fresh folder skips the check-then-write HEADs** (`WriteMode::CreateNewInFreshFolder`, `refuse_if_taken`): the
  engine's merge walker hands it down for every name in a folder its own `create_directory` just made, which that
  creation proved empty (a rename to a new name, or a copy of a folder into a place without one). The header paths still
  apply (they cost nothing); the HEAD before the write, and before a multipart completion, doesn't go. A one-request
  server-side copy (`server_copy.rs::copy_whole`) also skips its verifying HEAD; an upload and a copy in parts keep
  theirs. A copy whose answer is lost still HEADs (`landed_whole`), but only then. Decision/Why: one HEAD per object was
  a third of a folder rename's requests (1,005 of 3,021 for 1,005 objects on B2's check-then-write profile, the requests
  behind its daily Class B cap) and protects nothing the folder's creation didn't. ❗ The accepted window: a file
  another writer puts in the brand-new folder while the operation runs is overwritten, where the HEAD would have refused
  it if it had landed before that HEAD. ❗ The verify HEAD a fresh-folder copy skips detects only that same writer
  (another object at a name in a folder that didn't exist moments ago), and only when it lands between our copy and that
  HEAD, so skipping it widens nothing the accepted window doesn't already cover. B2's 1,005-object rename to a new
  folder: about 1,005 HEADs (the sources') plus LISTs. A merge into a folder that already existed keeps every check:
  that's where another writer's files plausibly are. Pinned by `fresh_folder_test.rs` over `fake_s3.rs` and the app's
  `a_folder_of_1005_objects_renames_through_the_engine` (at most one HEAD per object).
- **Garage ends an upload when another write replaces its object** (`NoSuchUpload` on the next part or the completion;
  `apps/desktop/test/s3-servers/README.md`). Under `CreateNew` that's read as the name being taken, after a HEAD
  confirms it, ❌ never as the destination "not found".
- **`create_file`** (New File) also refuses a name a FOLDER holds: an object beside a same-named prefix would hide under
  the folder in every listing. `write_from_stream` leaves that check to the engine's destination pre-check, which reads
  the listing anyway, to save a request per file.

## Unfinished uploads

S3 keeps an unfinished multipart upload's parts forever, invisible in every listing and billed. Cancel and every failure
abort it on the spot; what an abort can't reach (a crash, a dropped future, a server gone mid-abort) is swept later.

- **The record** (`upload_ledger.rs`): `<state dir>/unfinished-uploads` under `VolumeHost::state_dir("s3")`, one line
  per event (`+` when `CreateMultipartUpload` answers, `-` once completed or aborted), every field percent-encoded,
  rewritten to the open records whenever a sweep reads it. A process-wide registry marks uploads running in THIS
  process, and a guard marks a dropped upload abandoned. Without a state directory (a test host) the record lives for
  the session. ❗ Being process-wide, a fixture cell asks it only about its own scratch prefix (`open_under`), or
  another cell's open record of the same account fails it.
- **Decision/Why not the operation log**: the operation log is the durable journal of what happened to the USER's files,
  for undo and search, and its rows are paths a rollback can act on. An unfinished upload is protocol state that only
  this crate can act on (`AbortMultipartUpload`), keyed by an account and an upload ID; putting it there would mean a
  schema migration, a new row kind no rollback understands, and the app reaching into S3 vocabulary. A file this crate
  owns, in a directory the host hands every backend, keeps it where the knowledge is.
- **An abort is confirmed by listing** (`abort_upload`): a part request cut off just before an abort can land after it
  and bring the upload back (VersityGW does; AWS documents the race), so each round aborts and then lists the key's
  uploads, up to four rounds 200 ms apart, and only a listing without the upload ID forgets the record. Every preset's
  listing shows its unfinished uploads, R2 included (§ "Verified providers"); nothing special-cases a provider here.
- **The sweep** (`S3VolumeInner::sweep_unfinished_uploads`) runs in the background at every connect and after a
  reconnect, and aborts the account's open records that no task in this process is running. A record the server confirms
  gone (aborted now or already) is forgotten; any other answer keeps it for the next connect. ❌ It never aborts an
  upload ID it didn't record, and never lists the server's uploads to decide: another tool's upload may be live.
- ❗ **"In flight" is per process, so the sweep relies on one Cmdr per data dir** (`instance_lock.rs` in the app). A
  second LIVE process on the same record has its running upload aborted by the first one's sweep ("the server ended the
  upload"), while an upload a SIGKILLed process left is swept as designed (verified on R2, GCS, and Spaces,
  `live_hostile_crash_recovery`, 2026-10-02). Don't share a state dir between processes.

## Folders, delete, and rename

`mutation.rs`. A folder is a prefix: it exists when it has a zero-byte `name/` marker OR any key under it, and ❗ a
folder wins over an object of the same name (`NameHolds`), except where the listing showed the file as `<name> (file)`
(§ "Listing and stat"): then each row names only its own holder (`paths::Holder`).

- **`create_directory`** writes the marker, refusing a taken name (`AlreadyExists`), a missing parent (`NotFound`), and
  a FILE holding the parent's name (`NotADirectory`: a marker under it would turn that file into a folder in every
  listing). That's `mkdir`'s contract, so the shared `cmdr_fs::volume::mkdir_all` walk runs unchanged and refuses a file
  in the way at any depth; every level it creates gets a marker, so a `mkdir -p` folder survives emptying.
- **`delete`** reads one listing of `name/` capped at two keys (`listing::folder_contents`): anything but the marker is
  `ENOTEMPTY`, the marker alone deletes the marker, nothing at all falls back to a HEAD and deletes the object (or
  answers `NotFound`). A LIST per delete is a class-A request. ❗ Every such "what's under it?" listing (delete, stat's
  fallback, `rename_work`, `tally_subtree`, a folder listing) is skipped for a prefix past the 1,024-byte key ceiling
  (`listing::can_hold_keys`): `<key>/` for a key at the ceiling can't match anything, and B2 refuses it with
  `400 InvalidRequest` rather than an empty page, which made such an object undeletable there (verified on B2,
  `live_hostile_names_round_trip`, 2026-10-02; pinned by `long_key_test.rs` over `fake_s3.rs`).
- **`delete_files`** (`batch.rs`) is the batch a move's source sweep sends per folder level, and a volume delete per
  1,000 files (`delete_batch_size`): `DeleteObjects`, 1,000 keys a request with `Content-MD5`, quiet, its body parsed
  even on 200, each failed key reported against its own path (matched NFC on R2). ❗ By key, with no folder check: the
  trait's contract is files the caller just listed. A throttle, a server fault, or a failed connection sends the batch
  again after 1 s and 2 s (`ask_again_on_a_blip`; the request is idempotent), pinned by `batch_retry_test.rs` over
  `fake_s3.rs`.
- **`rename`** moves one file of up to the part floor (64 MiB, `copies_whole`): `CopyObject` keeping the metadata (so
  the mtime survives), a HEAD proving the copy is ours, then the source's delete, so the worst a failure leaves is two
  copies. `force: false` refuses a taken name first.
- **`rename_work`** answers `CopyThenDelete` for a folder (any key under the prefix) and for an object past the part
  floor, so every caller sends those through the app's engine; `rename` itself still answers them `NotSupported`, so
  nothing copies a folder by accident. One capped listing, plus a HEAD for an object.
- **`tally_subtree`** (`batch.rs`) counts objects under a prefix with a recursive listing, a thousand keys a request,
  stopping one past the cap; folder markers aren't files. F2 asks it how big a rename would be, and prices the rename
  from what it kept: each object's size and `LastModified` (the upload time early deletion bills from, ❌ never
  `x-amz-meta-mtime`), and the folders the keys name.

## Server-side copy

`server_copy.rs`. `copy_on_server` copies from this place or a sibling place of the SAME account (the endpoint and key
id, matched on the concrete `S3Volume` the source downcasts to); another account or another backend is `NotSupported`,
and so is a cross-bucket copy where the provider copies within one bucket only (Spaces). The engine then streams.

- **The source is HEADed once**: its size picks the shape, its ETag pins every part (`x-amz-copy-source-if-match`, so an
  object replaced mid-copy fails the part rather than stitching two versions; R2 enforces it, Hetzner and Spaces ignore
  it, § "Verified providers"), and its metadata travels. A one-request `CopyObject` is pinned the same way: a `412` is
  `SourceChanged`, unless the copy also carried a no-overwrite header (R2's, GCS's generation match), where one HEAD of
  the source tells the two apart (`source_moved_on`). ❗ On GCS a multipart-ETag source goes unpinned
  (`refuses_multipart_copy_pin`). The window that leaves: a source replaced between its HEAD and the copy lands as the
  new version, carrying the old version's restated date and headers. The verify HEAD catches a size change outside a
  fresh folder; inside one nothing does (`fake_s3.rs`'s `replace_after_head` pins the pinned case).
- ❗ **A source replaced mid-copy is `VolumeError::SourceChanged`**, and nothing is published: a part refused by the pin
  (a 412, which on a part copy is the pin, ❌ never `AlreadyExists`), a part past the source's new end (`InvalidRange`,
  416: every range comes from the HEAD, so a smaller replacement on a provider ignoring the pin shows this way; verified
  on Spaces, `live_hostile_races`, 2026-10-02), or, off the `enforces_copy_source_pin` allowlist, a second HEAD right
  before the completion that finds another ETag (`source_unchanged`). The upload is aborted, and the engine never
  deletes that source (a move's delete follows only a finished copy), so the new version survives. One HEAD per
  multipart copy; a replacement between that HEAD and the completion stays blind. A single `CopyObject` is atomic and
  needs none. Both fixtures enforce the pin, so `copy_test.rs` covers the 412 path between parts and the HEAD path with
  a replacement after the last part.
- **Up to the part floor, one `CopyObject`**; past it, a multipart upload of `UploadPartCopy` ranges with the upload
  plan's part size and the provider's tail rule (§ "Multipart"), even under 5 GB, so progress moves per part and a pause
  lands between parts. ❗ **GCS has no `UploadPartCopy`** (`copies_in_parts` is false): there a copy of any size is one
  `CopyObject`, and past S3's 5 GiB ceiling (`MAX_COPY_OBJECT_SIZE`) `copy_on_server` answers `NotSupported` so the
  engine streams it.
- **Up to the profile's `copy_concurrency()` (16) parts in flight**, with AIMD on the window (`Window`): halved on a
  throttle (`S3Error::is_throttle`: `SlowDown`, 503, 429, Wasabi's and R2's codes), one wider per landed part. A
  throttled or faulted part goes again after 1, 2, then 4 s.
- **A pause lands at `ServerCopyProgress::checkpoint`**, asked before creating the upload and before each part; while it
  waits, the parts in flight keep being driven to completion. A cancel aborts the upload through the ledger's
  listed-until-gone abort, and the source is never touched.
- ❗ **Every 200 is parsed**: `CopyObjectResult` and `CopyPartResult` can carry an `<Error>`.
- **No-overwrite**: `CopyObject` takes R2's `cf-copy-destination-if-none-match`, else a HEAD first (VersityGW and Garage
  ignore `If-None-Match` on a copy, fixture README); a multipart copy refuses at its completion, as an upload does. A
  HEAD after every copy verifies it.
- **The date survives**: every server-side copy restates the source's metadata (`REPLACE`): its `x-amz-meta-mtime`, else
  its `Last-Modified` as the mtime, its content headers (`Content-Type`, `Content-Encoding`, `Cache-Control`,
  `Content-Disposition`, `Content-Language`), and its other user metadata (both fixtures honour `REPLACE`,
  `copy_test.rs`). A multipart copy names the same metadata at its creation. Lost by restating: `Expires` and a website
  redirect, which nothing Cmdr writes uses.
- ❗ **Every server-side copy carries its own write token** (`x-amz-meta-cmdr-write`, never the source's), so a copy
  whose answer is lost asks `landed_whole` (one HEAD, never a delete): our token at the source's size means the server
  applied it, and the copy reports it. For a `CompleteMultipartUpload` that's any failed completion; for a one-request
  `CopyObject`, a transport failure or a server fault. Size and ETag shape alone can't prove it (an earlier identical
  copy matches both). No token, any other size, or no object: the copy fails (a multipart one aborts its upload), which
  is safe because a move keeps its source. Decision/Why the one-request copy too: before it, a `CopyObject` whose answer
  was lost failed, the engine fell back to streaming the file, and the streamed write's no-overwrite check refused the
  name the copy itself had taken, so a rename stopped with `DestinationExists` on a fresh key (live, Hetzner, once in
  six 1,005-object renames, 2026-10-02). Restating instead of `COPY` costs no request: the source HEAD it reads from is
  sent either way. Pinned by `late_cancel_test.rs` (parts) and `copy_landed_test.rs` (one request), both over
  `fake_s3.rs`.

## Responses

- **A buffered answer is capped.** `transport/answer.rs::read_capped` reads an `Answer`'s body up to `MAX_ANSWER_BODY`
  (16 MiB, against a ~1 MiB 1,000-key listing page) and refuses an announced or streamed overrun as a typed
  `ExchangeError::BodyTooLarge`, so a hostile S3-compatible endpoint can't exhaust memory. A data read (`Opened`)
  streams and is never buffered.
- **The parsers are fuzzed.** The `fuzzing` feature exposes `fuzzing::response_body`, which feeds one body to every
  response parser in `xml/`; the `s3_xml` target drives it (`fuzz/DETAILS.md`).
- **The element tree** (`xml/mod.rs`): bodies are small, so each is read into a tree first, matched by local name (AWS
  uses a default namespace, some servers none). Text is kept untrimmed because keys may begin or end with spaces;
  `Element::value` trims for numbers, dates, and tokens. Entities resolve through `GeneralRef` (quick-xml 0.41 splits
  them out of text nodes; `crates/cmdr-webdav/src/propfind.rs` has the evidence). A 32-level depth cap keeps a hostile
  body from building a tree whose recursive drop overflows the stack.
- **Every parser checks the root.** An `<Error>` root inside a 2xx is `BodyError::Embedded`, boxed to keep `Result`
  small. AWS documents this for Complete and CopyObject; UploadPartCopy and DeleteObjects get the same check because it
  costs nothing.
- **URL-encoded listings.** ListObjectsV2 and ListMultipartUploads ask for `encoding-type=url` (XML 1.0 can't carry some
  characters a key may hold), and the parser decodes only when the response echoes `<EncodingType>url</EncodingType>`.
  AWS writes a space as `+` and a plus as `%2B`, so `+` becomes a space before percent-decoding (botocore's
  `unquote_plus`). A server that encodes a plus as a literal `+` would break this; `integration_test.rs` lists keys
  holding both (`a b+c.txt`, `x + y/`) and both fixtures pass (verified on VersityGW v1.8.0 and Garage v2.4.1,
  2026-10-01).
- **Archived objects.** `StorageClass::is_archived` is true for `GLACIER` and `DEEP_ARCHIVE` (Glacier Instant Retrieval
  reads on demand, so it isn't). A listing child carries it as `FileEntry::in_cold_storage`, which the pane shows as an
  "archived" glyph; a stat also reads HEAD's `x-amz-storage-class` and `x-amz-archive-status` (`cold_from_head`), the
  only place Intelligent-Tiering's archive tiers show. A read of any of them answers `InvalidObjectState`, which
  `map_s3_error` turns into `VolumeError::ColdStorage(path)` by the code alone; the copy dialog, the listing error pane
  (an archived zip, browsed), and the viewer each word it from that typed variant. A restored object still lists as
  `GLACIER`, so it keeps the glyph and reads fine. The internals say "cold storage" because "archive" already means a
  zip here; the UI says "archived". Restore is a later milestone. Neither fixture has storage classes, so this is
  unit-tested only (`query_test.rs`, `errors_test.rs`).

## Request bodies

`CompleteMultipartUpload` carries explicit part numbers. `Delete` is always quiet (only failures come back) and carries
`Content-MD5`, which AWS still requires. Both escape CR and LF as `&#13;` / `&#10;`: a parser normalizes a literal CR or
CRLF to LF, so a key holding one would otherwise name a different object (AWS's object-key naming guide asks for exactly
this).

## Errors

`S3Error { status, code, message, request_id, region, endpoint }`. `code` is an `S3ErrorCode` variant for every code
some path acts on, `Other(String)` for the rest, and `NoBody` when there's no `<Error>` (HEAD, a proxy's HTML page). The
predicates (`is_not_found`, `is_precondition_failed`, `is_not_implemented`, `is_retryable`) use the code, falling back
to the status only for `NoBody` (and `Other`, for retryability). `region` (on `AuthorizationHeaderMalformed`) and
`endpoint` (on `PermanentRedirect`) are the routing hints a connect probe can use to find a bucket's real region.

## Multipart

`plan_parts(total)`: one part size for the whole upload, at least 64 MiB (fewer billed requests than S3's 5 MiB floor)
and at least `total / 10,000`, rounded up to a whole MiB. Past 10,000 × 5 GiB it's `TooLarge`. Equal parts are an R2
requirement (`InvalidPart` at completion otherwise); doing it everywhere also means a re-sent part covers the same
bytes. **A tail under 5 MiB follows the provider's `ShortTail`** (`ProviderProfile::short_tail`), because two servers
refuse opposite shapes: R2 answers `InvalidPart` to a last part LARGER than the rest (live, 2026-10-02), and Garage
refuses an `UploadPartCopy` source under 5 MiB even as the last part (fixture README). So every preset keeps the tail as
its own smaller part (`Keep`, S3's own rule), and "Other", which may be Garage, folds it into the part before (`Fold`),
skipped when that part would pass 5 GiB, which only happens near the 48.8 TiB ceiling. Whether an upload goes as one PUT
is decided with the tail folded either way (`shape_for`), so 65 MiB stays one PUT everywhere. `PartPlan::range` gives
each part's inclusive byte range for `x-amz-copy-source-range` or a ranged read, the last one running to the end of the
object.

**Spec correction: no marker can find our own unfinished uploads.** The plan says the startup sweep matches unfinished
uploads "by a Cmdr marker in the initiation metadata". `ListMultipartUploads` returns only key, upload ID, initiator,
storage class, and initiation time, and no API reads an in-progress upload's metadata. The sweep has to work from upload
IDs Cmdr recorded locally when it started each upload, or abort every upload older than some age, which would also abort
other tools' uploads. The first is what ships: § "Unfinished uploads".

## Metadata

The source file's mtime goes in `x-amz-meta-mtime` in rclone's format: Unix seconds as a fixed-point decimal with up to
nine fractional digits and trailing zeros dropped (`1354040105.123456789`), negative before the epoch. That's
`swift.TimeToFloatString` from `ncw/swift`'s `meta.go`, which rclone's S3 backend uses for its `mtime` key (read on
2026-10-01). `parse_mtime` reads it back the way rclone does, cutting a fraction past nine digits and padding a shorter
one, and refuses anything that isn't digits, one optional `-`, and one optional `.`.

A write takes the date from the source stream (`VolumeReadStream::modified_at`: a local file's `stat`, another S3
object's own `x-amz-meta-mtime` or `Last-Modified`) and sets it on the PUT or on `CreateMultipartUpload`, never on the
parts. A source with no date writes none. A rename copies the metadata with the object.

## Cost estimates

`cost/`. A dialog about to copy, move, or delete on S3 shows "About $0.02 at AWS list prices". Nothing here sends a
request: the inputs are the scan the dialog already ran.

- **The table** (`cost/s3-prices.json`, schema 1): per provider, request classes (a name as the provider's page spells
  it, a price per million, the S3 operations in it), `egressPerGb`, `storagePerGbMonth`, `minimumStorageDays`,
  `minimumBillableObjectBytes`, the currency, `asOf`, the source URL, and `notes`. `apps/api-server` serves a
  byte-identical copy at `/s3-prices/v1` (`apps/api-server/src/s3-prices/`, whose test compares the two bytewise), so a
  price change edits both and the Worker deploy reaches every install; the bundled copy is the fallback. The sources and
  their dates: `docs/notes/s3/provider-research.md`.
- **Parsing is strict where it protects the math, loose where it protects a newer server**: every operation priced
  exactly once, every number finite and non-negative, else the whole table is refused (the app keeps its copy). Unknown
  providers, operations, and fields are ignored; a newer `schemaVersion` is refused.
- **List prices only.** Free tiers (AWS's 100 GB, R2's monthly requests), included allowances (B2 and Wasabi's free
  downloads, Hetzner's 1 TB), and monthly minimums are `notes`, never math: we can't see the account's month. So
  `egressPerGb` is what a normal account pays per download (AWS $0.09, everyone else $0), and the dialog's (i) says the
  real bill can differ.
- **One-time cost only.** Requests, downloads, and early deletion. Ongoing storage isn't an operation's cost, so the
  minimum billable size matters only inside an early-deletion charge.
- **Early deletion** (Wasabi's 90 days): each deleted object with a known age under the minimum bills
  `max(size, minimum object) × remaining days × storagePerGbMonth / 30`, in GiB. Whole days of age, rounded down; a date
  in the future counts as brand new; an object with no date costs nothing (we don't guess).
- **`Workload` mirrors the write paths**, method by method, with the shapes in each doc comment: an upload is one PUT up
  to the part floor or Create + parts + Complete, then a verifying HEAD; a server copy adds the source's HEAD; a
  provider off the conditional-write list (everyone but AWS and R2) adds a no-overwrite HEAD (two for parts). Deletes
  batch 1,000 keys a `DeleteObjects`; a folder's removal is a capped listing plus the marker's delete. An overwrite is
  `replace_object` (the replaced object's remaining days, with no request of its own) plus, for an upload,
  `upload_over`: off the `refuses_short_body` allowlist a one-PUT overwrite goes as a one-part multipart upload (a HEAD
  finding the original, then Create, a part, and Complete in place of the PUT; § "Overwrites in parts"). A write into a
  folder the operation made (`upload_fresh`, `copy_on_server_fresh`) has no no-overwrite HEAD, and a one-request copy
  there no verifying HEAD; a multipart copy off the pin allowlist has one more source HEAD.
- **The engine's own requests are counted too**, one method per engine step, so the estimate is exact for the shapes the
  dialogs price: `stat_selection` (the scan's top-level stat), `list_folder` (each listing page, read once by the scan
  and once by the walk), `open_destination` and `check_move_within`, `probe_name` (each selected name at the
  destination), `make_folder` (a folder's `create_directory`), `sweep_folder` + `swept_object` (a move's source sweep,
  one `DeleteObjects` per level), and `delete_folder`. The app's `s3_costs/plan.rs` composes them; the app's
  `the_engine_sends_what_the_estimate_counts` runs ten operations through the engine on both fixtures (upload, download,
  same-bucket copy, move within, rename, move off, delete, and the same for two selected files) and asserts every
  request kind equal, and the live cell does the same on each provider's own rules. What stays approximate, each stated
  in `plan.rs`: listing pages are one per folder plus one per thousand files, a move's sweep sends one batch per folder
  level (an empty folder sends none), the destination exists and no selected folder clashes with one there, and source
  folders carry markers (Cmdr's do; a folder without one costs a HEAD where the marker's delete would be). A retried
  request adds one.
- **Gigabytes are binary** (AWS's GB is 2^30; Wasabi's FAQ divides a TB by 1,024).
- **"Other" has no prices**, so no estimate. AWS prices are US East's; other regions differ by a little.
