# cmdr-webdav details

Must-knows and the module map: `CLAUDE.md`. This file carries the decisions.

## The connection model

HTTP holds no session. "Connected" means the last request that reached the wire came back; "disconnected" means one
failed with a transport error (`reqwest::Error::is_connect` / `is_request`, mapped to
`VolumeError::DeviceDisconnected`), or that the server went silent under a waiting request (§ "Silent or slow").
`connect_webdav_volume` reads the account's secret from the `CredentialStore` (service `scheme://host:port`, scope
`username`; nothing stored is `NeedsCredentials`), builds a `WebdavClient` (`user_agent("Cmdr")`, a 10 s connect timeout
and no `read_timeout`, redirects off, Basic auth on every request, plus a pool-free twin for the silence probe), and
proves it with one `PROPFIND Depth: 0` on the root. The probe rides `tokio::select!` against the cancel token; a cancel
leaves nothing behind. On success the backend records the PII-free analytics event `webdav_connected`.

**Decision: the client turns every response decoder off and the crate declares `http2` itself.** The app's reqwest has
`gzip` and `http2` unified in through `genai`, which `cmdr-webdav` built alone never saw. Decoders: a file manager
copies bytes, so a file served with `Content-Encoding: gzip` (a `.gz` handed out as-is, or a server compressing on the
fly) must copy as the bytes the server holds, at its `Content-Length`; with `gzip` on, reqwest decoded it and dropped
the length. The builder sets `no_gzip`, `no_brotli`, `no_deflate`, and `no_zstd`, and the test build turns every decoder
on through a dev-dependency so `transport_test.rs::an_encoded_file_reads_back_as_its_stored_bytes` runs against the
client the app ships. `http2`: so this crate's own tests negotiate what the app does; the gap hid a GCS-only HTTP/2
refusal from `cmdr-s3`'s live suite (`crates/cmdr-s3/DETAILS.md` § "Connecting"). `transport_test.rs` fails to compile
without it (2026-10-02).

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

The remote half is what `WebdavClient::url_for` percent-encodes (everything but unreserved characters) and appends a
trailing slash to for collections. ❗ `FileEntry.path` is the APP spelling, prefix and all, because a pane hands what a
listing gave it straight back — as does `display_path_for`, the listing-cache patcher's.

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
same: `apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Look-alike names". Unit cells:
`src/volume/paths_test.rs`; Docker cells:
`apps/desktop/src-tauri/src/file_system/write_operations/backend_suites/webdav_look_alike_test.rs`.

## Write staging

**Decision**: the transfer engine owns staging, and this backend doesn't stage a `CreateOrReplace` write again.
`write_from_stream(CreateOrReplace)` is ONE streaming PUT to the path it's handed, with `Content-Length: size`, progress
every 200 ms from a shared counter, and cancel by poisoning the body stream (the request aborts). Any failure DELETEs
that path best-effort, the same contract SFTP's truncating open keeps. **Why**: the engine stages every write here
(`write_is_single_shot` keeps its `false` default,
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "File writes are staged"), so the path
IS its `.cmdr-tmp-*` temp, and its landing MOVE (`rename(force = false)`, `Overwrite: F`) is what gives the bytes the
user's name and refuses a taken one. A second staging layer under that temp buys nothing and costs a second MOVE per
file plus a second temp in the user's folder (measured on the stock Apache fixture, 2026-09-23: 1 PUT + 2 MOVEs per new
file with it, 1 PUT + 1 MOVE without; pinned by
`apps/desktop/src-tauri/src/file_system/write_operations/backend_suites/webdav_wire_cost_test.rs`). ❌ **Rejected: a
capability saying "my write is already staged" so the engine skips its own.** The backend's temp would sit outside
everything the engine's staging carries: the in-flight ledger and its crash sweep, the pane hiding by ownership, the
`LandingName` refusal, the ` (recovered)` rescue, and the discard of an unplaced temp. One owner keeps one source of
truth.

**`CreateNew` still stages here**: a PUT to `<dest>.cmdr-tmp-<pid><nanos><n>`, then a MOVE onto `dest` with
`Overwrite: F`, which Apache `mod_dav` and sabre/dav both refuse with 412 over a taken name (`Attempted::TakingAName`
maps it to `AlreadyExists`; the conformance cell and
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
store one. The cross-backend contract is
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Copies keep the source's date".

- **Source**: the read stream reports the GET's `Last-Modified`, the same date a PROPFIND lists as `getlastmodified`
  (whole seconds), so it costs no round trip. Both Apache and Nextcloud send it on 200 and 206 alike.
- **Destination, best effort**: the PUT carries `X-OC-Mtime: <Unix seconds>` when the source has a date. ownCloud's
  extension, honored by Nextcloud (it answers `X-OC-MTime: accepted`), ownCloud, and rclone. WebDAV itself has no way to
  set a date: `getlastmodified` is a protected live property that no PROPPATCH may change. A server without the
  extension ignores the header and keeps its own date; that's a `debug!`, never a failure. The staging `MOVE` keeps what
  the PUT set (verified on `nextcloud:34.0.2-apache` by `nextcloud_a_copy_keeps_the_source_date`, 2026-10-07).
- ❗ **Apache `mod_dav` stores no date at all** (verified on httpd 2.4, by `conformance_test.rs`, 2026-10-07), so its
  cells split the contract: the source half runs on `seed.sh`'s `dated.txt` (the fixture's own `touch -d`, the only way
  to age a file there), and `apache_stores_no_date_so_a_copy_onto_it_carries_its_own` asserts the limit so it stays a
  fact. The destination half is pinned where it can hold, `nextcloud_test.rs`, so a PUT that stops sending the header
  fails a cell. That's a per-fixture expectation, ❌ not a capability flag: nothing in the app branches on whether a
  server keeps dates, and a flag would only move the same fact from a test into the trait.
- **Through the engine**, only the copy-off cell exists
  (`webdav_integration_a_copy_off_a_server_keeps_the_source_date`): the shared lane's servers are all Apache, and the
  Nextcloud lane runs this crate's cells only.

## The reconnect model

`state.rs` keeps `Connected | Disconnected | NeedsCredentials` in an atomic; `emit_if_changed` reports transitions only,
and a retired volume reports nothing. `reconnect.rs`: `note_lost_session` acts once on the `Connected → Disconnected`
edge, drops the client, and, if "reconnect automatically" is on, runs a 2/5/15/30/60/120 s backoff loop that re-reads
the store and re-probes. A refusal latches `auth_attempt_spent`, moves to `NeedsCredentials`, and stops.
`attempt_reconnect` probes now; `reconnect_with_credentials(username, password)` requires the volume's own username
(another account is another volume: `NotSupported`), refreshes a REMEMBERED secret (never seeds one), and probes with
the typed password. `UnattendedReconnect` is `SwitchOff`, `NoStoredSecret`, or `Possible`. `sign_in_prompt` is always
`SignInShape::Password`, so the sheet renders one field under a read-only username.

`note_lost_session` takes the dead client out and cancels its `lost` token, so every other operation still waiting on it
answers `DeviceDisconnected` at once rather than at its own budget; the pool goes with the client, so no request is ever
handed one of its connections again. ❗ **An installed client counts as live only while the state says `Connected`**:
the drop may still be on its way (a spawned task, when the lock was busy) when the frontend's reconnect fires on the
event, so `rebuild` treats a client installed under any other state as the dead one and dials. The other half: a fresh
client is installed and marked `Connected` under ONE write guard, and `drop_dead_client` takes only under a
non-`Connected` state, so the late task can never take the fresh one. The same pair as `crates/cmdr-sftp/DETAILS.md` §
"Coming back".

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
