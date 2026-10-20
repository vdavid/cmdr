# cmdr-webdav details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions.

## The connection model

HTTP holds no session that liveness depends on (a server's session cookie only saves work). "Connected" means the last
request that reached the wire came back; "disconnected" means one failed with a transport error
(`reqwest::Error::is_connect` / `is_request`, mapped to `VolumeError::DeviceDisconnected`), or that the server went
silent under a waiting request (§ "Silent or slow"). `connect_webdav_volume` reads the account's secret from the
`CredentialStore` (service `scheme://host:port`, scope `username`; nothing stored is `NeedsCredentials`), builds a
`WebdavClient` (`user_agent("Cmdr")`, a 10 s connect timeout and no `read_timeout`, redirects off, Basic auth on every
request, a cookie jar (§ "Session cookies"), plus a pool-free twin for the silence probe), and proves it with one
`PROPFIND Depth: 0` on the root. The probe rides `tokio::select!` against the cancel token; a cancel leaves nothing
behind. On success the backend records the PII-free analytics event `webdav_connected`.

**Decision: the client turns every response decoder off and the crate declares `http2` itself.** The app's reqwest has
`gzip` and `http2` unified in through `genai`, which `cmdr-webdav` built alone never saw. Decoders: a file manager
copies bytes, so a file served with `Content-Encoding: gzip` (a `.gz` handed out as-is, or a server compressing on the
fly) must copy as the bytes the server holds, at its `Content-Length`; with `gzip` on, reqwest decoded it and dropped
the length. The builder sets `no_gzip`, `no_brotli`, `no_deflate`, and `no_zstd`, and the test build turns every decoder
on through a dev-dependency so `transport_test.rs::an_encoded_file_reads_back_as_its_stored_bytes` runs against the
client the app ships. `http2`: so this crate's own tests negotiate what the app does; the gap hid a GCS-only HTTP/2
refusal from `cmdr-s3`'s live suite (`crates/cmdr-s3/DETAILS.md` § "Connecting"). `transport_test.rs` fails to compile
without it (2026-10-02).

### Session cookies

**Gotcha: Nextcloud verifies the Basic password on every request that brings no session cookie**, and that check is a
deliberately slow hash. Every authenticated DAV request cost ~145 ms on a quiet host (even `OPTIONS`; unauthenticated
`status.php` answers in 7 ms) and 300–750 ms on a busy one; sending back the cookies the first answer set brings it to
15–45 ms, PROPPATCH and the writes included (verified on the Nextcloud 34 fixture with curl, with and without a cookie
jar, 2026-10-13). The cost is CPU on the server, so parallel requests contend: the Nextcloud suite's cells took 8–22 s
and timed out at 30 s under load, with one cell's 35 requests spanning 11 s alone.

**Decision: the main client keeps a reqwest cookie jar (`cookie_store(true)`, the `cookies` feature, which the app's
manifest declares too) and still sends Basic on every request.** Basic stays so an expired or dropped session falls back
to the password in the same request rather than a 401 round trip. Nextcloud skips its CSRF check for a session that
signed in through DAV Basic auth, so the writes work on it. The jar lives in memory with the client and dies with it, so
a reconnect starts fresh; servers that set no cookie see no change. The pool-free silence twin keeps no jar: its
`OPTIONS` proves only that the server answers.
`transport_test.rs::the_next_request_brings_back_the_session_cookie_and_the_password` pins both halves.

**An instance is a name and a root over a shared client.** `WebdavVolume` is `{ name, root, inner }`, the same split
`crates/cmdr-sftp/DETAILS.md` § "The connection model" describes, and
`WebdavVolume::sharing_connection(name, remote_root)` builds another instance over the same client with no re-probe. It
is pure, it goes in through the registry's non-retiring replace, and it is deliberately not `Volume::rerooted`; that
section has why. `set_redial_root` moves the root the next re-probe asks for, once an edit is accepted: ❗ a reconnect
PROPFINDs the root, so a place whose old root was since deleted would otherwise never come back. `inner.params` sits
behind a `std::sync::RwLock` for it, read through the `params()` snapshot.

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

**A file where a folder should be can't be read off a status.** A MKCOL on a name a FILE holds, and one on a path under
that file, answer differently per server, and neither answer says "file":

- sabre/dav answers by the book: 405 on the file's own name, 409 under it (verified on Nextcloud 34.0.2, by `curl` and
  by the Docker cell, 2026-09-30). Those read as "already there" and "parent missing".
- Apache `mod_dav` answers **400 to everything addressed under a file**, a PROPFIND included, and to the file's own name
  when the URL carries the trailing slash a collection takes, which is how `mkcol` spells it (verified on httpd 2.4.68,
  same way, same date). That is an unclassified `IoError`.

So `create_directory_all` asks: `MakesDirectories::leads_to` is one `Depth: 0` PROPFIND, sent only after a MKCOL was
refused, and the shared walk turns a non-collection at or above the failed level into `NotADirectory(path)`. The walk
reads an unclassified answer to that PROPFIND as "look one level up", which is what gets past Apache's 400. Both servers
are held to it (`conformance_test.rs`, `nextcloud_test.rs`); the walk itself: `cmdr_fs::volume::mkdir_all`.

A `multistatus` entity (`&amp;`) reaches the parser as its own `Event::GeneralRef`, never inside a text node, so
`propfind.rs` resolves it there; a text-level `unescape` would silently drop the character.

TLS: `tokio-rustls` surfaces every handshake refusal as an `io::Error` of kind `InvalidData`, so `CertificateUntrusted`
covers any TLS refusal, of which a self-signed NAS is by far the commonest. Narrowing it to trust alone would need a
`rustls::Error` downcast, which means a direct `rustls` dependency; not taken yet.

## Path handling

`paths.rs` is `cmdr-sftp`'s translation with a different root, and both go through
`cmdr_fs::volume::remote_paths::RemoteRoot`: the app spells a file `webdav://ada@nas.local:443/Photos/a.jpg`, the server
spells it `/Photos/a.jpg`, and the volume is rooted at `<prefix><remote root>`. The prefix comes from
`cmdr_fs::volume::webdav_app_root`, a sibling of the `webdav_volume_id` the registry keys on. ❗ `webdav://` whatever
the transport is: `http` and `https` to one host and port are the same server, which is the call the volume id already
makes.

`remote_root` is normalized to `/` or `/Photos` under the base URL, `..` is resolved lexically BEFORE the containment
check, the prefix and the root are both matched by whole components, and anything outside is `NotFound` (never
anchored). ❗ A bare server-absolute path is refused too; `crates/cmdr-sftp/DETAILS.md` § "Path handling" has the
reasoning, which is the same here. The three root aliases stay, and `query.rs` uses one: `is_root` and
`get_space_info_impl` ask for `/`.

The remote half is what `WebdavClient::url_for` percent-encodes (`urls.rs`: everything but unreserved characters) and
appends a trailing slash to for collections. ❗ `FileEntry.path` is the APP spelling, prefix and all, because a pane
hands what a listing gave it straight back — as does `display_path_for`, the listing-cache patcher's.

Listings are one `PROPFIND Depth: 1` with a body naming
`resourcetype, getcontentlength, getlastmodified, creationdate, getetag, quota-available-bytes, quota-used-bytes`.
`propfind.rs` reads the `DAV:` namespace under any prefix, takes properties only from 2xx `propstat`s, percent-decodes
`href`s, and reduces absolute-URL hrefs to their path. The self entry is dropped by comparing decoded, slash-normalized
paths, never by position. With redirects off, a PROPFIND on a slash-less collection that answers 3xx (nginx and some NAS
firmware) is retried once WITH the slash.

## New names go out composed

Every path goes out byte for byte (`url_for` percent-encodes the bytes it's given), so an entry stored decomposed (NFD)
is read, deleted, and overwritten under its own spelling. Only a name Cmdr CREATES (an upload's free name, a new folder
or file, a rename's target, a ` (N)` pick, a new archive) goes out composed (NFC): `composes_new_names` answers `true`,
and the app's write layer respells the name through `Volume::spell_new_name`, ❌ never this crate. Downloads keep the
server's bytes.

Why: a WebDAV server on Linux stores the bytes the URL spelled, where web servers, PHP, and scripts match bytes, so a
decomposed `café.jpg` from macOS looks right and breaks every link to it. The decision and which other tools do the
same: `crates/cmdr-operations/DETAILS.md` § "Look-alike names". Unit cells: `src/volume/paths_test.rs`; Docker cells:
`apps/desktop/src-tauri/src/operations_host/backend_suites/webdav_look_alike_test.rs`.

## Write staging

**Decision**: the transfer engine owns staging, and this backend doesn't stage a `CreateOrReplace` write again.
`write_from_stream(CreateOrReplace)` is ONE streaming PUT to the path it's handed, with `Content-Length: size`, progress
every 200 ms from a shared counter, and cancel by poisoning the body stream (the request aborts). Any failure DELETEs
that path best-effort, the same contract SFTP's truncating open keeps (on a Nextcloud, a big file goes up in chunks
instead, still onto that one path: § "Chunked uploads (Nextcloud)"). **Why**: the engine stages every write here
(`write_is_single_shot` keeps its `false` default, `crates/cmdr-operations/src/transfer/DETAILS.md` § "File writes are
staged"), so the path IS its `.cmdr-tmp-*` temp, and its landing MOVE (`rename(force = false)`, `Overwrite: F`) is what
gives the bytes the user's name and refuses a taken one. A second staging layer under that temp buys nothing and costs a
second MOVE per file plus a second temp in the user's folder (measured on the stock Apache fixture, 2026-09-23: 1 PUT +
2 MOVEs per new file with it, 1 PUT + 1 MOVE without; pinned by
`apps/desktop/src-tauri/src/operations_host/backend_suites/webdav_wire_cost_test.rs`). ❌ **Rejected: a capability
saying "my write is already staged" so the engine skips its own.** The backend's temp would sit outside everything the
engine's staging carries: the in-flight ledger and its crash sweep, the pane hiding by ownership, the `LandingName`
refusal, the ` (recovered)` rescue, and the discard of an unplaced temp. One owner keeps one source of truth.

**`CreateNew` still stages here**: a PUT to `<dest>.cmdr-tmp-<uuid>` (`cmdr_fs::staging::temp_sibling_name`, shortened
to fit the server's name limit), then a MOVE onto `dest` with `Overwrite: F`, which Apache `mod_dav` and sabre/dav both
refuse with 412 over a taken name (`Attempted::TakingAName` maps it to `AlreadyExists`; the conformance cell and
`nextcloud_test.rs::the_write_path_lands_a_file_byte_exact_on_sabre_dav` pin each). A conditional PUT
(`If-None-Match: *`) straight to `dest` would refuse too, but after it failed for any other reason nothing says whether
what's at the name is our partial or another writer's file, so the cleanup couldn't be both safe and complete. The
engine never sends `CreateNew` here; it's the trait's contract for any other caller. `copy_within` is one COPY with
`Overwrite: T`, `Depth: infinity`, progress reported once with the source's PROPFIND size.

**A byte count that disagrees with `size` is a failed write**, in either direction, and whatever the PUT stored is
DELETEd. From the engine that path is its temp, so the landing never runs and the user's filename never sees it. All
three shapes are pinned by a cell in `volume/integration_test.rs`:

- **A source that ends EARLY** ends the request from our side (hyper's `NotEof`), which `reqwest` reports with the same
  predicate a dropped connection answers. ❌ Reading it as `DeviceDisconnected` would flip the whole volume offline over
  one stale `size`, so `ended` plus `fetched != size` turns it into an `IoError` naming both numbers.
  `a_source_that_ends_early_never_reaches_the_users_filename` fails with `Err(DeviceDisconnected(…))` if that arm goes.
  ❗ This arm has no alignment escape and cannot grow one: hyper's poll gate closes only once `Content-Length` is
  SATISFIED, and a short source never satisfies it, so the body is polled until the source answers `None`.
- **A source that yields MORE** is the sharper one: hyper TRUNCATES the body at `Content-Length` (verified on hyper
  1.10.1, in its HTTP/1 encoder's `Kind::Length` arm, 2026-09-01), the server stores the prefix and answers 201, and
  nothing on the wire is wrong. Only the count this side kept says the file is short.
  `a_source_that_overruns_its_promise_never_reaches_the_users_filename` fails with `Ok(150000)` if that arm goes: a
  truncated file the engine would then land at the user's name, reported as a success.
- **A source that yields more on a PIECE BOUNDARY** is the same fault where a naive count cannot see it, and it is why
  the body reads ahead. hyper stops POLLING the moment the promise is met, so a source whose pieces divide `size`
  exactly is never asked again and every count agrees with the promise. Measured before the read-ahead existed: 200,000
  bytes in 50,000-byte pieces against a promised 150,000 answered `Ok(150000)`, and the prefix went to the user's
  filename (`webdav-fixture-apache`, 2026-09-02).
  `a_source_that_overruns_on_a_piece_boundary_never_reaches_the_users_filename` is the cell.

### The read-ahead, and its two counters

The body's `unfold` pulls the NEXT piece out of the source before handing the current one over. That one line is the
over-long guard: whether or not hyper polls again, "the source still had bytes" is already on record when the PUT
returns. It costs one extra piece held in memory (the piece was going to be allocated anyway) and buys back the case
above.

It also splits one number into two, and ❗ they are not interchangeable:

- **`fetched`** — bytes pulled OUT of the source. The guard's number, and what `write_from_stream` returns. Only this
  one can count a piece hyper never asked for, which is the entire point.
- **`handed`** — bytes given TO the body. What progress reports, clamped to `size`. This is the same quantity the bar
  has always shown, so the read-ahead changes nothing a user sees. ❌ Reporting `fetched` would run the bar one piece
  ahead of the wire and could show 100% before the last bytes are sent, which invites a cancel at the worst moment; the
  clamp is there so an over-long source on its way to being refused cannot push it past 100% either.

On every success the two are equal and both equal `size`.

**Cancellation.** The `unfold` answers a cancelled token before it pulls or hands over anything, so a cancelled upload
puts no further byte on the wire; the piece read ahead is dropped with the stream state, having never been sent, and
whatever the PUT stored is DELETEd on the way out. `a_cancelled_upload_leaves_neither_the_destination_nor_a_temp` holds
that here, and the app's `webdav_integration_a_cancelled_upload_leaves_nothing_behind` holds it through the whole
transfer pipeline. ❗ The crate cell's source is deliberately slow (16 pieces, 40 ms each): cancellation reaches this
backend through the 200 ms progress tick, so a body that outruns one tick can only ever be cancelled before its first
byte, which is not the case worth guarding.

## Chunked uploads (Nextcloud)

**Why**: a reverse proxy in front of a Nextcloud commonly caps one request's body (Cloudflare at 100 MB on its Free and
Pro plans, an nginx at its `client_max_body_size`), so a big file sent as ONE `PUT` fails after the user waited through
most of it (issue #174). Nextcloud's answer is its chunking API (developer manual, "Chunked file upload"), and this
backend uses it for every file bigger than one chunk:

1. `MKCOL <install>/remote.php/dav/uploads/<user id>/cmdr-<uuid>/`.
2. `PUT` chunks `00001`, `00002`, … into it, each one of `writes.rs`'s streaming `PUT`s (`stream_put`) over a window of
   the one source stream (`chunked_upload.rs`'s `SharedSource`), so the size guard, the read-ahead, progress, and cancel
   hold per chunk exactly as for a whole file.
3. `MOVE …/cmdr-<uuid>/.file` onto the target, which assembles the chunks; the server then drops the collection itself.

- **v2, with v1 for free.** All three carry `Destination` (the target, as the `dav/files` endpoint spells it), which v2
  requires, and the chunks carry `OC-Total-Length` so v2 checks the quota on the first chunk (507) rather than at the
  `MOVE`. v2 runs only where the server has a distributed cache (Redis or Memcached); without one its plugin steps aside
  and the same requests assemble the v1 way (read in `apps/dav/lib/Upload/ChunkingV2Plugin.php`, Nextcloud 35.0.0,
  2026-10-14). The fixture has none, so its cell runs v1; the request shape is v2's.
- **Detection: `X-User-Id` on the connect probe's 207**, plus a base URL on one of Nextcloud's account endpoints
  (`remote.php/webdav/…` or `remote.php/dav/files/<user>/…`). Nextcloud's `UserIdHeaderPlugin` sets it on every
  authenticated DAV answer (verified on `nextcloud:35.0.0-apache` by `curl`, both endpoints, 2026-10-14); it's also the
  only reliable source of the USER ID the uploads home is filed under, which can differ from the login name (an email
  login, LDAP). Zero extra requests. ❌ Not `d:current-user-principal`: the legacy `remote.php/webdav/` answers it 404.
  No `OC-*` header appears on a plain DAV answer, and `status.php` would be a request per connect. A public share, the
  DAV root, and every non-Nextcloud server upload in one piece. The answer lives on the `WebdavClient`, so each
  reconnect asks again.
- ❗ **The `Destination` must be on `remote.php/dav`.** sabre/dav resolves it against its own base, so a target on the
  legacy `remote.php/webdav/` is re-spelled as `remote.php/dav/files/<user id>/…` (`chunking.rs`).
- **Chunk size and threshold are one number, 64 MiB** (`CHUNK_SIZE`): under Cloudflare's cap, far above v2's 5 MB floor
  for every chunk but the last, and a file that fits in one chunk keeps the single `PUT`. A file needing more than v2's
  10,000 chunks (625 GiB) gets longer ones. Names are zero-padded to five digits so v1's name order is the number order.
- **Dates ride the assembling `MOVE`** (`X-OC-Mtime`, `X-OC-CTime`), which answers `accepted` for each like a `PUT` does
  (same server, by `curl` and the cell).
- **The target sees nothing until the `MOVE`.** Any failure before it (a refused chunk, a cancel, a source of the wrong
  length) DELETEs the collection, which also cancels a v2 backend write, and leaves the target alone; a failed `MOVE`
  removes the target too. The server expires an abandoned collection after a day anyway. A source that runs dry is
  reported as the whole file's short count; one with bytes left after the last chunk is refused before the `MOVE`.
- **The `MOVE` budget grows with the file**: assembly copies every chunk, so past `MUTATION_BUDGET` it allows 10 MiB/s.
- **Pause** parks in the source stream as it does for a single `PUT` (mid-chunk). A pause over a day loses the upload:
  the server expires it, the next chunk fails, and the cleanup runs.
- Pinned by `chunking_test.rs` (detection, URLs, the plan), `src/volume/chunked_upload_test.rs` (the windows), and
  `nextcloud_test.rs::a_big_file_goes_up_in_chunks_and_a_cancelled_one_leaves_nothing` (byte-exact, dates, progress,
  collections gone after success and after a cancel; it shrinks chunks to 5 MiB). Nextcloud refuses to LIST the uploads
  home, so the cell asks for each collection by the name `chunked_upload::STARTED` recorded.

## What a real server answers

Three things this backend's shape rests on are the SERVER's behaviour, not ours, and a stock Apache `mod_dav` can settle
none of them: it honours `Range` natively and omits the quota properties entirely. `volume/nextcloud_test.rs` pins each
one against `webdav-fixture-nextcloud`, and `desktop-rust-webdav-nextcloud` is the lane that runs it. Read the numbers
here as one server's answer, evidence-anchored, rather than as the protocol's.

- **A ranged GET comes back 206 with exactly the window asked for** (verified on `nextcloud:34.0.2-apache`, sabre/dav
  behind Apache 2.4.68 + PHP 8.5.9, by the Docker cell, 2026-09-02). The 200 path in `streams.rs` stays anyway: RFC 9110
  § 14.2 makes ranges optional, no other real server has been watched, and the fallback costs a whole file's bandwidth
  only when it fires. What exercises it is a fixture built to: `webdav-fixture-norange` serves the same export under
  Apache's `MaxRanges none`, which answers every ranged GET 200 with `Accept-Ranges: none` and the whole file (verified
  on httpd 2.4.68 by `curl` and by the two cells, 2026-09-02). Both halves of the skip are pinned there — the resumed
  stream and `read_range` — and each cell asks the server what it answered before asserting on the backend, so a fixture
  that started honouring `Range` fails rather than quietly re-testing the 206 path.
- ❗ **A GET's length is the server's or the caller's, ❌ never a guessed `0`** (`streams.rs`). An answer with no
  `Content-Length` (a proxy that stripped it, a chunked body) declares the caller's hint (`open_read_stream_with_hint`),
  else `Unknown`; a body that ends before the length the server named (a 206's `Content-Range`) fails with an `IoError`
  instead of ending cleanly. Before this, a lengthless `200` declared exactly zero bytes, so a WebDAV destination always
  refused it and an MTP one was told to expect an empty object (#538). Pinned by `get_length_test.rs` against
  `src/volume/fake_dav.rs`, the in-process server `slow_server_test.rs` shares.
- **A PUT with no `Content-Length` is accepted, 201 Created, body byte-exact** (same server and date; 256 KiB by the
  Docker cell, 8 MiB by hand with `curl`). ❌ So "sabre/dav answers 411 to a chunked PUT" is NOT true of Nextcloud on
  Apache with `mod_php`, which is what the official image runs. `writes.rs` sends the length regardless, and the reason
  is simply that it always knows the size: a body of unknown length buys nothing here and gives every proxy and every
  server configuration in the world a chance to disagree. The cell is what would tell us if a version answered 411.
- **RFC 4331 quota reports the ACCOUNT's numbers.** A 5 GiB account answers `quota-available-bytes` + `quota-used-bytes`
  adding up to exactly 5,368,709,120, nothing like the container's disk (same server and date). `get_space_info` reads
  that pair as `SpaceInfo::Bounded`, and ❗ the used figure there is `quota-used-bytes` and ❌ never `oc:size`, so the
  three numbers stay one self-consistent set.
- **An account with NO quota answers `quota-used-bytes: 0`, however much it holds.** This is the trap, and it's the
  common case: unlimited is what a stock Nextcloud user gets. Alongside it comes `quota-available-bytes: -3`, the
  `SPACE_UNLIMITED` sentinel (`-1` and `-2` are the others). Measured on the fixture's own Nextcloud (2026-09-03): the
  two accounts hold the identical ~65 MB skeleton, and the quota'd one reports `quota-used-bytes: 64934262` while the
  unlimited one reports `0`. It isn't a stale filecache — `occ files:scan` finds 0 new and 0 updated and the figure
  stays 0. ❌ So "the used figure is real whenever the ceiling isn't" is FALSE, and trusting it renders "0 bytes used"
  under the pane of an account full of files.
- **`oc:size` is the way out, and the reason a non-standard property is in `PROPFIND_BODY`.** ownCloud's namespace
  (`http://owncloud.org/ns`), inherited by Nextcloud, carries the bytes a collection holds counting everything under it,
  and on the account root that IS the used figure: `64934262` on both fixture accounts, matching `quota-used-bytes`
  exactly wherever that one works. `get_space_info` prefers it in the unbounded branch and falls back to
  `quota-used-bytes`, so Apache `mod_dav` and generic sabre/dav are untouched (they send neither, and land on
  `NotSupported`). The parser resolves it by NAMESPACE, never by the `oc:` prefix a document happens to use, since a
  server may bind ownCloud's namespace to any prefix and `oc` to anything else; `propfind_test.rs` pins that with a
  decoy. ❗ The bar for the next vendor property: the standard has to be unable to answer the question at all, the
  fallback has to leave every other server exactly as it was, and a fixture cell has to hold the claim up.
- ❗ The `occ` quota is applied at process start, so a quota changed on a live server stays invisible over WebDAV until
  the server restarts; the fixture provisions in a post-installation hook for exactly that reason.

What the pane does with each shape: `cmdr-fs`'s `SpaceInfo`, and `apps/desktop/src/lib/file-explorer/DETAILS.md`.

## Dates

Copies off this backend always keep the source's modification date; copies onto it keep it only where the SERVER can
store one. The cross-backend contract is `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "Copies keep the
source's metadata".

- **Source**: the read stream reports the GET's `Last-Modified`, the same date a PROPFIND lists as `getlastmodified`
  (whole seconds), so it costs no round trip. Both Apache and Nextcloud send it on 200 and 206 alike.
- **Destination, best effort**: the PUT carries `X-OC-Mtime: <Unix seconds>` when the source has a date. ownCloud's
  extension, honored by Nextcloud (it answers `X-OC-MTime: accepted`), ownCloud, and rclone. WebDAV itself has no way to
  set a date: `getlastmodified` is a protected live property that no PROPPATCH may change. A server without the
  extension ignores the header and keeps its own date; that's a `debug!`, never a failure. The staging `MOVE` keeps what
  the PUT set (verified on `nextcloud:35.0.0-apache` by `nextcloud_a_copy_keeps_the_source_date`, 2026-10-12).
- ❗ **Apache `mod_dav` stores no date at all** (verified on httpd 2.4, by `conformance_test.rs`, 2026-10-07), so its
  cells split the contract: the source half runs on `seed.sh`'s `dated.txt` (the fixture's own `touch -d`, the only way
  to age a file there), and `apache_stores_no_date_so_a_copy_onto_it_carries_its_own` asserts the limit so it stays a
  fact. The destination half is pinned where it can hold, `nextcloud_test.rs`, so a PUT that stops sending the header
  fails a cell. For the mtime that's a per-fixture expectation, ❌ not a capability flag: mtime isn't a loss-note class,
  so nothing in the app branches on it. Creation and folder dates ARE a capability (§ "Creation dates and folder
  dates"), because the loss note reads it.
- **Through the engine**, only the copy-off cell exists
  (`webdav_integration_a_copy_off_a_server_keeps_the_source_date`): the shared lane's servers are all Apache, and the
  Nextcloud lane runs this crate's cells only.

### Creation dates and folder dates (Nextcloud)

WebDAV has no standard way to set either; Nextcloud has its own, documented in its WebDAV client API ("Basic File &
Folder Operations": the `X-OC-CTime` request header and the `nc:creation_time` property). Verified on
`nextcloud:34.0.2-apache` (`webdav-fixture-nextcloud`, by `curl` and the cells below, 2026-10-12):

- **A file's creation date rides the PUT** as `X-OC-CTime: <Unix seconds>` beside `X-OC-Mtime`; the server answers
  `X-OC-CTime: accepted`, lists it as `creationdate` and `nc:creation_time`, and the staging `MOVE` keeps it. Zero new
  requests. A server that took the probe's word and then didn't take the header is a `warn!` (the loss note can't hear a
  refusal inside `write_from_stream`).
- **A folder's creation and modification dates take ONE PROPPATCH** (`d:lastmodified` and `nc:creation_time`, whole
  seconds) in `apply_metadata`, after its contents land: one request per created folder, like SFTP's and SMB's
  `SETSTAT`. A 207 whose every `propstat` is 2xx is `Kept`; anything else `Refused`.
- ❗ **Which server keeps them is the connect probe's answer**, per volume: the probe alone asks for `nc:creation_time`
  (`transport.rs`'s `PROBE_BODY`), and a 2xx `propstat` holding it sets `keeps_dates`, re-read on every reconnect.
  `metadata_caps()` then answers `WebdavVolume::DATE_KEEPING_CAPS` (stores `CREATED`, folders keep `CREATED` and their
  modification date), else `MetadataCaps::NONE`. ❌ Not every PROPFIND: on a server without it every entry grows a 404
  `propstat`, real bytes on a big listing. ❌ Never PROPPATCH dates on `mod_dav`: it answers 207 and stores them as dead
  properties, changing nothing, so the copy would report dates kept that weren't. Pinned by
  `conformance_test.rs::apache_keeps_no_creation_dates_and_says_so`.
- **Copies off the server report no creation date**: a GET carries none, and a PROPFIND per file would be a request per
  file. A folder's comes off the listing, like every backend's.
- ❗ **Nextcloud answers `creationdate` 1970-01-01 for an item with no creation time** (anything not uploaded with
  `X-OC-CTime`, every folder it made itself). The parser reads that as none: as a date, a copy off the server would
  stamp 1970 on the user's folder (`FolderMetadata` carries a listed creation date onto every created folder).
- **Pinned by**
  `nextcloud_test.rs::{nextcloud_keeps_a_files_creation_date, nextcloud_keeps_a_folders_dates, a_folder_with_no_creation_time_lists_none}`
  and `propfind_test.rs` (the epoch, the namespace, the PROPPATCH answer).

## The reconnect model

The lifecycle is `cmdr_fs::volume::http_session`, shared with `cmdr-s3`: `crates/cmdr-fs/DETAILS.md` § "The HTTP
backends' connection lifecycle" has the mechanism and its reasoning (the drop on the first `DeviceDisconnected`, the
backoff, the one unattended try, and why a failing request loses only the client it went out on). `reconnect.rs` brings
what is this backend's own: the PROPFIND probe, and its refusals mapped (`AuthenticationRejected` and
`AuthMethodUnsupported` spend the unattended try; `NeedsCredentials` asks a person without spending it).
`attempt_reconnect` probes now; `reconnect_with_credentials(SignIn)` requires the volume's own username (another account
is another volume: `NotSupported`), then signs in through the shared lifecycle, which files the secret only once it
connected (#530). `UnattendedReconnect` is `SwitchOff`, `NoStoredSecret`, or `Possible`. `sign_in_prompt` is always
`SignInShape::Password`, so the sheet renders one field under a read-only username.

## Bounded bodies

A hostile or broken server can stream an answer forever, so every buffered body reads through `WebdavClient::read_body`
with a cap, ❌ never `.text()` / `.bytes()`. An announced `Content-Length` past the cap is refused before a byte is
read; a streamed overrun stops at the cap. `MAX_LISTING_BODY` (128 MiB, about 200,000 children at 500–700 bytes each)
bounds a PROPFIND; `MAX_PROBE_BODY` (1 MiB) bounds the connect probe's `Depth: 0`. A listing past its cap is a typed
`PropfindOutcome::TooLarge`, which `volume/query.rs` answers as an `IoError`. Streaming reads and writes (GET, PUT) are
never buffered, so they carry no cap.

What fits under the cap still reaches `propfind.rs`, so the parser is fuzzed: the `fuzzing` feature exposes
`fuzzing::propfind`, which the `webdav_propfind` target drives (`fuzz/DETAILS.md`).

## Silent or slow

A server that goes SILENT (a NAS asleep, Wi-Fi gone, a VPN dropped) closes nothing, and HTTP has no keepalive, so a
request on one waits for its budget and ends with `ConnectionTimeout`, which says nothing about the server. ❌ A timeout
can't be the signal: a huge listing on a slow NAS times out just the same, and reading one as a lost connection would
flicker `Disconnected` on a server that's merely busy. So `cmdr_fs::volume::liveness` (shared with `cmdr-s3`) watches
for silence instead:

- **What counts as hearing from the server**: a response's headers (`WebdavClient::send`, which every request goes out
  through), each body chunk (PROPFIND bodies are read chunk by chunk for this; `WebdavReadStream` notes its own), and
  each upload piece hyper takes. The last is sound because hyper asks for the next piece only as the socket drains, and
  a socket drains only while the far end acknowledges; without it, a server that says nothing until the whole upload is
  in would look silent for its entire length.
- **The ladder** (`Timings::PRODUCTION`): with an operation waiting (`noting` holds a `Waiting`) and 10 s of quiet, the
  watch sends an `OPTIONS` on the base URL through a pool-free client, so it always dials FRESH: a pooled probe could
  ride the very connection that went quiet. ANY answer (a 401, a 405) means busy, and the wait goes on. Two unanswered
  in a row, 10 s budget each, means gone: 30 s in all, the silence `cmdr-smb` and `cmdr-sftp` allow. A byte on any
  request while a probe is out counts as its answer.
- **Gone** cancels the client's `lost` token. `noting` races every operation against it and answers
  `DeviceDisconnected`, which takes the refused path from there: one `Disconnected`, the backoff if the switch is on,
  nothing if it's off.
- **Cost**: nothing while idle (the clock starts when the first operation starts waiting, and the watch ends with the
  last), no probe while bytes flow, and one small request per 10 s of silence otherwise.

What stays deliberately out of reach: a server alive enough to answer a fresh `OPTIONS` while one request's connection
has died is slow, not gone, to this watch, and that request waits for its own budget. The OS covers the idle half of
that: `reqwest` keeps TCP keepalive on pooled connections (15 s idle, 3 probes 15 s apart; verified on reqwest 0.13.4,
`ClientBuilder::new`, 2026-09-23), so a dead idle connection is reaped rather than handed to the next request. A
single-threaded server too busy to answer anything for 30 s would read as gone; no NAS or Nextcloud setup works that
way.

Pinned three ways: `crates/cmdr-fs/src/volume/liveness_test.rs` runs the ladder on a paused clock with a closure for a
probe (exact deadlines, no server); `volume/slow_server_test.rs` runs it for real on a shortened ladder against an
in-process server that holds a listing, trickles a body, or goes quiet on command; `volume/connection_drop_test.rs` runs
the production ladder against Apache behind a black-holed `TcpProxy`.

## Connecting from the frontend

The sign-in UI's command surface is protocol-agnostic: `connectServer` (a brand-new target) and `connectSavedPlace` (one
already saved) in `apps/desktop/src-tauri/src/commands/servers.rs`, documented end to end in
`apps/desktop/src/lib/servers/DETAILS.md`. Both funnel into `network::webdav_volume_wiring::connect_and_register`, which
answers with outcomes (`ServerConnectOutcome`, widened across protocols)
`connected | authentication_rejected | needs_credentials | auth_method_unsupported | certificate_untrusted | not_a_webdav_server | timed_out | unreachable | cancelled`
(`AuthMethodUnsupported` is its own variant, ❌ never folded into `AuthenticationRejected`: a Digest-only server never
saw the password). The rest of the app's commands: `cancelServerConnect`, `disconnectPlace`,
`saveWebdavCredentials(url, username, secret)` / `hasServerSecret` / `forgetServerSecret`, `getKnownWebdavServers` /
`forgetServer` (the unsuffixed ones are the protocol-agnostic `servers.ts`). Editing a saved server without connecting
goes through the protocol-agnostic `updateSavedServer` (`commands/servers.rs`), which calls
`webdav_volume_wiring::save_without_connecting` directly. ❗ Neither that edit nor a connect can change `pinned`:
`webdav_known_servers::remember` honors it only for a NEW entry and carries the stored value across on a replace, so a
reconnect can't undo an unpin; it defaults to FALSE, and `getKnownWebdavServers` in `tauri-commands/webdav.ts` is the
one place that default is spelled. `getWebdavUnattendedReconnect(volumeId)`, and the backend-neutral
`reconnectSmbVolume` / `reconnectSmbVolumeWithCredentials` / `getVolumeSignInState`.

### `https` first, for a typed `http` address

Basic auth rides every request, so over `http` the password crosses the network readable (#540; David's call,
2026-10-19: warn, and try `https` first). `connectServer` (a typed address) dials through
`WebdavConnectionParams::dial_preferring_https` (`https_first.rs`): when the address is `http`, an unauthenticated
`OPTIONS` to its `https_twin` (same host and path, an explicit port kept, the default one moved to 443) decides. An
answer carrying a `DAV` header (any status) over a TLS handshake this Mac trusts moves the dial, so the server is saved,
keyed, and identified by the `https` URL from the start. A plain-`http` port, an untrusted certificate (the self-signed
NAS), a refusal, 4 s of silence, and an answer without `DAV` all fall back to the typed address, and the sheet's warning
stays up for it (`apps/desktop/src/lib/servers/DETAILS.md`).

- ❗ **Only a `DAV` answer counts** (#540): a 443 serving another vhost, or a 301 back to `http`, answers `OPTIONS` too.
  A server that wants auth before it says `DAV` reads as "doesn't answer", which errs toward the typed address.
- ❗ **A twin whose DIAL fails falls back to the typed address**, whatever the reason but Cancel: the probe proves
  something speaks WebDAV there, ❌ not that it's the server typed (another root, another account). The caller hands in
  one dial closure that re-offers the secret under each address's own key.

- ❌ **Never `https`-first for a SAVED place** (`connect_and_register_since`): the saved URL is its volume id and its
  secret's key, so a silent move would orphan both.
- ❗ **The probe runs under the attempt's cancel token**, with both candidate places registered, so Cancel and a server
  move reach it like the dial.
- ❗ **No credentials on the probe.** It runs before the password has anywhere safe to go.

## A folder's DELETE

`DELETE` on a collection is recursive (RFC 4918 § 9.6.1), and so is a forced `MOVE` onto one (§ 9.9.3, the destination
is deleted first). The trait's verbs are one node (`Volume::delete`) and never a held folder (`Volume::rename`), so both
refuse with `ENOTEMPTY` when a `Depth: 1` PROPFIND counts anything under the target (`mutation.rs::refuse_if_held`).

That count alone leaves a window: a child landing between the PROPFIND and the recursive verb (one round trip, 20–150 ms
on Nextcloud) would go with the folder, and the call would report success. So a folder the count found empty is moved
aside first (`delete_aside.rs`), for a delete (the same-volume move's source sweep included, via `delete_directory`) and
for a forced rename onto it (`replace_collection`):

1. `MOVE` it to `<name>.cmdr-deleting-<uuid>` beside it (`Overwrite: F`). Nobody else knows that name, so nothing new
   can land in it; a writer aiming at the old path gets a 409 instead of losing a file.
2. Count it there (`Depth: 1`). Empty: `DELETE` it, which can no longer race. A 404 there counts as done. For a forced
   rename, the source first `MOVE`s onto the now-free name with `Overwrite: F` (never `T`, so nothing it replaces is
   unconfirmed), then the empty aside is deleted; a failure there leaves only what the next listing tidies.
3. Held, or the `DELETE` (or the rename's `MOVE` in) refused: `MOVE` it back (`Overwrite: F`) and answer `ENOTEMPTY` (or
   the refusal). When the original name was taken meanwhile, it goes to `<name> (kept by Cmdr)` (then `… 2`, up to 20)
   and the error says where; when even that fails, the error names the aside. Nothing is lost in any branch.
4. A server that refuses the first `MOVE` (403, 405, 501) gets the plain `DELETE` (or the plain `Overwrite: T` rename),
   with the window above still open there. A Nextcloud share's root is the case in mind; its `MOVE` renames the mount
   point, so it likely takes the main path (not verified on a share).

Why it holds up:

- **It runs on a task of its own.** Once the folder is aside, only finishing puts it right, so a caller that stops
  waiting (the silence watch's cut, a dropped future) doesn't strand it. Each request keeps its `MUTATION_BUDGET`.
- **The aside hides by ownership** (`StagingTemp::adopt`, `cmdr_fs::staging`), so a pane doesn't flash it while the
  delete holds it, and a leftover nobody holds shows under a name that still says which folder it was.
- **A crash between the steps leaves an EMPTY aside at worst**, and the next listing of its parent deletes it
  (`tidied_leftover`). Only an empty one with the strict name (`is_deleting_aside_name`), never one in flight here: a
  held leftover is a folder whose move back didn't happen. Another device's Cmdr mid-delete can lose that race to the
  tidy; its delete then reports the 404 of a folder that's gone, which is what it was doing anyway.
- **Files are still deleted one by one under their real names first**, so Nextcloud's trash only ever receives the empty
  aside, as `<name>.cmdr-deleting-<uuid>` (verified on Nextcloud 35.0.0 `webdav-fixture-nextcloud`, trashbin PROPFIND,
  2026-10-20).

Cells: `volume/delete_aside_test.rs` scripts the race on the fake server (`FakeDav::after`); its
`assert_delete_moves_aside_on_a_real_server` runs on Apache (`integration_test.rs`) and Nextcloud (`nextcloud_test.rs`).

Why not a protocol guard: neither closes the window on the fixture servers.

- **An `If` ETag precondition**: `mod_dav` gives a collection no `getetag` at all, and sabre/dav (Nextcloud) answers 412
  to ANY etag condition on a collection, its current one included, though the etag itself does change when a child lands
  (sabre validates etags on files only). So `If: (["<etag>"])` would fail every folder delete on Nextcloud.
- **A depth-infinity `LOCK`**: `mod_dav` enforces it (a foreign `PUT` into the folder is refused), but Nextcloud grants
  the lock and still takes the foreign `PUT` (201), then deletes it with the folder.

(verified on httpd 2.4.69 `webdav-fixture-apache` and Nextcloud 35.0.0 `webdav-fixture-nextcloud`, by hand with `curl`,
2026-10-20.)

A forced rename onto a FILE keeps the plain `Overwrite: T`: a file isn't recursive, so there's no window to close.

## Which side a test lives on

This crate: the parser, the path translation, the status table, the state machine and the silence ladder (no server),
the slow-server cells (`volume/slow_server_test.rs`, an in-process server, no Docker), and the Docker cells against the
fixture stack (`volume/integration_test.rs`, `volume/conformance_test.rs`, the "reconnect automatically" cells in
`volume/reconnect_test.rs`, and the real-drop cells in `volume/connection_drop_test.rs`, all `#[ignore]`d without it).
The drop cells cut the TCP connection in a `cmdr_fs::testing::tcp_proxy::TcpProxy` they own, refused and silent; ❌
never pause or stop a container for that, since the stack is shared by lease. The conformance cells answer with Apache's
own verbs, which is the point: `MOVE` overwrites by default, `DELETE` on a collection is recursive, and a `PROPFIND` of
a collection nobody has created yet is a 404 that the conflict scan owes an empty list for. The app: anything whose
other half is the transfer pipeline, the registry, or the listing cache, built on `volume::testing`.

The two size-mismatch cells sit here rather than in the app's transfer suite for a reason worth keeping: reaching that
guard through the real pipeline needs a local source that disagrees with its own stat, which only happens by racing a
truncation. A cell handing `write_from_stream` a `size` the source was never going to match asks the same question with
nothing to race.

`volume/nextcloud_test.rs` is the exception in two ways, both deliberate. Its server is heavy enough to stay out of the
shared fixture lane, so `desktop-rust-webdav-nextcloud` selects it by MODULE PATH (`test(volume::nextcloud_test::)`) and
the shared lane subtracts the same atom — renaming the module takes the cells out of both, and `WebdavNextcloudTestAtom`
is the one place to change with it. And two of its cells build a `reqwest` request directly rather than going through a
`Volume` method, because what they ask is what the server does with a request this backend never sends.

Every connection in `volume::testing` resolves through `fixture_target`, so `CMDR_WEBDAV_TEST_URL` (plus `_USERNAME`,
`_PASSWORD`, and an optional `_ROOT`) points the whole suite at a server of your own with no code change, under the
`own-server` nextest profile. All 26 selected cells pass that way against the fixture's own Nextcloud (2026-09-02),
conformance cells included. The cells that can only be honest against the seeded fixture say so and return; which ones,
and what the write cells do to a real account, is in `apps/desktop/test/webdav-servers/README.md`.

## Diagnostic privacy

WebDAV logs debug-escape operation paths and carry the HTTP status (`code=`) or typed transport kind. A transport
failure's `reqwest` prose goes whole into `detail=`; reports redact and cap it:
`apps/desktop/src-tauri/src/redact/DETAILS.md` § "External-text fields". The original error remains available to
classification and the typed `VolumeError` path.

## Not supported, and say so out loud

- Digest authentication (`AuthMethodUnsupported`). OAuth and app-password flows are plain passwords to this backend.
- Certificate pinning or a trust prompt: an untrusted certificate is a typed refusal and nothing more.
- WebDAV locks (LOCK/UNLOCK); a 423 is reported as busy. **Decision: deliberately not. Why:** locks guard against
  concurrent EDITORS, which Cmdr isn't: it copies, moves, and renames whole files, and the staged PUT+MOVE already keeps
  a partial off the user's filename. A lock adds server-side state that outlives a crash (a dead lock a person has to
  clear) for no operation Cmdr performs. Revisit only if a server refuses unlocked writes in practice.
- No watcher: `listing_watch_coverage` is `None`; `notify_mutation` is what keeps a pane honest.
- Quota (`get_space_info`) where the server reports a usable `quota-used-bytes`: with a non-negative
  `quota-available-bytes` beside it that's a bounded account, without one it's an unbounded reading (§ "What a real
  server answers"). Apache `mod_dav` sends neither property, so it gets `NotSupported`.

## The public surface is capped

Root re-exports: 5 items (`WebdavConnectionParams`, `WebdavConnectError`, `WebdavVolume`, `UnattendedReconnect`,
`connect_webdav_volume`) plus `pub mod volume`, which the check counts as a sixth root promise. Public modules: 1
(`volume`), plus `volume::testing` under the `testing` feature. Pub items in `volume`: 4 (`WebdavVolume`,
`UnattendedReconnect`, `ConnectionState`, `connect_webdav_volume`); the check's own `countSurface` measures 10, since it
counts the methods on `WebdavVolume` too. Two of those are editing a connected place (§ "The connection model"):
`sharing_connection` and `set_redial_root`, which fit none of the four dispositions for the reasons
`crates/cmdr-sftp/DETAILS.md` § "The public surface is capped" gives for their twins. `index-crate-isolation` is pinned
at exactly 6 / 1 / 10 (measured 2026-09-11, `pnpm check index-crate-isolation`): no slack, so the next widening is a
conversation rather than a drift.
