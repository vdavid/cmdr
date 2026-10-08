# S3 library and test-fixture audit

Input for `crates/cmdr-s3`, a backend built in the same shape as `crates/cmdr-webdav` (`reqwest` 0.13 confined to a
transport module, streaming bodies with no request timeouts, silence detection, typed errors, no string matching).
Everything below was checked on 2026-10-01 against crates.io, GitHub, Docker Hub, vendor docs, and the crates' source
(rusty-s3 cloned at `v0.10.2`; `object_store` 0.14.2 and `aws-sdk-s3` 1.150.0 read from the cargo registry).

## Recommendations

- **Library**: write our own SigV4 signer and S3 XML layer on crates already in `Cargo.lock` (zero new crates). Don't
  adopt `rusty-s3` as the signer: it signs every request as a presigned URL (query-string auth), and Cloudflare R2
  doesn't accept presigned `POST`, which is how CreateMultipartUpload, CompleteMultipartUpload, and DeleteObjects
  travel. Keep rusty-s3 as a reference and a source of test vectors. Fallback if we'd rather not own the signer:
  standalone `aws-sigv4` (10 new crates, AWS-maintained). `aws-sdk-s3` doesn't fit the house shape.
- **Fixture**: VersityGW (`versity/versitygw`, Apache-2.0) as the primary Docker S3 server, plus Garage
  (`dxflrs/garage`) as a second server whose gaps we want to exercise (no conditional writes).

## Candidate crates

### rusty-s3

- **Version**: 0.10.2, released 2026-08-01 (60 days old, passes the 3-day window). 0.10.0 2026-06-18, 0.10.1 2026-07-21.
  BSD-2-Clause. About 1.1M downloads total, about 550k recent.
- **Repo health**: `paolobarbolini/rusty-s3`, 157 stars, not archived, last push 2026-09-21. Bus factor is one: Paolo
  Barbolini has 209 commits; the next human has nine. Eight open issues and PRs, several of them old asks that never
  landed:
  - #47 (2022-01) "add CopyObject action": an open PR, untouched for over four years.
  - #95 (2023-11) "Is there a way to parse S3 error response?": still open, no error parsing exists.
  - #92 (2023-10) "Type definitions for HeadObject": still open, no header parsing exists.
  - #163 (2026-08) "`ListObjectsV2::parse_response` fails on responses that omit `ETag`".
- **Design**: sans-IO. Each action builds a URL and signs it; you send it with any HTTP client. ❗ Signing is
  presigned-URL only (`signing::sign` writes `X-Amz-Algorithm`, `X-Amz-Credential`, `X-Amz-Signature`, and the rest into
  the query string). There's no `Authorization` header mode anywhere in the crate. The canonical request hardcodes
  `UNSIGNED-PAYLOAD` (`src/signing/canonical_request.rs`), so streaming bodies work.
- **Operations covered** (from `src/bucket.rs` and `src/actions/`):
  - ListObjectsV2 (prefix, delimiter, continuation token, start-after, max-keys; always sends `encoding-type=url` and
    percent-decodes keys). The response struct has no `IsTruncated` or `KeyCount`, and `ETag` is a required field
    (that's #163).
  - HeadObject, GetObject, PutObject, DeleteObject: URL signing only. Range, `If-None-Match`, and `x-amz-meta-*` work by
    inserting them into `headers_mut()` before signing, then sending the same headers. Nothing parses response headers.
  - DeleteObjects: builds the XML body plus `Content-MD5` (pulls `md-5` and `base64`), parses `DeleteResult` including
    per-key `Error` (`Code` as a `String`).
  - Multipart: CreateMultipartUpload, UploadPart, CompleteMultipartUpload (body from an ETag iterator, part numbers
    implied 1..N), AbortMultipartUpload, ListParts.
  - Also CreateBucket, DeleteBucket, HeadBucket, GetBucketPolicy.
- **Operations missing**, which we'd hand-roll anyway: ListBuckets, CopyObject, UploadPartCopy
  (`x-amz-copy-source-range`), ListMultipartUploads, all response-header parsing (HeadObject's size, ETag,
  `Last-Modified`, `x-amz-meta-*`), and the S3 `<Error>` body (`Code`, `Message`, `RequestId`).
- **Error bodies**: not handled at all. No typed codes; `PreconditionFailed`, `SlowDown`, `NoSuchKey`, and
  `InvalidObjectState` would all be ours.
- **Dependencies**: `jiff`, `url`, `percent-encoding`, `zeroize`, plus a crypto backend: `rustcrypto` (`hmac` 0.13 +
  `sha2` 0.11), `aws-lc-rs`, or `graviola`. The `xml` feature adds `instant-xml` (+ `instant-xml-macros`, `xmlparser`);
  `full` adds `md-5`, `base64`, `serde`, `serde_json`. It migrated from `quick-xml` to `instant-xml` on 2026-05-14, so
  its XML parsing would be a second XML stack next to our `quick-xml` 0.41 (which `cmdr-webdav` uses).
- **Lockfile cost** (resolved in a scratch crate and diffed by name against our `Cargo.lock`):
  - `default-features = false, features = ["aws-lc-rs"]`: one new crate (`rusty-s3`), plus `jiff-core` only because the
    scratch resolve picked a newer `jiff` than our 0.2.31. All its crypto and URL crates are already ours.
  - Default features: six new (`rusty-s3`, `instant-xml`, `instant-xml-macros`, `xmlparser`, and the `jiff` note above).
- **Fit verdict**: cheap in dependencies, but it covers roughly half the needed surface, and its one structural feature
  (presign-everything) is the wrong shape for us:
  - **R2 rejects presigned POST**. Cloudflare's docs: "R2 supports presigned URLs for GET, HEAD, PUT, and DELETE
    operations. POST is not currently supported" (`developers.cloudflare.com/r2/api/s3/presigned-urls/`, read
    2026-10-01). CreateMultipartUpload, CompleteMultipartUpload, and DeleteObjects are all POST. Untested by us on a
    real bucket; if we ever reconsider rusty-s3, verify this first.
  - **Signatures land in URLs**, and `reqwest::Error`'s `Display` includes the URL, so every transport error would log a
    live credential-bearing URL unless we scrub it. Header auth avoids that class of leak.
  - With the XML feature off, what remains is about 300 lines of signing and URL building, which is what we'd write.

### aws-sdk-s3

- **Version**: 1.151.0 published 2026-09-30 (one day old, too fresh); pin 1.150.0 (2026-09-25) if ever adopted. It ships
  a release roughly weekly. Apache-2.0. About 91M downloads.
- **Repo health**: `awslabs/aws-sdk-rust` (generated output, 3,340 stars, 153 open issues, pushed 2026-09-30); the
  generator is `smithy-lang/smithy-rs`. Corporate-maintained, not archived.
- **Coverage**: everything on our list, with typed per-operation errors (`NoSuchKey` and friends as enum variants) and a
  presigner.
- **Lockfile cost**:
  - With `aws-config` and defaults: 33 new crate names, among them `aws-sdk-sso`, `aws-sdk-ssooidc`, `aws-sdk-sts`,
    `aws-smithy-*` (about a dozen), `crc-fast`, `lru`, `regex-lite`, `xmlparser`, and `base64-simd`. Defaults also pull
    a second, legacy HTTP stack beside ours: `hyper` 0.14, `h2` 0.3, `http` 0.2, `http-body` 0.4, `rustls` 0.21,
    `hyper-rustls` 0.24, `tokio-rustls` 0.24, `sct`.
  - `default-features = false, features = ["rt-tokio", "http-1x", "behavior-version-latest"]` and no `aws-config`: 26
    new names, no new HTTP stack, but then we have to implement the `HttpClient` trait over our `reqwest` ourselves. The
    SDK has no reqwest adapter; its own client (`aws-smithy-http-client`) is hyper-based.
- **Compile weight**: `aws-sdk-s3` alone is 238,453 lines of generated Rust (18 MB of source, measured in the registry);
  `object_store` for comparison is 38,685. Expect a large one-off cost on clean builds and in every cold check lane. We
  didn't build it, to spare the machine.
- **Checksum default**: since the January 2025 SDK releases, `RequestChecksumCalculation` and
  `ResponseChecksumValidation` default to `WhenSupported`. The SDK then sends `x-amz-checksum-crc32` /
  `x-amz-sdk-checksum-algorithm` and switches streaming uploads to `aws-chunked` trailer encoding. Backblaze B2 rejects
  those headers ("Unsupported header 'x-amz-checksum-algorithm'", seen in the Discourse and pingvin-share bug threads
  from 2025), and other S3-compatibles broke the same way. The fix is
  `.request_checksum_calculation(RequestChecksumCalculation::WhenRequired)` plus
  `.response_checksum_validation(ResponseChecksumValidation::WhenRequired)` on the S3 config (both builder methods exist
  in 1.150.0, `src/config.rs`), or the env vars `AWS_REQUEST_CHECKSUM_CALCULATION=when_required` and
  `AWS_RESPONSE_CHECKSUM_VALIDATION=when_required`.
- **Fit verdict**: no. It brings its own retry, timeout, and stalled-stream policies (which fight our silence detection
  and no-timeout streaming), a large compile cost, and an HTTP adapter we'd have to write anyway.

### object_store (brief)

- 0.14.2, 2026-09-15, MIT/Apache-2.0, `apache/arrow-rs-object-store` (324 stars, 162 open issues, active). About 91M
  downloads.
- Surprisingly cheap for us: it rides `reqwest` 0.13 and `quick-xml` 0.41, so only four new names (`object_store`,
  `crc-fast`, `humantime`, and the probe crate itself).
- Wrong abstraction: a generic `ObjectStore` trait. No ListBuckets, no UploadPartCopy, no ListMultipartUploads or
  resume. Non-multipart `put` takes an in-memory `PutPayload`, not a stream. `ClientOptions` defaults to a 30 s request
  timeout and 5 s connect timeout, and it runs its own retry loop. Errors collapse to `NotFound`, `AlreadyExists`,
  `Precondition`, and a generic bucket. It does support `If-None-Match: *` (`PutMode::Create`), metadata attributes, and
  presigned URLs.

### rust-s3 (brief)

- 0.37.2, 2026-05-04, MIT, `durch/rust-s3` (679 stars, 27 open issues, pushed 2026-09-29).
- A full client with its own HTTP choices: 25 new names, including `attohttpc`, `native-tls`, `openssl`/`openssl-sys`,
  `hyper-tls`, `minidom`/`rxml` (a third XML stack), and `rust-ini`. Not a fit for a crate where `reqwest` is the
  transport.

### aws-sigv4 standalone (the fallback)

- 1.6.0, 2026-09-22, Apache-2.0, part of smithy-rs. With `default-features = false, features = ["http1", "sign-http"]`
  it adds 10 names: `aws-sigv4`, `aws-credential-types`, `aws-smithy-async`, `aws-smithy-http`, `aws-smithy-runtime-api`
  (+ `-macros`), `aws-smithy-types`, `base64-simd`, `bytes-utils`, `outref`, `vsimd`.
- Does header signing and presigning, and `SignableBody::UnsignedPayload` for streaming.

## Licenses against `deny.toml`

All allowed. `rusty-s3` is BSD-2-Clause; `jiff` and its siblings are MIT/Unlicense (already in our graph); `instant-xml`
is Apache-2.0 OR MIT; `xmlparser` is MIT/Apache-2.0; every `aws-*` and `aws-smithy-*` crate is Apache-2.0; `crc-fast` is
MIT OR Apache-2.0; `vsimd` and `outref` are MIT; `object_store` is MIT/Apache-2.0; `rust-s3` is MIT. Nothing needs a new
`allow` entry. (Read from crates.io license fields; we didn't run `cargo deny` because nothing was added to the
workspace.)

## What we'd write ourselves (recommended path)

Everything sits on crates already in `Cargo.lock`: `reqwest` 0.13, `quick-xml` 0.41, `hmac` 0.13 + `sha2` 0.11 (or
`aws-lc-rs` 1.18), `url`, `percent-encoding`, `httpdate`, `jiff` or `time`, `md-5` 0.11 + `base64` 0.22/0.23. Rough size
is 1,000–1,400 lines plus tests:

1. **SigV4 signer** (about 250–350 lines). `Authorization` header mode for every API call with
   `x-amz-content-sha256: UNSIGNED-PAYLOAD`, plus query-string presigning for share links (7 days max, which is S3's
   SigV4 ceiling). Gotchas: S3 encodes the canonical URI once (other AWS services encode it twice); RFC 3986 query
   encoding (`%20`, never `+`; rusty-s3 fixed exactly this on 2026-06-10); trimmed header values; and signed `host`
   including a non-default port. Test against AWS's published SigV4 examples (rusty-s3's `src/signing/mod.rs` tests
   carry the S3 presign vectors) and against the fixture servers.
2. **Request builders** (about 150 lines). Path-style vs virtual-host URLs, per-segment key encoding, `region` (`auto`
   for R2, `us-east-1` default), `x-amz-meta-*`, `If-None-Match: *`, `Range`, `x-amz-copy-source` (URL-encoded) and
   `x-amz-copy-source-range`.
3. **XML parsers on quick-xml** (about 400–600 lines). `ListBucketResult` (V2, with `IsTruncated`, optional `ETag`, and
   `encoding-type=url` decoding), `ListAllMyBucketsResult`, `InitiateMultipartUploadResult`,
   `CompleteMultipartUploadResult`, `CopyObjectResult`, `CopyPartResult`, `ListMultipartUploadsResult`,
   `ListPartsResult`, `DeleteResult`, and `Error`. ❗ CompleteMultipartUpload, CopyObject, and UploadPartCopy can answer
   HTTP 200 with an `<Error>` body; the parser must check the root element, never trust the status alone.
4. **XML builders** (about 60 lines). `CompleteMultipartUpload` (explicit part numbers) and `Delete`, with `Content-MD5`
   (AWS still requires it, or a checksum header, on DeleteObjects).
5. **Typed errors** (about 150 lines). An enum from the `<Error><Code>`, e.g. `NoSuchKey`, `NoSuchBucket`,
   `NoSuchUpload`, `PreconditionFailed`, `SlowDown`, `InvalidObjectState`, `AccessDenied`, `SignatureDoesNotMatch`,
   `RequestTimeTooSkewed`, `EntityTooSmall`, `InvalidPart`, with a catch-all `Other(String)` for the code only. Classify
   on `Code` and HTTP status, never on `Message`. HEAD responses have no body, so they classify by status alone (404,
   412, 403).
6. **Header parsing** (about 50 lines). `Content-Length`, `ETag`, `Last-Modified`, `Content-Range`, `x-amz-meta-*`,
   `x-amz-version-id`.

## Docker S3 test servers

### Assessed

- **MinIO**: out. `minio/minio` was archived on GitHub on 2026-04-25 (AGPL-3.0, 61k stars), after a move to source-only
  distribution in late 2025 and removal of the community Docker Hub images (Docker Hub `minio/minio/tags` returns
  "object not found" on 2026-10-01). Building our own image from an archived tree would pin us to dead code.
- **LocalStack**: out. `localstack/localstack` was archived on GitHub (last push 2026-03-23), and since 2026-03-23 the
  `localstack/localstack` image needs an auth token, even in CI (LocalStack blog "moving to a single image";
  testcontainers issues #465 and #3592).
- **VersityGW**: in. `versity/versitygw`, Apache-2.0, 2,967 stars, pushed 2026-09-30, 151 open issues (a busy tracker,
  not a neglected one). Latest v1.8.0 on 2026-09-04; image `versity/versitygw:v1.8.0` is 31 MB. One Go binary, POSIX
  backend (a bucket is a directory, an object is a file), so it starts in well under a second and lets tests seed data
  straight into the backing directory. Its source implements UploadPartCopy, DeleteObjects, and `If-None-Match` /
  `If-Match` on puts (the wiki's POSIX backend page documents "conditional-publish locking"). Run shape:
  `ROOT_ACCESS_KEY=… ROOT_SECRET_KEY=… versitygw --port :<high port> posix /data`. Keep `/data` on a named volume, not a
  macOS bind mount: the POSIX backend stores metadata in xattrs, which Docker Desktop bind mounts may not carry
  (unverified).
- **Garage**: in, as the "different behavior" server. `dxflrs/garage:v2.4.1` (2026-09-08), 27 MB, AGPL-3.0 (we only run
  the container, no linking, so the license doesn't touch Cmdr). Rust, actively developed (primary forge
  `git.deuxfleurs.fr/Deuxfleurs/garage`, GitHub mirror 4,618 stars). Its S3 compatibility page lists ListBuckets,
  CopyObject, DeleteObjects, ListMultipartUploads, ListParts, UploadPartCopy, and presigned URLs as implemented. It does
  NOT support conditional writes: forge issue #1052 "support conditional writes" is open (filed 2025-05-28), and #1326
  was closed as its duplicate on 2026-02-07. It silently ignores `If-None-Match` and overwrites (verified on v2.4.1,
  2026-10-01; the full per-call table is in `apps/desktop/test/s3-servers/README.md`). That gap is the point: Cmdr must
  not assume no-clobber writes work everywhere. Setup cost: a TOML config plus a bootstrap step (`garage layout assign`
  / `apply`, `garage key create`, `garage bucket create`, `garage bucket allow`) in `start.sh`, a few seconds.
- **SeaweedFS**: a solid alternative to Garage. Apache-2.0, 35k stars, pushed 2026-10-01, 795 open issues; images about
  26 MB. Its S3 gateway handles `If-None-Match` (`weed/s3api/s3api_object_handlers_put.go`) and copy-part. Heavier
  conceptually (master + volume + filer behind the gateway), so it brings more moving parts than we need.
- **s3s / s3s-fs**: Apache-2.0, `Nugine/s3s`, 310 stars, 0.17.0 on 2026-09-24. `s3s-fs` implements ListBuckets,
  CopyObject, UploadPartCopy, DeleteObjects, ListParts, and references `if_none_match`, but has no ListMultipartUploads,
  and the project describes `s3s-fs` as a test/demo backend. Could run in-process as a Rust dev-dependency, at the cost
  of pulling `hyper` server crates into dev builds. Not first choice.
- **RustFS**: Apache-2.0, 34k stars, very active, but still `1.0.1-preview.*` tags and a 111–166 MB image. Too young and
  too big for a fixture.
- **Scality CloudServer (Zenko)**: Apache-2.0, 1,951 stars, active, but a Node.js server with a large image and slow
  start. No advantage over VersityGW.
- **Ceph RGW**: the real thing behind Hetzner Object Storage, but there's no maintained single-container demo image and
  a full Ceph is far too heavy for a test fixture. Cover Ceph-specific behavior by running the suite against a real
  Hetzner bucket occasionally instead.

### Recommendation

- **Primary**: VersityGW. Alive, permissive, 31 MB, sub-second start, and it covers multipart, UploadPartCopy,
  conditional writes, DeleteObjects, ListBuckets, and presigned URLs.
- **Secondary**: Garage, to exercise a server without conditional writes and with a different implementation lineage
  (Rust, its own listing and ETag code). Swap in SeaweedFS if Garage's bootstrap step proves annoying.
- Both should bind `127.0.0.1` on high ports, the same as `apps/desktop/test/webdav-servers/`.
