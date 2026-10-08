# cmdr-s3

The S3 backend for AWS and every S3-compatible server (DETAILS § Providers): the protocol plus a `Volume` per place (a
bucket, or an account root listing them). Plan: `docs/specs/s3-support-plan.md`. Fixtures:
`apps/desktop/test/s3-servers/`.

## Module map

- `sigv4.rs`, `encoding.rs`, `request.rs`, `ops.rs` (one builder per S3 call), `profile.rs` (preset → endpoint and what
  the provider enforces), `xml/`, `error.rs` (`S3Error`), `multipart.rs`, `metadata.rs`: pure values.
- `params.rs`, `refusal.rs` (`S3ConnectError` + the probe's table), `transport/` (`S3Client`; `answer.rs` reads what
  comes back; the only `reqwest` user), `routing.rs` (an AWS or Wasabi account root's per-bucket regions).
- `volume/`: a file per job: `mod.rs` (connect), `query.rs` + `listing.rs`, `streams.rs` (GET), `writes.rs` (PUT),
  `multipart_upload.rs` (+ the sweep), `server_copy.rs`, `batch.rs`, `mutation.rs`, `paths.rs`, `state.rs` +
  `reconnect.rs`; `testing.rs` + `testing/` (fixtures, `S3Target`) and `live_*` (real accounts).
- `cost/`: the price table (`s3-prices.json`, byte-identical to `apps/api-server`'s), `Workload` (requests counted the
  way the write paths send them), `Estimate`. ❗ A write path that sends a request more or less updates its `Workload`
  method too.

## Must-knows

- ❌ **Never classify by `<Message>`.** `<Code>` plus the status; a bodyless answer (every HEAD) by status alone.
- ❗ **`reqwest` stays in `transport/`**; every request goes through it inside `noting` (operations detect liveness).
- ❗ **Every request costs the user money.** ❌ No HEAD per child, no watcher, no space poll, no index.
- ❗ **A wrong secret is ambiguous**: Garage answers `AccessDenied`, so only `SignatureDoesNotMatch` /
  `InvalidAccessKeyId` (or R2's 401) are `KeysRejected`.
- ❌ **No `.timeout()` on a GET or an upload, never a buffered body** beyond one part. A 200 to a ranged GET is skipped
  locally. A buffered answer is capped (`read_capped`, `DETAILS.md` § "Responses").
- ❗ **Parse every success body**: Complete, CopyObject, UploadPartCopy, DeleteObjects can fail inside `200 OK`.
- ❗ **Keys are never trimmed**; a `.`/`..` segment is refused (`KeyError::DotSegment`).
- ❗ **Writes go to the final key**. ❌ Nothing partial is ever published: bodies are buffered (≤ one part), a PUT holds
  its last piece for a Cancel check, and a cut-off PUT removes only what carries its own write token.
- ❗ **Pause parks requests (`stop_signal`), ❌ never holding one past `PAUSE_HOLD`**: R2 drops a silent body at ~15 s.
- ❗ **Profile capabilities are allowlists from live evidence, ❌ never a probe** (Garage answers 200 to an ignored
  `If-None-Match`). Re-verify with `apps/desktop/test/s3-servers/live.sh` first. Off the list, `CreateNew` HEADs.
- ❗ **An upload is recorded before its first part**, and an abort counts only once a listing confirms it; the sweep
  aborts ❌ only recorded uploads, ❌ never one in flight.
- ❗ **A file beside a folder of its name lists as `<name> (file)`**: resolve paths through `paths.rs::resolve`, ❌
  never `RemoteRoot` directly, or the folder row reaches the file.
- ❗ **`delete` is one node** (`ENOTEMPTY` while keys sit under a folder). **`rename` moves one small file**; a folder
  or an object past the part floor is `RenameWork::CopyThenDelete`, which callers send through the engine.
- ❗ **An overwrite of an existing object off the `refuses_short_body` allowlist goes as a multipart upload**, even one
  part: VersityGW publishes a cut-off PUT, which would lose the original.
- ❗ **Server-side copy stays within one account**, matched on the concrete `S3Volume`, ❌ never a path. GCS has no
  `UploadPartCopy`: one `CopyObject` there.
- ❗ **No checksum headers; equal-size parts** (R2); a short tail follows the provider's `ShortTail`. Streamed bodies
  sign `UNSIGNED-PAYLOAD`.
- ❌ **One unattended auth attempt, never a loop.**
- ❗ **A share link is a credential**: `cmdr_fs::volume::ShareLink`, ❌ never logged, never across IPC.

Decisions and evidence: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
