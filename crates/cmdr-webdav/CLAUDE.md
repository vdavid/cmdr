# cmdr-webdav

The WebDAV backend: a `Volume` over one `reqwest` client with one account's Basic credentials on it. Same shape as
`crates/cmdr-sftp`, leaner: no host keys, no auth ladder, no extensions probe. Decisions and the full tables:
`DETAILS.md`. The Docker fixture stack: `apps/desktop/test/webdav-servers/start.sh`.

## Module map

- `params.rs`: `WebdavConnectionParams` (base URL, username, root under it) and the store key `scheme://host:port`
  scoped by username.
- `errors.rs`: `WebdavConnectError` and the status-code table (`map_status`, keyed by an `Attempted` context).
- `transport.rs`: `WebdavClient`, URL building, PROPFIND, the connect probe. `propfind.rs`: the `multistatus` parser.
- `volume/`: `mod.rs` (the volume, `connect_webdav_volume`, `send`), `paths.rs`, `query.rs`, `streams.rs` (GET),
  `writes.rs` (staged PUT + MOVE), `mutation.rs`, `copy.rs`, `scan.rs`, `state.rs` + `reconnect.rs`, `volume_impl.rs`,
  `testing.rs` (fixtures, `testing` feature).

## Must-knows

- ❗ **`reqwest` stays in `transport.rs`, `errors.rs` (status codes, typed predicates), `streams.rs`, and `writes.rs`
  (the two streaming bodies), and `volume/mod.rs`'s `send`.** Everything else works in `Url`s, `StatusCode`s, and
  `PropfindEntry`s. Its features are EXACTLY a subset of the app's; a second configuration would enter the graph twice.
- ❌ **Never classify by message.** A status is judged by number plus `Attempted` (what the request was trying to do); a
  `reqwest::Error` by `is_timeout` / `is_connect` / `is_request`; a TLS refusal by the `io::ErrorKind::InvalidData` in
  its source chain.
- ❌ **No `read_timeout`, and no `.timeout()` on the streaming PUT or GET**: both would cut a long transfer. The
  non-streaming verbs carry `MUTATION_BUDGET` (10 min), PROPFIND `PROPFIND_BUDGET` (60 s); `transport.rs` has why.
- ❗ **Every wire-touching delegator wraps itself in `noting`.** There is no watcher and no session: the operations ARE
  the disconnect detector. A `DeviceDisconnected` flips the state once and starts the backoff loop.
- ❌ **One unattended authentication attempt, never a loop.** A 401 on the re-probe moves to `NeedsCredentials` and
  stops. The store is only ever refreshed by an attended sign-in, never seeded.
- ❗ **Redirects are off.** A followed MOVE or COPY would resend `Destination` somewhere the user never named. A
  PROPFIND on a slash-less collection that answers 3xx is retried once with the slash.
- ❗ **PUT sends `Content-Length` from `size`**, never a body of unknown length. Every write lands on a `.cmdr-tmp-*`
  sibling and is MOVEd into place with `Overwrite: T`. ❌ A source whose byte count disagrees with `size` is never
  MOVEd: hyper truncates a longer body and the server stores the prefix happily.
- ❗ **The upload body reads one piece AHEAD, and that is load-bearing.** hyper stops polling a body once
  `Content-Length` is satisfied, so a source that pieces up exactly on `size` would otherwise look honest while the
  server holds a prefix. The read-ahead is why there are two counters: `fetched` guards the size, `handed` (clamped)
  drives progress. ❌ Never collapse them. `DETAILS.md` § "Write staging".
- ❗ **`Range` may be ignored.** A 200 to a ranged GET is handled by skipping locally; `read_range` never returns more
  than `len` and drops the response as soon as the window is full. The server that makes that branch run is
  `webdav-fixture-norange`, port 13483. What a real server answers to that and to a chunked PUT, with dates:
  `DETAILS.md` § "What a real server answers".
- ❗ **DELETE is recursive by protocol; the trait's is not.** `delete` refuses a non-empty collection with `ENOTEMPTY`
  after a `Depth: 1` PROPFIND.
- ❌ Never `root_anchored`, never a stat per child in a scan, never `authoritative_listing` (coverage is `None`).
- Digest-only servers are a typed `AuthMethodUnsupported`, by the `WWW-Authenticate` scheme token.
