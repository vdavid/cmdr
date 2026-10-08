# The S3 fixture stack

Two S3 servers in Docker, chosen because they disagree where it matters: VersityGW behaves like AWS on conditional
writes, and Garage doesn't do them at all. Why these two and not MinIO, LocalStack, or SeaweedFS:
`docs/notes/s3/library-and-fixture-audit.md` § Docker S3 test servers. The plan they serve:
`docs/specs/s3-support-plan.md`.

```bash
./start.sh            # core: VersityGW and Garage
./start.sh minimal    # VersityGW alone
./stop.sh             # releases this shell's lease; downs only at zero holders
```

`pnpm check` brings the stack up on its own (`desktop-rust-integration-tests` declares `s3/core`), so a manual
`start.sh` is for iterating by hand. `start.sh` waits for each container's HEALTHCHECK, then sends one signed
ListBuckets from the host before it says "ready".

❌ **Never pause, stop, or kill a container to simulate a server going away**: other test binaries, worktrees, and
sessions lease the same stack at the same time. Put a `cmdr_fs::testing::tcp_proxy::TcpProxy` between the client and the
fixture and cut that, the way `crates/cmdr-webdav/src/volume/connection_drop_test.rs` does.

## The servers

- **`s3-fixture-versitygw`**, port 14480: `versity/versitygw:v1.8.0` over its POSIX backend. The default target. Honours
  `If-None-Match` and `If-Match` on writes.
- **`s3-fixture-garage`**, port 14481: `dxflrs/garage:v2.4.1`, single node. Ignores every write precondition, so it's
  the server that proves Cmdr never leans on no-clobber PUTs working everywhere.

Both answer path-style requests (`http://127.0.0.1:<port>/<bucket>/<key>`) in region `us-east-1`, and both start with
two empty buckets, `cmdr-test` and `cmdr-test-2` (two, so cross-bucket copy has somewhere to go).

**Credentials, the same pair on both**, so a cell switches server by port alone. They're fixtures and public on purpose:

- Access key: `GK00000000000000000000c0de`
- Secret key: `c0de` repeated 16 times (64 hex characters)

The pair is in Garage's own key format (`GK` + 24 hex, a 64-hex secret), because `garage key import` refuses anything
else; VersityGW takes any string. The single source is `docker-compose.yml`; `start.sh` repeats it for its probe.

## How they come up

- **First-party wrapper images**, one per server (`image-versitygw/`, `image-garage/`). Each runs the upstream binary as
  PID 1 and, beside it, an entrypoint step that creates the buckets: through the S3 API for VersityGW (a bucket made by
  `mkdir` lacks the ownership xattrs the gateway writes), through the `garage` CLI for Garage (layout assign and apply,
  key import, bucket create, bucket allow). Garage's official image is `FROM scratch` with no shell, so its wrapper
  copies the binary onto `alpine:3.24.2`.
- **Healthy means bootstrapped.** Each entrypoint writes `/tmp/fixture-ready` after its last bucket, and the HEALTHCHECK
  reads it. The lease adopts a stack on health, so a gateway that answered before its buckets existed would hand a cell
  a `NoSuchBucket`.
- **Idempotent.** Every bootstrap step checks first or tolerates a repeat (VersityGW answers 409 for a bucket it already
  owns), because `restart: unless-stopped` and the volumes mean the entrypoint runs again over state that's already
  there. Verified on 2026-10-01: a re-run adopts, a `restart` re-bootstraps to healthy, and `stop.sh` + `start.sh` comes
  back with the volumes intact.
- **Two named volumes**, `s3-fixture-versitygw-data` and `s3-fixture-garage-data`. ❗ Named, ❌ never a macOS bind mount
  for VersityGW: its POSIX backend keeps object metadata in xattrs. Named also means no anonymous volume per recreation
  (the WebDAV stack once leaked 41 GB that way). Objects persist across runs, so cells must not assume an empty bucket:
  give each cell a key prefix of its own. `docker compose -p s3-fixture down -v` wipes both, and the next bring-up
  recreates the buckets.
- **Scratch expires on its own.** Cells never delete their `scratch_prefix`, and nothing runs `down -v`, so without this
  the VersityGW volume reached 4,900 prefixes and 64 GB in five days. Now everything under `cmdr-test-` goes:
  VersityGW's entrypoint runs a janitor that removes top-level `cmdr-test-*` dirs untouched for two hours
  (`FIXTURE_PREFIX_MAX_AGE_MIN`, checked every `FIXTURE_JANITOR_INTERVAL_S`), and Garage's bootstrap sets a lifecycle
  rule expiring that prefix after one day (its lifecycle worker runs daily, so a day is the floor). The keys seeded once
  and shared across runs live under `cmdr-seed-` for exactly this reason.

## Ports and binding

- **14480+, this stack's own range**: WebDAV owns 13480+, SFTP 12480+, SMB's vendored consumer stack 11480+, and
  `smb2`'s own harness 10480+. The check runner exports `S3_FIXTURE_<SERVICE>_PORT` from
  `scripts/check/checks/s3_ports.go`, and the compose defaults match it (`TestS3FixturePortsMatchComposeDefaults`).
- **Loopback only.** Every mapping carries a `${S3_BIND_ADDR:-127.0.0.1}` prefix, since a writable bucket with public
  credentials shouldn't land on the LAN or the tailnet. `TestS3FixturePortsBindToLoopback` fails the run if a mapping
  loses it. Set `S3_BIND_ADDR=0.0.0.0` to reach the fixtures from a NAT'd VM or a second machine.
- **Its own lease namespace**, `/tmp/cmdr-s3.lock` and `/tmp/cmdr-s3-leases`. The model: `scripts/check/DETAILS.md` §
  "Two fixture stacks, two lease namespaces".

## What each server answered

Observed by hand with `curl --aws-sigv4` and a small Go SigV4 signer against VersityGW v1.8.0 and Garage v2.4.1, on
2026-10-01. These are the evidence later milestones build on; re-check them when bumping either image.

**Conditional writes**, the reason there are two servers:

- `PutObject` with `If-None-Match: *` on an existing key: VersityGW answers **412** `PreconditionFailed` (with
  `<Condition>If-None-Match</Condition>`) and keeps the old bytes. Garage answers **200 and overwrites**: it ignores the
  header without a word.
- `PutObject` with `If-None-Match: *` on a new key: 200 on both.
- `PutObject` with a wrong `If-Match` ETag: VersityGW 412, Garage 200 and overwrites. With the right ETag: 200 on both.
- `CompleteMultipartUpload` with `If-None-Match: *` over an existing key: VersityGW 412 and keeps the old object; Garage
  200 and replaces it.
- `CopyObject` with `If-None-Match: *` over an existing key: **200 and overwrites on both**. So even VersityGW honours
  the precondition on PUT and multipart completion only, never on a server-side copy.

**Everything else:**

- ListBuckets, PUT, `GET` with `Range: bytes=5-9` (206 with `Content-Range: bytes 5-9/20`), `GET` with a range past the
  end (416), `HEAD` on a missing key (404), and a missing bucket (404 `NoSuchBucket`): the same on both.
- Multipart create, upload part, and complete: work on both. The completed ETag is `"<md5>-<part count>"` on both.
- `UploadPartCopy`, whole source and ranged (`x-amz-copy-source-range`): work on both with a source of 5 MiB or more. ❗
  **Garage refuses a copy source under 5 MiB even as the last part** (400 `InvalidRequest`, "Source object is too small
  (minimum part size is 5Mb)"); VersityGW accepts it, as AWS does for a last part. A multipart copy has to upload a
  short tail rather than copy it, at least on Garage.
- `CopyObject` across buckets (`cmdr-test` to `cmdr-test-2`): 200 on both.
- ❗ **A PUT cut off mid-body (the client closes the connection before `Content-Length`): VersityGW stores the bytes
  that arrived as the object**, under its name, which S3 never does; Garage keeps nothing. Deterministic on VersityGW
  v1.8.0 (`cmdr-s3`'s `write_test.rs::a_cancelled_single_put_publishes_nothing`, a source stalled after 1 MiB of 3:
  1,048,576 bytes stored), 2026-10-01.
- ❗ **An aborted multipart upload can come back on VersityGW** when an `UploadPart` cut off a moment before the abort
  lands after it: `ListMultipartUploads` shows the upload again (7 of 20 runs of `…a_cancel_mid_multipart…`,
  2026-10-01). AWS documents the same race and says to abort again until the parts are gone.
- A `PutObject` to a key while a multipart upload of that key is in flight: ❗ **Garage ends the upload** (the next
  `UploadPart` or `CompleteMultipartUpload` answers 404 `NoSuchUpload`); VersityGW keeps it, and completing it replaces
  the object just put. Observed through `cmdr-s3`'s `write_test.rs` (`…catches_a_writer_mid_upload`, both servers) on
  2026-10-01; `crates/cmdr-s3/DETAILS.md` § "No-overwrite writes" has what Cmdr does with it.
- `DeleteObjects` with `Content-MD5`: 200 on both, and a key that never existed comes back under `<Deleted>`, as on AWS.
  Garage also adds a `<VersionId>` and `<DeleteMarkerVersionId>` to each entry on an unversioned bucket. Past 1,000
  keys, two requests clear 1,005 on both (`cmdr-s3`'s `batch_test.rs`, 2026-10-01).
- `CopyObject` with `x-amz-metadata-directive: REPLACE` writes the metadata sent (an `x-amz-meta-mtime` included) on
  both, and `UploadPartCopy` with `x-amz-copy-source-if-match` carrying the source's current ETag succeeds on both
  (`cmdr-s3`'s `copy_test.rs`, 2026-10-01). A stale one (the source replaced mid-copy) is refused with 412 on both
  (`a_source_replaced_mid_copy_fails_as_changed_and_publishes_nothing`, 2026-10-02).
- A presigned `GET` (query auth, `X-Amz-Expires=300`, `UNSIGNED-PAYLOAD`): 200 on both.
- A wrong secret: VersityGW answers 403 `SignatureDoesNotMatch`, Garage 403 `AccessDenied`. A refusal classifier can't
  rely on the code alone to tell "wrong secret" from "no permission".
- Garage's `CompleteMultipartUpload` `<Location>` reads `https://cmdr-test..s3.garage.localhost/...` (a doubled dot from
  its `root_domain`). Cosmetic; nothing should parse it.

**How real providers compare.** `live.sh` beside this README runs `cmdr-s3`'s `live_` cells against real accounts (§
"Live providers" below; never in a lane). Two things the fixtures here don't show: both accept a zero-byte PUT and a
bodyless POST without `Content-Length`, which R2, Hetzner, and GCS refuse with 411; and R2 refuses a last part LARGER
than the rest, the shape Garage's small-copy-source rule pushed toward. Real providers refuse a cut-off PUT (VersityGW's
publish is the fixture outlier), and VersityGW's `If-None-Match` on Complete matches AWS and R2 but not Hetzner, GCS, or
Spaces. All findings: `crates/cmdr-s3/DETAILS.md` § "Verified providers".

❗ **macOS's curl 8.7.1 signs `x-amz-copy-source-range` wrong**: both servers reject the request with a signature
mismatch, while the same request signed by hand passes. It's a curl bug, not a server one, so don't trust a
`curl --aws-sigv4` failure on that header as evidence about a server.

## Live providers

`live.sh` runs `cmdr-s3`'s `live_` cells against seven real accounts: R2, Hetzner (`nbg1`), GCS, Spaces, AWS
(`eu-north-1`, plus a `us-west-2` bucket for region routing), Backblaze B2 (`eu-central-003`), and Wasabi
(`eu-central-1`). Never in a lane or CI.

```bash
./live.sh                      # every provider, every live cell, then the sweep
./live.sh aws,b2               # only these providers
./live.sh all live_batch       # every provider, only cells matching a filter
./live.sh all live_hostile     # the hostile cells: names, sizes, cancels, crashes, races, scale
source ./live-env.sh           # the variables alone, for another runner
```

The hostile cells (`cmdr-s3`'s `live_hostile_test.rs` and `live_hostile_failure_test.rs`) take 2–5 minutes per provider
(the ~1 GiB read-back dominates; 300 MiB on Wasabi, 200 MiB on B2 for its 1 GB daily download cap), so run them a few
providers at a time or one cell at a time (`live_hostile_sizes`, `live_hostile_cancel`, `live_hostile_crash`,
`live_hostile_races`, …). Findings: `docs/notes/s3/live-hostile-2026-10.md`.

`live-engine.sh` runs the app's transfer-engine flows against the same accounts: the scenarios the app crate's
`s3_integration_` Docker cells run (copy, move, merge under every policy, rename, delete, cancel, pause, rollback), plus
copies between two providers, archived objects on AWS, and the requests each operation sends against the cost estimate.
Same arguments (`./live-engine.sh r2,gcs`, `./live-engine.sh all copies_between`), `cargo test` one cell at a time, one
`LIVE [provider] flow: ok` line per pair; `CMDR_S3_LIVE_FLOWS=pause,rollback` narrows a cell to the flows whose names
hold one of the pieces (a capped B2 rerun); `RUST_LOG=copy=debug,volume=info` explains a failure. A full pass is roughly
5–15 minutes per provider (GCS and R2 the slowest), so run a few providers at a time. ❗ The 1,005-object rename sends
about 1,005 HEADs (a source one per object) plus its seeding and the other cells' reads: on a B2 account with a daily
Class B cap (2,500 free) a full run can still use it up, and every B2 read then answers 403 until midnight GMT.
Findings: `docs/notes/s3/live-engine-2026-10.md`.

`live-env.sh` is the single source of the variables (`CMDR_S3_LIVE_<NAME>_{REGION,KEY_ID,SECRET,BUCKET,BUCKET_2}` and a
few extras): credentials from David's sops store through `secret`, bucket names as defaults. Any variable already set
wins. ❗ It never echoes a value. The buckets stay between runs; each cell deletes what it wrote under
`cmdr-live/<run>/`, and the last step sweeps leftovers. Cost guardrails: Wasabi bills every object for 90 days even once
deleted, so its big-file cells stay small.

Hetzner and Spaces bill a base fee while any bucket exists, even an empty one, so their test buckets don't stay: they
exist only for a run, and `live-env.sh` exports their variables only when a bucket is passed in. A default run skips
both. To rerun one:

- **Hetzner** (about EUR 6.49/month, billed by the hour, so a one-hour run costs about one cent; verified on
  hetzner.com/storage/object-storage, 2026-10-02): create the buckets in `nbg1` with the `HETZNER_S3_*` key
  (`aws s3api create-bucket --endpoint-url https://nbg1.your-objectstorage.com`), then
  `CMDR_S3_LIVE_HETZNER_BUCKET=<name> CMDR_S3_LIVE_HETZNER_BUCKET_2=<name-2> ./live.sh hetzner`. `_BUCKET_2` is optional
  and enables the cross-bucket cells.
- **Spaces** (USD 5/month while any bucket exists, prorated): the `DO_SPACES_*` key is scoped to the bucket name in
  `secret DO_SPACES_TEST_BUCKET` and can't create a bucket, so create that name in `secret DO_SPACES_REGION` from the
  control panel (or with a temporary full-access key minted by the PAT: `POST /v2/spaces/keys` with
  `{"bucket": "", "permission": "fullaccess"}`), then `CMDR_S3_LIVE_SPACES_BUCKET=<name> ./live.sh spaces`.
- **After the run**, empty and delete the buckets (abort unfinished multipart uploads first), and list buckets to
  confirm none remain. Spaces' scoped key can empty its bucket but not delete it: delete it with a temporary full-access
  key over the S3 API (DigitalOcean's API has no bucket delete), then `DELETE /v2/spaces/keys/<access_key>`.

## Adding a server

1. Add the service to `docker-compose.yml`, prefixed `s3-fixture-`, on the next free port, publishing it as
   `'${S3_BIND_ADDR:-127.0.0.1}:${S3_FIXTURE_<NAME>_PORT:-<port>}:<container port>'`, with a HEALTHCHECK that passes
   only once its buckets exist.
2. Add it to the right mode in `start.sh`'s case table **and** to `modeServices` on `stacklease.S3` in
   `scripts/check/stacklease/registry.go` (`TestS3ModeServicesAgree`). Map its container port in `start.sh`'s probe.
3. Add its port to `s3ServiceHostPorts` in `scripts/check/checks/s3_ports.go`, and its key to `s3CoreServices` if it's
   in `core`.
4. If it builds an image of its own, add the context to `buildContextsRel` on `stacklease.S3`, or an edit to it never
   reaches a running container.

## Adding a cell

The shared fixture lane selects every `#[ignore]`d test in `package(cmdr-s3)`, plus app-crate cells named
`s3_integration_*` (`laneFixtures` in `scripts/check/checks/fixture-lane-coverage.go`). Gate each cell with an
`#[ignore]` reason naming `s3-servers/start.sh` or `s3-fixture`, connect through `cmdr_s3::volume::testing`, and work
under a `scratch_prefix` of your own: the objects persist across runs until the fixture expires them. A key meant to be
shared across runs goes under `cmdr-seed-` (`testing::seed_once`), never `cmdr-test-`. Seed with `testing::seed`, which
goes through the crate's own request builders. Run a cell against BOTH servers when what it asserts could differ between
them; a wrong secret already does (above). By hand: `./start.sh`, then
`cargo nextest run -p cmdr-s3 --run-ignored only`.

Every S3 fixture cell runs four at a time on nextest's `s3-fixture` test group with a 30 s cap (`.config/nextest.toml`):
at full parallelism on a loaded machine all of them starved past the 8 s cap together. The group finds a `cmdr-s3` cell
by its `_on_versitygw` / `_on_garage` suffix (or the connection-drop and reconnect modules) and an app cell by its
`s3_integration_` prefix, so ❗ a new cell keeps that naming or it runs ungrouped. The lane takes about three minutes.
