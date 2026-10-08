# S3 provider capability research

Capability and pricing facts for the `crates/cmdr-s3` backend, per provider, from provider docs. All pages accessed
2026-10-01 unless a page date is given. "Page date" is the provider's own "last updated" stamp. Forum and third-party
claims are labeled **corroboration** and never stand alone for a "supported" verdict.

Providers: AWS S3 (general purpose buckets), Cloudflare R2, Backblaze B2 (S3-compatible API), Wasabi, Hetzner Object
Storage, Google Cloud Storage (XML API), DigitalOcean Spaces. MinIO is skipped: nothing trivially available that changes
a decision here.

**Live findings beat this note.** All seven named providers were tested against real accounts on 2026-10-02
(`apps/desktop/test/s3-servers/live.sh`); the results, and where they contradict a doc below (marked **Live:**), are in
`crates/cmdr-s3/DETAILS.md` § "Verified providers".

## AWS S3 (general purpose buckets)

### Conditional writes

- `If-None-Match: *` is supported on `PutObject`, `CompleteMultipartUpload`, and `CopyObject`. The header expects `*`
  and requires SigV4 and `s3:PutObject`. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html
- Clash: `412 Precondition Failed`. With concurrent conditional writes, the first to finish wins and later ones get 412.
  Same source.
- `409 Conflict` (`ConditionalRequestConflict`) when a concurrent delete succeeds before the conditional write
  completes. `PutObject` may be retried on 409; `CompleteMultipartUpload` needs a whole new `CreateMultipartUpload`.
  Sources: same page, plus https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html
- In-progress multipart uploads don't count as existing objects: client A's MPU can be beaten by client B's conditional
  `PutObject`, and then A's conditional `CompleteMultipartUpload` fails with 412. Source: conditional-writes page.
- Versioned buckets: the check applies to the current version only; a current delete marker counts as "absent". Same
  source.
- `If-Match: <etag>` is also supported on the same three APIs (412 on mismatch, 404 if the object vanished). Same
  source.

### CopyObject

- Single atomic `CopyObject` up to 5 GB; larger needs `UploadPartCopy`. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/API/API_CopyObject.html
- Cross-bucket and cross-region copy are supported (both regions must be enabled for the account; cross-region data
  transfer is billed to the source account). Same source.
- `CopyObject` can return `200 OK` with an error embedded in the body (errors during the copy, for example throttling in
  a cross-region copy). Always parse the body. Same source.
- `x-amz-metadata-directive`: `COPY` (default) or `REPLACE`. Same source.
- Source in `GLACIER` / `DEEP_ARCHIVE` (or Intelligent-Tiering archive tiers) must be restored before it can be a copy
  source. Same source.
- `UploadPartCopy` supports `x-amz-copy-source-range`; part limits come from the multipart limits page. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/API/API_UploadPartCopy.html

### Multipart

- Max object size 48.8 TiB ("50 TB"); 10,000 parts max; part size 5 MiB to 5 GiB, no minimum on the last part; ListParts
  and ListMultipartUploads return at most 1,000 entries per page. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/userguide/qfacts.html
- Single `PUT` up to 5 GB. Source: https://docs.aws.amazon.com/AmazonS3/latest/userguide/upload-objects.html
- Lifecycle `AbortIncompleteMultipartUpload` with `DaysAfterInitiation` auto-aborts stale uploads. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpu-abort-incomplete-mpu-lifecycle-config.html
- `ListMultipartUploads` and `AbortMultipartUpload` are standard (API reference). No default auto-abort rule is
  documented: incomplete uploads are billed until aborted.

### Listing

- `ListObjectsV2` supports `delimiter` (any character for general purpose buckets; only `/` for directory buckets), up
  to 1,000 keys per page. Source: https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html
- `ListBuckets` needs `s3:ListAllMyBuckets`. Paginated with `max-buckets` (1–10,000) and `continuation-token`; accounts
  with a bucket quota above 10,000 must paginate. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListBuckets.html
- With an IAM policy scoped to one bucket (no `s3:ListAllMyBuckets`), the request is denied (`AccessDenied`, 403). This
  follows from the permission requirement; I didn't find a page that shows the exact response.

### DeleteObjects

- Up to 1,000 keys per request; verbose or quiet mode; a missing key is reported as deleted. `Content-MD5` is documented
  as required for general purpose buckets. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/API/API_DeleteObjects.html
- Note for implementation: current aws-sdk versions send a flexible checksum instead of `Content-MD5`; AWS accepts that.
  I didn't find an AWS page stating this, so treat it as unverified for third-party providers (see "Couldn't verify").

### Rate limits

- At least 3,500 `PUT/COPY/POST/DELETE` and 5,500 `GET/HEAD` requests per second per partitioned prefix; `503 Slow Down`
  while S3 scales. Source: https://docs.aws.amazon.com/AmazonS3/latest/userguide/optimizing-performance.html

### Presigned URLs

- Max 7 days with SigV4 and long-term IAM user credentials; temporary credentials cap it at the session lifetime;
  console max 12 hours. Source: https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html

### Checksums

- `CRC64NVME` is the default checksum algorithm; all AWS-owned clients compute a checksum on upload and S3 validates it
  server side. Source: https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity.html
- So AWS accepts `x-amz-checksum-*` and `x-amz-sdk-checksum-algorithm` natively.

### Metadata

- User metadata (`x-amz-meta-*`) is limited to 2 KB (sum of UTF-8 bytes of keys and values) within an 8 KB `PUT` header;
  keys are stored lowercase; it can't be changed after upload except by copying the object onto itself. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html
- `Last-Modified` isn't user-modifiable (table column "can user modify: No"). For multipart uploads it's the date the
  MPU was **initiated**, not completed. Same source.

### Archive storage classes

- `GetObject` returns `InvalidObjectState` (403) for Glacier Flexible Retrieval, Glacier Deep Archive, and the
  Intelligent-Tiering Archive / Deep Archive Access tiers until `RestoreObject` completes. Source:
  https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html
- Glacier Instant Retrieval reads immediately (it isn't in that list).

### Endpoint shape

- Both virtual-hosted and path-style work in all regions; path-style "will be discontinued in the future", but the
  deprecation was postponed (2020-09-23 update, no new date). Virtual-hosted TLS fails for bucket names with dots.
  Source: https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html
- Regions are real region codes (`us-east-1` and so on); SigV4 must use the bucket's region.

### Pricing (US East, N. Virginia; page `og:updated_time` 2026-09-25)

- Source: https://aws.amazon.com/s3/pricing/
- Storage, S3 Standard: $0.023/GB-month (first 50 TB).
- `PUT/COPY/POST/LIST`: $0.005 per 1,000.
- `GET/SELECT` and all other requests: $0.0004 per 1,000.
- `DELETE` and `CANCEL`: free. `DeleteObjects` (a POST) counts as a delete and is free: **corroboration**
  (https://repost.aws/questions/QUZouQPXJERluPStXFvke0tw/s3-deleteobject-pricing), the price table's assumption.
- Free Tier: since 2025-07-15, new accounts get up to $200 in credits instead of per-service allowances (pricing page,
  re-scraped 2026-10-01).
- Egress to internet: $0.09/GB (first 10 TB/month); first 100 GB/month free, aggregated across all AWS services and
  regions.
- Standard-IA and One Zone-IA: 30-day minimum storage duration, prorated charge for early deletion. S3 Standard has no
  minimum duration and no minimum object size.

## Cloudflare R2

### Conditional writes

- `PutObject` supports `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`. Source:
  https://developers.cloudflare.com/r2/api/s3/api/ (page date 2026-07-31)
- Clash: `412 PreconditionFailed` (error 10031). Source: https://developers.cloudflare.com/r2/api/error-codes/ (page
  date 2026-07-31)
- `CopyObject`: the standard `If-None-Match` on the destination isn't listed. R2 has its own headers instead,
  `cf-copy-destination-if-match` and `cf-copy-destination-if-none-match`, which fail with 412. They're checked at commit
  time and aren't atomic with the `x-amz-copy-source-if-*` checks. Source:
  https://developers.cloudflare.com/r2/api/s3/extensions/ (page date 2026-06-08)
- `CompleteMultipartUpload`: the compatibility table lists no conditional headers, so treat it as **not supported** (the
  table explicitly lists what's implemented per operation). **Live:** R2 enforces `If-None-Match: *` there (412), and
  ignores it on `CopyObject`.

### CopyObject

- Supported with `x-amz-metadata-directive` and source conditionals; `UploadPartCopy` supported with
  `x-amz-copy-source-range` (source conditionals not implemented there). Source: S3 API compatibility page.
- Single-request copy size limit isn't documented (the 5 GiB single-part upload limit likely applies; unverified).
- Cross-bucket copy within an account: not stated explicitly on any page I found.

### Multipart

- Part size 5 MiB to 5 GiB (last part may be smaller); **all parts except the last must be the same size** (error 10048
  `InvalidPart`). **Live:** a last part LARGER than the rest is `InvalidPart` too. Source:
  https://developers.cloudflare.com/r2/objects/upload-objects/ (page date 2026-07-29) and the error codes page.
- Max 10,000 parts; single-part upload max 5 GiB; multipart max 4.995 TiB; object max 5 TiB. Source:
  https://developers.cloudflare.com/r2/platform/limits/ (page date 2026-06-08)
- Uploading the same part number again replaces the earlier part; if the second upload fails, the original part is lost.
  Source: S3 API compatibility page.
- `ListMultipartUploads` and `AbortMultipartUpload` are implemented. Source: S3 API compatibility page. **Live:** with a
  bucket-scoped key, `ListMultipartUploads` answers 200 and lists nothing, not even an upload just created.
- Buckets have a **default** lifecycle rule that expires multipart uploads seven days after initiation;
  `AbortIncompleteMultipartUpload` rules are also supported. Source:
  https://developers.cloudflare.com/r2/buckets/object-lifecycles/ (page date 2026-04-21)

### Listing

- `ListObjectsV2` supports `delimiter`, `prefix`, `continuation-token`, `start-after`. Source: S3 API compatibility
  page.
- `ListBuckets` supports `ListObjectsV2`-style search parameters (also as headers) for accounts with over 1,000 buckets.
  Source: extensions page.
- Tokens: "Object Read & Write" / "Object Read only" can be scoped to specific buckets; listing buckets is an Admin
  permission ("Admin Read only" allows listing buckets). Source: https://developers.cloudflare.com/r2/api/tokens/ (page
  date 2026-10-01). What a bucket-scoped token gets back from S3 `ListBuckets` (403 or an empty list) isn't documented.

### DeleteObjects

- Implemented. Max keys per request not documented. Source: S3 API compatibility page.

### Rate limits

- Max one concurrent write per second to the same key; exceeding it returns `429 TooManyRequests` (error 10058).
  `503 ServiceUnavailable` (10043) means retry with backoff. Sources: limits page and error codes page.
- No documented per-bucket or per-account request rate for the S3 API (the Cloudflare REST API is limited to 1,200
  requests per five minutes, which doesn't apply to the S3 endpoint). Source: limits page.

### Presigned URLs

- Expiry 1 second to 7 days (604,800 s); browser `POST` form uploads aren't supported. Source:
  https://developers.cloudflare.com/r2/api/s3/presigned-urls/ (page date 2026-08-22)

### Checksums

- Supported: `CRC64NVME` as `FULL_OBJECT` only; `CRC32`, `CRC32C`, `SHA1`, `SHA256` as `COMPOSITE` only. Source: S3 API
  compatibility page.
- Errors exist for malformed (`InvalidDigest`, 400) and mismatching (`BadDigest`, 400) checksums, so R2 validates rather
  than ignores them. Source: error codes page.
- Cloudflare's own aws-sdk-rust example uses default SDK config with no checksum override (page date 2026-04-21),
  suggesting default SDK checksum headers work. Source: https://developers.cloudflare.com/r2/examples/aws/aws-sdk-rust/

### Metadata

- Object metadata up to 8,192 bytes. Source: limits page.
- R2 RFC 2047-decodes `x-amz-meta-*` values before storing and encodes Unicode values on the way out; metadata keys with
  Unicode written via Workers are stripped over S3 and counted in `x-amz-missing-meta`. Source: extensions page.
- `LastModified` isn't settable (no API for it). Not stated explicitly.

### Key normalization

- R2 **NFC-normalizes object keys** by default: `Héllo` in NFC and NFD forms are the same object in R2 but different
  objects elsewhere. Source: https://developers.cloudflare.com/r2/reference/unicode-interoperability/ (page date
  2026-04-21)

### Storage classes

- `STANDARD` and `STANDARD_IA` (Infrequent Access). No archive class; IA reads immediately but has a retrieval fee and a
  30-day minimum. Sources: https://developers.cloudflare.com/r2/buckets/storage-classes/ (page date 2026-04-21), pricing
  page.

### Endpoint shape

- `https://<ACCOUNT_ID>.r2.cloudflarestorage.com`; region is `auto` (empty and `us-east-1` alias to `auto`). Source: S3
  API compatibility page.
- Jurisdictional buckets (for example EU) need a jurisdiction-specific endpoint, so one client per jurisdiction. Source:
  tokens page.
- Virtual-hosted form `https://<bucket>.<ACCOUNT_ID>.r2.cloudflarestorage.com` appears in the presigned URL example.
  Source: presigned URLs page.
- Strong global consistency for reads, writes, deletes, and lists. Source:
  https://developers.cloudflare.com/r2/reference/consistency/ (page date 2026-04-30)

### Pricing (page date 2026-10-01)

- Source: https://developers.cloudflare.com/r2/pricing/
- Standard storage: $0.015/GB-month. IA: $0.01/GB-month.
- Class A (writes and lists: `PutObject`, `CopyObject`, `ListObjects`, `ListBuckets`, `CreateMultipartUpload`,
  `UploadPart`, `UploadPartCopy`, `CompleteMultipartUpload`, `ListMultipartUploads`, `ListParts`, and more):
  $4.50 per
  million (IA $9.00).
- Class B (`GetObject`, `HeadObject`, `HeadBucket`, and more): $0.36 per million (IA $0.90).
- Free operations: `DeleteObject`, `DeleteBucket`, `AbortMultipartUpload`. `DeleteObjects` is in no list; the price
  table prices it like `DeleteObject` (re-checked 2026-10-01, "Last updated Oct 1, 2026").
- Egress: free for all classes. IA data retrieval: $0.01/GB.
- Free tier per month: 10 GB-month storage, 1 million Class A, 10 million Class B.
- Minimum storage duration: none for Standard, 30 days for IA. No minimum object size mentioned.
- Unauthorized requests (401) aren't billed.

## Backblaze B2 (S3-compatible API)

B2's per-operation API reference pages (`/apidocs/s3-*`) are thin: they list only a subset of headers. Several answers
below therefore rest on corroboration.

### Conditional writes

- `If-None-Match` / `If-Match` aren't listed among the `S3 Put Object` request headers. Source:
  https://www.backblaze.com/apidocs/s3-put-object
- **Corroboration** (Reddit, r/backblaze, LanceDB thread, undated in the excerpt): "B2 still returns 501 Not Implemented
  for If-None-Match."
  https://www.reddit.com/r/backblaze/comments/1psaizc/using_backblaze_b2s3_with_lancedb_0170_as_direct/
- Verdict: **not supported** for `PutObject`; `CompleteMultipartUpload` and `CopyObject` unknown, assume not.

### CopyObject

- Supported; checksum of the source is maintained on copy, and changing the checksum algorithm via copy isn't supported;
  `x-amz-tagging` is rejected, `x-amz-tagging-directive` ignored. Source:
  https://www.backblaze.com/apidocs/s3-copy-object and https://www.backblaze.com/docs/cloud-storage-s3-compatible-api
- `UploadPartCopy` supported. Source: https://www.backblaze.com/docs/cloud-storage-call-the-s3-compatible-api
- Single-request size limit not stated on the S3 page; B2 caps normal (non-large) files at 5 GB. Source:
  https://www.backblaze.com/docs/cloud-storage-large-files
- Cross-bucket copy: `x-amz-copy-source` takes `bucket/key`, but no page states cross-bucket support explicitly.

### Multipart

- Large files: 5 MB to 10 TB; parts 5 MB to 5 GB; at least two parts; every part except the last at least 5 MB. Source:
  https://www.backblaze.com/docs/cloud-storage-large-files
- Max parts: 10,000 (B2 native docs elsewhere; not on the page I read, so unverified here).
- `ListMultipartUploads` and `AbortMultipartUpload` are supported (listed). Source: call-the-S3-API page.
- Auto-cancel: lifecycle rule `daysFromStartingToCancelingUnfinishedLargeFiles` (can't be zero);
  `Put Lifecycle Configuration` exists in the S3 API. Source:
  https://www.backblaze.com/docs/cloud-storage-lifecycle-rules

### Listing

- `ListObjectsV2` supported. Source: call-the-S3-API page. Delimiter support isn't spelled out but is required by every
  S3 tool B2 documents (rclone, Cyberduck).
- `ListBuckets` with a key restricted to one bucket requires the `listAllBucketNames` capability ("required for
  compatibility with SDKs and integrations"). Without it the call fails; the exact status isn't documented. Source:
  https://www.backblaze.com/docs/cloud-storage-s3-compatible-app-keys

### DeleteObjects

- Supported (`POST /?delete`, `Quiet` element). Max keys not documented. Source:
  https://www.backblaze.com/apidocs/s3-delete-objects
- **Versioning is on by default**: B2 "retains all of the files that you upload and all of the different versions", and
  `DeleteObject` without a version ID returns `x-amz-delete-marker: true` (a hide marker). Older versions are kept
  forever unless deleted or a lifecycle rule removes them. So a plain delete doesn't free storage. Sources: lifecycle
  rules page and https://www.backblaze.com/apidocs/s3-delete-object

### Rate limits

- New accounts default to 500 requests per second for upload and download operations; S3 API returns `503 SlowDown` (may
  include `Retry-After`). Source: https://www.backblaze.com/docs/cloud-storage-rate-limits

### Presigned URLs

- Supported for download and upload; browser `POST` uploads aren't. Max expiry isn't documented on B2's pages. Source:
  S3-compatible API page.

### Checksums

- **Corroboration** (Stack Overflow, Drupal and pingvin-share issues): B2 rejected `x-amz-checksum-crc32` with
  "Unsupported header" and implemented these headers in July 2025.
  https://stackoverflow.com/questions/79606343/laravel-11-backblaze-b2-s3-compatible-put-error-unsupported-header-x-am
- B2's own docs now reference checksum handling (copy preserves the source's checksum algorithm;
  `Put Lifecycle Configuration` requires a matching `x-amz-checksum` or `x-amz-trailer` when the algorithm header is
  sent). Sources: `s3-copy-object` page and https://www.backblaze.com/apidocs/s3-put-lifecycle-configuration (search
  excerpt).
- `CRC64NVME` support: not verified.

### Metadata

- File name plus all file info limited to 7,000 bytes of encoded header in most cases; 2,048 bytes of file info for
  SSE-C-enabled buckets (the excerpt's qualifier was cut). Source:
  https://www.backblaze.com/docs/cloud-storage-file-information

### Storage classes

- No archive classes; B2 is a single always-hot tier ("No minimum storage duration fees"). Source: pricing page.

### Endpoint shape

- `https://s3.<region>.backblazeb2.com` (for example `us-east-005`), HTTPS only; both virtual-hosted
  (`bucketname.s3.<region>.backblazeb2.com`) and path-style work. Source: call-the-S3-API page.

### Pricing

- Source: https://www.backblaze.com/cloud-storage/pricing and
  https://www.backblaze.com/cloud-storage/transaction-pricing
- Storage: $6.95/TB-month pay-as-you-go; first 10 GB free.
- Class A (writes, deletes, `CompleteMultipartUpload`), B (`GetObject`, `HeadObject`), and C (`CopyObject`,
  `UploadPartCopy`, `ListObjectsV2`, `ListBuckets`, `ListMultipartUploads`, and more): **free** for pay-as-you-go.
- Class D (event notifications): first 2,500/day free, then $0.004 per 10,000.
- Egress: free up to 3x average monthly storage, then $0.01/GB.
- No minimum file size fees, no minimum storage duration fees.
- Note: the raw HTML of the transaction page still contains an older
  "$0.005/GB/month" string that client-side script
  replaces with $0.00695; use $6.95/TB.

## Wasabi

Wasabi docs claim the service is "100% bit-compatible" with AWS S3 and IAM. Source:
https://docs.wasabi.com/apidocs/wasabi-api

### Conditional writes

- No Wasabi page mentions `If-None-Match` on writes. **Unknown**; probe at connect time.

### Wasabi-specific operations (relevant to a file manager)

- **Server-side rename**: HTTP `MOVE` with `Destination`, renames an object (all versions); `X-Wasabi-Prefix: true`
  renames every key under a prefix (a whole "folder") in one call. `Overwrite` defaults to `false` (error if the
  destination exists). Needs `s3:PutObject`. Source: https://docs.wasabi.com/apidocs/operations-on-objects (page date
  2026-02-03)
- **Compose** (`PUT ?compose`, up to 32 data objects, same bucket, linked not copied) and **append** (`PUT ?append`, up
  to 1,023 times). Same source.
- Bucket rename via `MOVE` on the bucket. Source: https://docs.wasabi.com/apidocs/operations-on-buckets-1
- The docs don't say whether `MOVE` is atomic or how it interacts with the 90-day minimum.

### CopyObject

- No Wasabi-specific limits documented; assume AWS semantics (5 GB single copy). Unverified.

### Multipart

- Part size 5 MB to 5 GB. Incomplete multipart uploads are auto-deleted after about 31 days;
  `AbortIncompleteMultipartUpload` lifecycle rules are supported. Incomplete parts are billed as active storage for 30
  days, then (on a 90-day plan) as deleted storage for 60 more days. Source:
  https://docs.wasabi.com/docs/how-does-wasabi-handle-multipart-uploads (page date 2026-02-05)
- Max parts, `ListMultipartUploads`: not documented beyond "supports the AWS S3 multipart API".

### Listing, DeleteObjects

- Not separately documented; assumed AWS-compatible per the "bit-compatible" claim. Unverified.

### Rate limits

- The REST API error table names `RequestRateLimitExceeded: Your account exceeded the limit for {request_type} requests`
  and `TemporarilyUnavailable` (503). The scraped table was garbled, so the status code for the rate-limit error isn't
  reliable. Source: https://docs.wasabi.com/apidocs/rest-api-introduction (page date 2026-02-03)
- No published request-rate numbers. The free egress and API policy reserves the right to limit or suspend
  "non-validated" applications that impose "inefficient and unreasonable load". Source: https://wasabi.com/pricing/faq

### Presigned URLs

- Max 7 days (168 hours) with an IAM user. Source:
  https://docs.wasabi.com/docs/how-do-i-generate-pre-signed-urls-for-temporary-access-with-wasabi (page date 2026-05-08)

### Checksums

- Wasabi **rejects `CRC64NVME`** ("The algorithm type you specified in x-amz-checksum- header is invalid") and says it's
  working on it; workaround is forcing `CRC32`. Source: https://docs.wasabi.com/docs/how-do-i-use-aws-cli-with-wasabi
  (page date 2026-09-23)

### Metadata, archive classes

- Metadata size limit: not documented. Archive classes: none documented (single hot tier). Unverified.

### Endpoint shape

- Region-specific hosts `s3.<region>.wasabisys.com`; a wrong-region URL allows `GET` but not `PUT` or `DELETE`. Both
  path-style and virtual-hosted work; Wasabi recommends path-style. Sources: wasabi-api page and rest-api-introduction
  page.

### Pricing

- Source: https://wasabi.com/pricing/faq
- Storage: $7.99/TB-month (US and Europe pay-as-you-go example; computed per GB per day). The FAQ's math: $7.99 / 1,024
  GB = $0.0078/GB-month, / 30 days = $0.000260091/GB-day, which is what an early deletion bills per remaining day
  (re-checked 2026-10-01).
- Egress and API requests: free, subject to policy: monthly egress should not exceed active storage volume.
- **Minimum monthly charge**: 1 TB of active storage.
- **Minimum storage duration**: 90 days on pay-as-you-go. Deleting earlier triggers a "Timed Deleted Storage" charge for
  the remaining days. **Overwriting** an object also moves the old copy to deleted storage (charged 29 or 89 more days
  depending on the plan).
- **Minimum billable object size**: 4 KB.

## Hetzner Object Storage

### Compatibility exceptions (page date 2024-09-22)

- Source: https://docs.hetzner.com/storage/object-storage/supported-actions
- **`CopyObject` is only supported within the same bucket.** **Live:** `CopyObject` and `UploadPartCopy` between two
  buckets of one location (`nbg1`) both worked.
- Not supported: `Restore`, `Select`, `GetObjectAttributes`, tagging, copying SSE-C objects, conditional `PUT`/`DELETE`
  on **versioned** buckets, `CreateSession`.
- Only SSE-C encryption.

### Conditional writes

- Only exclusion listed is conditional `PUT`/`DELETE` on versioned buckets, which implies unversioned buckets support
  them. Backend is Ceph (overview page). **Live:** `If-None-Match: *` is enforced on `PutObject` (412) and ignored on
  `CompleteMultipartUpload` and `CopyObject`.

### Limits (page date 2024-09-23)

- Source: https://docs.hetzner.com/storage/object-storage/overview
- Metadata up to 8 KB per object.
- Single `PUT` up to 5 GB; part up to 5 GB; 10,000 parts; object up to 5 TB.
- 256 parallel TCP sessions per source IP.
- **750 requests/s per source IP and 750 requests/s per bucket.**
- 10 Gbit/s per bucket (read or write).
- 100 TB and 50,000,000 objects per bucket; 100 buckets and 200 S3 credentials per account.
- Minimum part size isn't documented.

### Multipart cleanup

- Leftover parts from aborted uploads count toward bucket size and billing; clean them with lifecycle policies or
  `list-multipart-uploads` plus abort. Source: https://docs.hetzner.com/storage/object-storage/faq/general

### Listing

- Keys and endpoints are per location: listing buckets through `fsn1` with a key shows only buckets of that key's
  project in that location. Source: https://docs.hetzner.com/storage/object-storage/getting-started/using-s3-api-tools
  (page date 2025-01-07)

### DeleteObjects, rate-limit response, presigned expiry, checksums, archive classes

- Not documented. No `Restore` means no archive classes.

### Endpoint shape

- `fsn1.your-objectstorage.com` (Falkenstein), `nbg1` (Nuremberg), `hel1` (Helsinki); public URLs are
  `https://<bucket>.<location>.your-objectstorage.com/<key>`. Region names are `fsn1`/`nbg1`/`hel1`. Path-style works
  (tool examples use it). Source: overview and using-s3-api-tools pages.

### Pricing

- Source: https://www.hetzner.com/storage/object-storage/ and the overview page.
- Base price €6.49/month (excl. VAT), billed hourly with a monthly cap, charged for every hour with at least one bucket
  (even empty). Includes 1 TB storage (744 TB-hours) and 1 TB egress.
- Extra storage: €0.0087 per TB-hour. Extra egress: €1.00 per TB.
- S3 operations (`PUT`, `GET`, `DELETE`): free. Ingress free.
- **Minimum billable object size: 64 KB.** Metadata counts toward billed size.
- No minimum storage duration mentioned.

## Google Cloud Storage (XML API, HMAC keys)

- **Interoperability**: the XML API at `storage.googleapis.com` speaks SigV4 with HMAC keys (a service account's or a
  user's, made under Cloud Storage › Settings › Interoperability). Region `auto` signs fine. Source:
  https://cloud.google.com/storage/docs/interoperability (accessed 2026-10-02).
- **Copies**: `x-amz-copy-source` works; a large copy across locations or storage classes may need several JSON-API
  `rewrite` calls, which the XML API can't express. Source:
  https://cloud.google.com/storage/docs/json_api/v1/objects/rewrite.
- **Bucket names** may hold dots and underscores (3–63 characters, up to 222 with dots). Source:
  https://cloud.google.com/storage/docs/buckets#naming.
- **Live** (2026-10-02): no `UploadPartCopy` (400 `NotImplemented`); `If-None-Match` ignored on every write;
  `x-goog-if-generation-match` refused beside `x-amz-*` headers (400 `ExcessHeaderValues`); `DeleteObjects` works up to
  1,000 keys; a bodyless POST needs `Content-Length: 0` (411 otherwise); metadata stays `x-amz-meta-*`.

### Pricing (accessed 2026-10-02)

- Source: https://cloud.google.com/storage/pricing. Standard storage in one region (us-central1): $0.020/GB-month.
- Class A $0.005 per 1,000 (every XML `PUT` and `POST`, listing objects), Class B $0.0004 per 1,000 (`GET`, `HEAD`),
  `DELETE` free. A multi-object delete is a `POST`, so Class A.
- Internet egress $0.12/GB for the first 10 TiB (worldwide excluding Asia and Australia).
- Always Free (US regions): 5 GB-months, 5,000 Class A, 50,000 Class B, 100 GB egress from North America.
- Soft delete keeps deleted objects seven days by default and bills them meanwhile.

## DigitalOcean Spaces

- **Endpoint**: `https://<region>.digitaloceanspaces.com`, region = the slug. Standard Storage regions: nyc3, ams3,
  sfo2, sfo3, sgp1, lon1, fra1, tor1, blr1, syd1, atl1, ric1, mkc1. Source:
  https://docs.digitalocean.com/products/spaces/details/availability/ (generated 2026-10-01).
- **Copies**: "Supported with `CopyObject`. Cross-region and cross-cluster copies are not supported." Source:
  https://docs.digitalocean.com/products/spaces/reference/s3-compatibility/ (accessed 2026-10-02).
- **Live** (2026-10-02): `If-None-Match` enforced on `PutObject` only; `UploadPartCopy` works and ignores
  `x-amz-copy-source-if-match`; the test key was bucket-scoped (`ListBuckets` and `CreateBucket` refused).

### Pricing (page date 2026-07-13)

- Source: https://docs.digitalocean.com/products/spaces/details/pricing/.
  $5.00/month subscription with 250 GiB of
  storage and 1,024 GiB of outbound transfer shared by every bucket; then $0.02/GiB-month
  and $0.01/GiB. No per-request fees.

## Server-side copy throughput

- No provider publishes per-request `CopyObject` / `UploadPartCopy` throughput (MB/s). AWS documents only request rates
  (3,500 `COPY` per second per prefix). Hetzner's 10 Gbit/s per-bucket cap is the only bandwidth number found. Measured
  numbers for every named provider: `crates/cmdr-s3/DETAILS.md` § "Verified providers".

## Surprises / risks for Cmdr

- **`If-None-Match: *` is not portable.** AWS supports it on all three write paths. R2 on `PutObject` and (live)
  `CompleteMultipartUpload`, with `cf-copy-destination-if-none-match` for `CopyObject`. Hetzner and Spaces on
  `PutObject` only (live); GCS nowhere (live). B2 rejects it (501, per corroboration); Wasabi is unknown. The
  no-overwrite guarantee needs a per-provider allowlist and a HEAD-then-write fallback with a documented race window.
- **Default SDK checksums break Wasabi.** The aws-sdk default `CRC64NVME` is rejected by Wasabi (page date 2026-09-23).
  R2 only accepts `CRC64NVME` as full-object and CRC32/CRC32C/SHA as composite. B2 only gained checksum headers in July
  2025 (corroboration). Pin a conservative `request_checksum_calculation` per provider.
- **R2 NFC-normalizes keys.** macOS hands Cmdr NFD names from some sources; an NFD key and its NFC twin collide in R2
  but stay distinct on other providers. Listing an R2 bucket returns NFC, so round-trips may "rename" files.
- **R2 multipart parts must be equal-sized** (except the last). A part-size scheme that varies sizes, or a resume that
  re-chunks, fails at `CompleteMultipartUpload` with `InvalidPart`. Re-uploading a part number can lose the old part.
- **Hetzner's docs say `CopyObject` is same-bucket only**, but a live cross-bucket copy within one location worked. 750
  req/s per bucket is low for parallel small-file operations. Spaces documents no cross-cluster copy, so cross-bucket
  copies stream there.
- **Rename = copy + delete is expensive on Wasabi.** The 90-day minimum and the overwrite rule mean renaming or
  overwriting a fresh object bills up to 89 extra days of the old copy. Wasabi's native `MOVE` (with `X-Wasabi-Prefix`
  for whole folders) avoids the copy, but billing impact is undocumented.
- **B2 deletes are soft by default.** Buckets keep all versions, so `DeleteObject` only writes a delete marker and the
  bytes stay billed. Users will expect "delete" to free space.
- **Minimum object size billing**: Hetzner 64 KB, Wasabi 4 KB. Folder marker objects and tiny files cost more than they
  look. Wasabi also bills a 1 TB monthly minimum.
- **AWS `Last-Modified` for multipart objects is the MPU initiation time**, so a long upload shows an earlier mtime than
  a single `PUT` would. No provider lets Cmdr set mtime; preserving it needs `x-amz-meta-*` (2 KB cap on AWS, 7,000-byte
  combined header on B2).
- **`CopyObject` can fail inside a `200 OK`** on AWS; the response body must always be parsed.
- **Bucket-scoped keys can't list buckets** on B2 (needs `listAllBucketNames`) and on R2 (listing is Admin-only); on
  Hetzner, listing only shows the key's project and location. The connect flow needs a "type the bucket name" path.
- **AWS object size limit is now 50 TB** (48.8 TiB) and R2's is 5 TiB, B2's 10 TB, Hetzner's 5 TB; size checks shouldn't
  hardcode 5 TiB.

## Couldn't verify

- R2: single-request `CopyObject` size limit; cross-bucket `CopyObject` (the test key reaches one bucket).
- B2 (no account): conditional writes from an official source (only corroboration found); `CRC64NVME` support; presigned
  URL max expiry; `DeleteObjects` max keys; max parts on the S3 API; cross-bucket `CopyObject`; HTTP status for
  `ListBuckets` without `listAllBucketNames`; whether a short PUT is refused.
- Wasabi (no account): conditional writes (`If-None-Match`) on any operation; `CopyObject` size limit; `DeleteObjects`
  max keys; metadata size limit; numeric rate limits and the rate-limit status code; whether `MOVE` is atomic and how
  it's billed under the 90-day rule; whether any non-hot storage class exists; the wrong-region behavior.
- Hetzner: presigned URL max expiry past seven days; rate-limit response code (503 vs 429; nothing throttled at 16 parts
  in flight); minimum part size beyond "5 MiB before the last part is refused".
- GCS: a large cross-location copy through the XML API (the test bucket is the only one the key reaches).
- Spaces: a cross-bucket copy within one region (the test key is bucket-scoped).
- AWS (no account): the exact response for `ListBuckets` with a bucket-scoped IAM policy (403 inferred from the required
  permission); whether third-party providers accept a flexible checksum in place of `Content-MD5` on `DeleteObjects`; a
  bucket in another region reached from the account root (tested against a fake AWS only).
