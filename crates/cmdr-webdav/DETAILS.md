# cmdr-webdav details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions.

## The connection model

HTTP holds no session. "Connected" means the last request that reached the wire came back; "disconnected" means one
failed with a transport error (`reqwest::Error::is_connect` / `is_request`, mapped to
`VolumeError::DeviceDisconnected`). `connect_webdav_volume` reads the account's secret from the `CredentialStore`
(service `scheme://host:port`, scope `username`; nothing stored is `NeedsCredentials`), builds a `WebdavClient`
(`user_agent("Cmdr")`, a 10 s connect timeout and no `read_timeout`, redirects off, Basic auth on every request), and
proves it with one `PROPFIND Depth: 0` on the root. The probe rides `tokio::select!` against the cancel token; a cancel
leaves nothing behind. On success the backend records the PII-free analytics event `webdav_connected`.

The probe's answers, in connect terms:

- **A transport failure, `is_timeout`**: `TimedOut`.
- **A transport failure, `is_connect` with an `InvalidData` `io::Error` in the source chain**: `CertificateUntrusted`.
- **Any other transport failure, `is_connect` / `is_request`**: `Unreachable`.
- **207 with a `multistatus`**: connected.
- **207 without one, 200, 404, 405, any other 4xx**: `NotAWebdavServer`.
- **401 carrying a `Basic` challenge**: `AuthenticationRejected`.
- **401 carrying none (a Digest-only server)**: `AuthMethodUnsupported`.
- **403**: `AuthenticationRejected`.
- **5xx**: `Transport`.

## The error table

`errors::map_status(status, path, attempted)`. `Attempted::TakingAName` is set by `create_file` (`If-None-Match: *`),
`create_directory` (MKCOL), and a no-clobber MOVE (`Overwrite: F`); everything else is `Reaching`.

| Status  | `Reaching`            | `TakingAName`   |
| ------- | --------------------- | --------------- |
| 401/403 | `PermissionDenied`    | same            |
| 404     | `NotFound`            | same            |
| 405     | `NotSupported`        | `AlreadyExists` |
| 409     | `NotFound` (ancestor) | same            |
| 412     | `IoError`             | `AlreadyExists` |
| 423     | `IoError` errno EBUSY | same            |
| 501     | `NotSupported`        | same            |
| 507     | `StorageFull`         | same            |
| other   | `IoError("HTTP nnn")` | same            |

The two path-carrying variants carry the path, never the server's wording (`crates/cmdr-sftp/DETAILS.md` § the error
policy has the reasoning; it is the same here). Transport errors: timeout → `ConnectionTimeout(path)`; connection gone →
`DeviceDisconnected(volume_id)`; body/decode → `IoError`. Per-request budgets: 60 s on a PROPFIND (`PROPFIND_BUDGET`),
10 min on MOVE, COPY, DELETE, MKCOL, and `create_file`'s in-memory PUT (`MUTATION_BUDGET`); the streaming PUT and GET
have none (`transport.rs` has the `read_timeout` reasoning, `streams.rs` the download idle budget).

A `multistatus` entity (`&amp;`) reaches the parser as its own `Event::GeneralRef`, never inside a text node, so
`propfind.rs` resolves it there; a text-level `unescape` would silently drop the character.

TLS: `tokio-rustls` surfaces every handshake refusal as an `io::Error` of kind `InvalidData`, so `CertificateUntrusted`
covers any TLS refusal, of which a self-signed NAS is by far the commonest. Narrowing it to trust alone would need a
`rustls::Error` downcast, which means a direct `rustls` dependency; not taken yet.

## Path handling

`paths.rs` is `cmdr-sftp`'s translation with a different root: `remote_root` is normalized to `/` or `/Photos` under the
base URL, `..` is resolved lexically BEFORE the containment check, the root is matched by whole components, and anything
outside is `NotFound` (never anchored). The result is a root-relative remote path; `WebdavClient::url_for`
percent-encodes each segment (everything but unreserved characters) and appends a trailing slash for collections.
`FileEntry.path` is the decoded remote path, the same string the app addresses the entry by.

Listings are one `PROPFIND Depth: 1` with a body naming
`resourcetype, getcontentlength, getlastmodified, creationdate, getetag, quota-available-bytes, quota-used-bytes`.
`propfind.rs` reads the `DAV:` namespace under any prefix, takes properties only from 2xx `propstat`s, percent-decodes
`href`s, and reduces absolute-URL hrefs to their path. The self entry is dropped by comparing decoded, slash-normalized
paths, never by position.

## Write staging

`write_from_stream` PUTs to `<dest>.cmdr-tmp-<pid><nanos><n>` with a streaming body wrapped from the source's chunks and
`Content-Length: size`, reports progress every 200 ms from a shared counter, cancels by poisoning the body stream (the
request aborts, the temp is DELETEd), then MOVEs the temp onto `dest` with `Overwrite: T`. Any failure DELETEs the temp
best-effort. `copy_within` is one COPY with `Overwrite: T`, `Depth: infinity`, progress reported once with the source's
PROPFIND size.

## The reconnect model

`state.rs` keeps `Connected | Disconnected | NeedsCredentials` in an atomic; `emit_if_changed` reports transitions only,
and a retired volume reports nothing. `reconnect.rs`: `note_lost_session` acts once on the `Connected → Disconnected`
edge, drops the client, and, if "reconnect automatically" is on, runs a 2/5/15/30/60/120 s backoff loop that re-reads
the store and re-probes. A refusal latches `auth_attempt_spent`, moves to `NeedsCredentials`, and stops.
`attempt_reconnect` probes now; `reconnect_with_credentials(username, password)` requires the volume's own username
(another account is another volume: `NotSupported`), refreshes a REMEMBERED secret (never seeds one), and probes with
the typed password. `UnattendedReconnect` is `SwitchOff`, `NoStoredSecret`, or `Possible`. `sign_in_prompt` is always
`Password`.

## Connecting from the frontend

The app's commands (owned by `apps/desktop`):
`connectWebdavVolume({displayName, url, username, remoteRoot, autoReconnect}, attemptId)` with outcomes
`connected | authentication_rejected | needs_credentials | certificate_untrusted | not_a_webdav_server | timed_out | unreachable | cancelled`
(`AuthMethodUnsupported` surfaces as `authentication_rejected` until the frontend grows a word for it),
`cancelWebdavConnect`, `disconnectWebdavVolume`, `saveWebdavCredentials(url, username, secret)` / `hasWebdavCredentials`
/ `deleteWebdavCredentials`, `getKnownWebdavServers` / `updateKnownWebdavServer` / `forgetKnownWebdavServer`,
`getWebdavUnattendedReconnect(volumeId)`, and the backend-neutral `reconnectSmbVolume` /
`reconnectSmbVolumeWithCredentials` / `getVolumeSignInState`.

## Which side a test lives on

This crate: the parser, the path translation, the status table, the state machine (no server), and the Docker cells
against the fixture stack (`volume/integration_test.rs`, `volume/conformance_test.rs`, all `#[ignore]`d without it). The
app: anything whose other half is the transfer pipeline, the registry, or the listing cache, built on `volume::testing`.

## Not supported, and say so out loud

- Digest authentication (`AuthMethodUnsupported`). OAuth and app-password flows are plain passwords to this backend.
- Certificate pinning or a trust prompt: an untrusted certificate is a typed refusal and nothing more.
- WebDAV locks (LOCK/UNLOCK); a 423 is reported as busy.
- No watcher: `listing_watch_coverage` is `None`; `notify_mutation` is what keeps a pane honest.
- Quota (`get_space_info`) only where the server reports both RFC 4331 numbers non-negative.

## The public surface is capped

Root re-exports: 5 items (`WebdavConnectionParams`, `WebdavConnectError`, `WebdavVolume`, `UnattendedReconnect`,
`connect_webdav_volume`) plus `pub mod volume`, which the check counts as a sixth root promise. Public modules: 1
(`volume`), plus `volume::testing` under the `testing` feature. Pub items in `volume`: 4 (`WebdavVolume`,
`UnattendedReconnect`, `ConnectionState`, `connect_webdav_volume`); the check's own `countSurface` measures 8, since it
counts the methods on `WebdavVolume` too. `index-crate-isolation` is pinned at exactly 6 / 1 / 8 (measured 2026-09-01):
no slack, so the first widening is a conversation rather than a drift.
