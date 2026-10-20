# cmdr-webdav

The WebDAV backend: a `Volume` over one `reqwest` client with one account's credentials. Same shape as
`crates/cmdr-sftp`, minus host keys, the auth ladder, and the extensions probe. Fixtures:
`apps/desktop/test/webdav-servers/start.sh`.

## Module map

- `params.rs`: `WebdavConnectionParams` and the store key (`scheme://host:port`, scoped by username). `https_first.rs`:
  `http` tries `https` first.
- `errors.rs`: `WebdavConnectError`, the status table (`map_status` by `Attempted`).
- `transport.rs`: `WebdavClient`, PROPFIND, connect probe. `propfind.rs`: `multistatus` parsing. `urls.rs`: remote paths
  as percent-encoded URLs. `chunking.rs`: Nextcloud's chunked-upload URLs and plan.
- `volume/`: `mod.rs` (the volume, `connect_webdav_volume`, `send`), `paths.rs`, `query.rs`, `streams.rs` (GET),
  `writes.rs` (the PUT), `chunked_upload.rs`, `mutation.rs`, `delete_aside.rs`, `copy.rs`, `scan.rs`, `reconnect.rs`
  (`http_session` hooks), `volume_impl.rs`, `testing.rs` (fixtures, `testing` feature).

## Must-knows

- ❗ **`reqwest` stays in `transport.rs`, `errors.rs`, `streams.rs`, `writes.rs`, `chunked_upload.rs`, and
  `volume/mod.rs`'s `send`.** Everything else works in `Url`s, `StatusCode`s, and `PropfindEntry`s. Its features are
  EXACTLY a subset of the app's; a second configuration would enter the graph twice.
- ❌ **Never classify by message.** A status by number plus `Attempted`; a `reqwest::Error` by `is_timeout` /
  `is_connect` / `is_request`; a TLS refusal by the `io::ErrorKind::InvalidData` in its source chain.
- ❗ **Every wire-touching delegator wraps itself in `noting`**: it races each operation against the client's `lost`
  token, so a SILENT server (30 s) cuts them with `DeviceDisconnected`. `DETAILS.md` § "Silent or slow".
- ❗ **Every request goes out through `WebdavClient::send` or `propfind`**, and a body read counts its chunks as
  `heard`, or the silence watch never hears it.
- ❗ **A buffered body goes through `read_body` with a cap**, ❌ never `.text()` / `.bytes()`.
- ❌ **No `read_timeout`, no `.timeout()` on the streaming PUT or GET, and never a timeout read as a lost server.**
  `transport.rs` has why.
- ❌ **One unattended auth attempt, never a loop.** A 401 on the re-probe moves to `NeedsCredentials` and stops.
- ❗ **A cookie jar AND Basic on every request**: sessionless, Nextcloud hashes the password per request (~150 ms vs
  ~20). `DETAILS.md` § "Session cookies".
- ❗ **Redirects are off**: a followed MOVE or COPY would resend `Destination` elsewhere.
- ❗ **The engine owns staging**: `CreateOrReplace` is one plain PUT onto its temp; ❌ don't stage it again. Only
  `CreateNew` stages here (MOVE `Overwrite: F`). `DETAILS.md` § "Write staging".
- ❗ **PUT sends `Content-Length` from `size`.** ❌ A byte count that disagrees fails the write and removes its target:
  hyper truncates a longer body and the server keeps the prefix.
- ❗ **The upload body reads one piece AHEAD** (hyper stops polling at `Content-Length`): `fetched` guards the size,
  `handed` (clamped) drives progress. ❌ Never collapse them.
- ❗ **On a Nextcloud (the probe's `X-User-Id`), a file past 64 MiB goes up in chunks**; the target sees nothing until
  the assembling MOVE. `DETAILS.md` § "Chunked uploads (Nextcloud)".
- ❗ **`Range` may be ignored**: a 200 to a ranged GET is skipped locally (fixture `webdav-fixture-norange`).
- ❗ **Unbounded space reads `oc:size`, ❌ never `quota-used-bytes`** (an unlimited Nextcloud answers `0`). One of two
  non-standard properties; `DETAILS.md` § "What a real server answers" sets the bar for a third.
- ❗ **Dates beyond the mtime only where the connect probe saw `nc:creation_time`**; ❌ never a PROPPATCH of dates on
  `mod_dav`, which keeps them as dead properties. `DETAILS.md` § "Dates".
- ❗ **DELETE and a forced MOVE are recursive; the trait's verbs aren't.** Both refuse a held collection, then move it
  aside, ❌ no ETag (`DETAILS.md` § "A folder's DELETE").
- ❌ Never `root_anchored`, never a stat per child in a scan, never `authoritative_listing` (coverage is `None`).
- Digest-only servers are a typed `AuthMethodUnsupported`, by the `WWW-Authenticate` scheme token.

Decisions and full tables: `DETAILS.md`; read it before non-trivial work here.
