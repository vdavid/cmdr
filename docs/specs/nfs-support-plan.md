# NFS support: direct NFSv3 connections

Tracks [#304](https://github.com/getcmdr/cmdr/issues/304). Why: a stock Linux NAS export (QNAP, Synology, TrueNAS) is
`secure`, so macOS mounts it only as root, and Finder and `mount_nfs` both fail for a normal user. Cmdr's own Connect to
Server refuses `nfs://` outright. This adds `crates/cmdr-nfs`, our own user-space NFSv3 client, on the rails SFTP,
WebDAV, and S3 laid down (saved place → pin, the reconnect cycle, the transfer engine, the conformance cells). An
unprivileged process on macOS may bind a source port below 1024 (the issue verified it: uid 501 dialed from port 811),
so a `secure` export works with no root and no OS mount.

Read before touching the matching milestone: `crates/cmdr-webdav/CLAUDE.md` + `DETAILS.md` (the closest lifecycle),
`crates/cmdr-sftp/DETAILS.md` § "The read window", § "The write window", § "Renaming without clobbering", § "Coming
back" (the closest byte path), `crates/cmdr-fs/DETAILS.md` § "Bodies a backend gets for free" and § "The HTTP backends'
connection lifecycle", `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Building a new volume", and
`apps/desktop/src/lib/servers/DETAILS.md` § "The model" and § "Add mode, protocol first".

## Loud rules

- ❗ **v3 only, `AUTH_SYS` only.** No v4, no Kerberos, no NLM locking, no WebNFS. A server that offers no v3 is a typed
  refusal, ❌ never a fallback.
- ❗ **The transport is ours and stays in `transport.rs` + `rpc.rs`.** Everything else speaks typed procedure calls.
- ❌ **Never classify by message.** NFS hands us numbers (`nfsstat3`, `mountstat3`, `accept_stat`, `reject_stat`,
  `auth_stat`, errno-like codes); map them by number. `error-string-match` enforces it.
- ❗ **Every wire-touching delegator wraps itself in `noting`**, exactly as WebDAV's and SFTP's do.
- ❗ **The engine owns staging.** `write_is_single_shot` keeps `false`; a `CreateOrReplace` write is one WRITE stream
  onto the temp the engine hands us. ❌ Never stage a second time here.
- ❗ **A hostile server can't make us allocate.** Every length on the wire is checked against the bytes left in its
  record before anything is allocated.
- ❌ **No string-built ids or app paths.** `cmdr_fs::volume::ids` mints `nfs_volume_id` / `nfs_app_root`;
  `server-path-utils.ts` mirrors it.
- ❌ **No `tauri` in the crate** (`index-crate-isolation`), and no user-facing words: typed reasons only.
- Strip milestone tags (`M1`, "Phase 2") from code, comments, test names, and `C.md` before landing
  (`docs/guides/multi-agent-refactors.md`).

## The dependency decision

**Verdict: write our own XDR types and codec; don't take `nfs3_types`.** Decision (revisit in review). Findings, checked
2026-10-20 on crates.io's API and GitHub's:

- `Vaiz/nfs3`: 25 stars, not archived, last push 2026-07-25 (a clippy fix and Dependabot bumps), 15 open issues, one
  maintainer. Alive, but thin.
- `nfs3_types` 0.5.2 (released 2026-05-24, so well past the 3-day window), `nfs3_macros` 0.5.0 (2026-02-14), and
  `nfs3_client` 0.9.0 (2026-05-24). All Unlicense. `nfs3_types` pulls only `nfs3_macros` (`proc-macro2`, `quote`, `syn`
  2, all already in `Cargo.lock`).
- ❗ **`cargo deny check` would refuse it today**: `deny.toml`'s `[licenses].allow` has no `Unlicense`. Adding it is
  loosening an allowlist, which needs David's OK (`AGENTS.md` § "Hard rules"), so an unattended run can't take it.
- ❗ **Its decoder allocates what the wire says.** `Opaque::unpack` does `vec![0u8; len]` from a server-sent `u32`
  before reading a byte (read in `src/xdr_codec/opaque.rs`, 0.5.2, 2026-10-20). A hostile or broken server can ask for 4
  GiB per field. It also copies every READ payload into an owned `Vec`, where we want a `Bytes` slice of the record.
- What it would save: about 1,100 lines of type definitions (`src/nfs3.rs`) plus MOUNT and portmap. Mechanical work an
  agent writes in well under an hour against RFC 1813, RFC 5531, and RFC 1833.
- ❗ **The Unlicense is a public-domain dedication**, so its definitions and its test vectors (`tests/*.rs`) may be read
  and ported freely as a reference, with no attribution owed and no dependency taken. Use them as a cross-check.

Fallback if the own codec fights us: David adds `"Unlicense"` to `deny.toml` and we depend on `nfs3_types` for the type
definitions only, decoding from a length-capped `&[u8]` record (which bounds the allocation to the record) and keeping
our own transport either way. `nfs3_client` is out regardless: one request in flight (#29), undefined state after a
failed reply (#157), IP literals only (#154), IPv4 only (#153).

## Product decisions (revisit in review)

Each is a recommendation for David to confirm or overturn; the plan is built on it.

- **The model: a server is the account, an export is its place** (SMB's host → shares, S3's account → buckets). One
  saved entry per PLACE (`host`, `port`, `export`, `uid`, `gid`, name, auto-reconnect, pin), the S3 store's shape.
  Volume id from `(host, port, export)`. Decision (revisit in review).
- ❗ **The identity port is the CANONICAL one**: `nfs_volume_id(host, port: u16, export)` and the app root take
  `port.unwrap_or(2049)`, ❌ never the port portmap answered (it can move when the server restarts, and a moved port
  must not orphan every tab). Folding: host lowercased, port literal, export by bytes after `normalize_remote_path` (so
  `/volume1/data/` and `/volume1//data` are one export), and only then escaped.
- ❗ **The app root carries the export as ONE percent-escaped segment** (`%` → `%25`, `/` → `%2F`, leading `/` dropped,
  the root export `/` → `%2F`): `nfs://nas.local:2049/volume1%2Fdata/photos`. Why: `server_of_path` must mint a volume
  id from a path alone, and with nested exports (`/share` and `/share/Public`, common on QNAP) a server-only prefix
  can't say which export a path belongs to. The cost, accepted: a file under nested exports has two spellings, one per
  place. ❗ A HUMAN-typed `nfs://nas/volume1/data/photos` (unescaped) is an address, ❌ not an app path: Go to path and
  ⌘K hand it to the NFS prefill and the connect-time split below, never straight to `server_of_path`, which would read
  it as export `/volume1` and folder `data/photos`.
- **The export/folder split happens in `connect`, before the id is minted.** `ServerTarget::Nfs.export` is what the
  person picked or typed (`host:/share/Public/photos` from Go to path). The backend resolves the real export by longest
  match against `EXPORT`, or, when `EXPORT` is refused, by `MNT` on ever-shorter prefixes; the rest becomes the start
  folder when none was given, and the save stores the resolved split. So an id never carries a folder.
- **The uid/gid are a setting of the place, ❌ not its identity.** Changing "Act as user ID" keeps the volume id, the
  tabs, and the favorites; it redials. The app path is `nfs://<host>:<port>/<export>/<path>` with no account in the
  authority. Why: AUTH_SYS has no secret, so the uid is a preference like SMB's username, and a uid typo fixed later
  shouldn't orphan every favorite. Decision (revisit in review).
- **The uid persists in the saved-place store**, plain JSON through `server_list_file`, ❌ not the Keychain: it's not a
  secret, and a Keychain prompt in front of a non-secret is friction. Decision (revisit in review).
- **The default uid/gid guess**, asked once the export is picked (one MNT + one READDIRPLUS on the export root): the
  most common non-zero owner among the root's entries; else the root's own owner if non-zero; else this Mac's uid/gid.
  The field shows the guess as its value with a one-line "Guessed from who owns the files in this export" help. A typed
  `0` earns a warning ("Most servers treat user ID 0 as nobody"), ❌ never a block. Why: on David's QNAP the export root
  is likely root-owned while the files are `1000`'s, so the root's owner alone guesses wrong. Decision (revisit in
  review).
- **xattrs are dropped, and the copy report says so** through the existing loss note (`MetadataCaps` without `XATTRS`),
  ❌ no AppleDouble `._` sidecars. Why: sidecars clutter the NAS for every Linux client and app, the user never asked
  for them, and the loss note already exists for WebDAV and S3. Reading sidecars the macOS kernel client wrote earlier
  back into xattrs on copy-off is deferred. Decision (revisit in review).
- **Dates and mode ARE kept**: SETATTR sets `mtime`, `atime`, and `mode` after a write and on created folders
  (`apply_metadata`). v3 has no creation date, so `CREATED` stays out of the caps. Decision (revisit in review).
- **Polling for changes: one GETATTR per directory a pane shows on the volume, every 3 s, re-listing only when the
  directory's `mtime` or `ctime` moved.** No pane on the volume means no polling at all. Why: 3 s feels live, a GETATTR
  is ~100 bytes each way, and a NAS spends nothing on it; it's the explicit exception to "subscribe, don't poll",
  written down in the crate's `DETAILS.md`. `listing_watch_coverage` stays `None` (a poll misses a change inside one
  tick, so it can't vouch for a pre-flight). Decision (revisit in review).
- **Timeouts**: a 5 s TCP connect per resolved address and a 15 s budget for the whole dial; the silence ladder on
  `cmdr_fs::volume::liveness` with NFS's own `Timings` of 5 s quiet, 5 s per probe, two probes (gone after 15 s of
  silence, against the 30 s the other backends allow); a 60 s budget on metadata and mutation calls; none on READ and
  WRITE as a whole (silence is the signal, as on WebDAV), but a single stuck reply has its own rule (§ Architecture, "A
  reply that never comes"). The probe is the RPC `NULL` procedure on a FRESH connection. Why: the issue asks for "a
  friendly error in seconds", NFS servers answer `NULL` from the RPC layer even when busy, and a stopped `nfsd` on a
  live host already answers instantly with a TCP reset. Decision (revisit in review).
- **The Network view hint**: one muted line under the hub's server list, "NFS servers don't announce themselves. Add one
  with its address, like nfs://nas.local/share." with the address as an "Add server" link that opens the sheet on NFS.
  No Bonjour browse, no port probing of known hosts. Decision (revisit in review).
- **Non-UTF-8 names**: listed nowhere, counted, and logged once per listing (`debug!`, the count only). A lossy name
  would point every operation at bytes that don't exist. Decision (revisit in review).
- **`root_squash` and `all_squash` are the server's business**: no detection; a refusal maps to `PermissionDenied` with
  the path, and the pane's error says what it always says.

## Deferred, and say so

- NFSv4, Kerberos, NLM locks, NFS over UDP, WebNFS public handles.
- Bonjour/mDNS discovery of NFS servers.
- Reading `._` sidecars back into xattrs; writing them.
- Following a symlink whose target is absolute or leaves the export (shown as a symlink, opening it is a typed
  `NotSupported`). Relative links inside the export resolve client-side.
- The index (no background indexing on NFS volumes, like S3 and WebDAV today) and thumbnails beyond what the viewer's
  `read_range` already gives.
- Server-side copy (v3 has none).
- A fuzz target for the decoder (`fuzz/`, the `webdav_propfind` pattern). Worth adding after the effort.
- **The small bug from the issue** (an OS-mounted NFS volume reports `kind: local` and `indexStatus: fresh` in
  `cmdr://state`): separate, not this effort. File it as its own issue.

## The "pane stuck after a dead OS mount" bug: already fixed

The issue's third bullet (the listing hung with no error, and stayed empty after the server came back) was filed as
[#305](https://github.com/getcmdr/cmdr/issues/305), closed 2026-10-02 by `b38d9c6e5` ("a folder on a server that stopped
answering reports itself stalled, keeps retrying, and lands on its own"). The mechanism is
`crates/cmdr-listing/DETAILS.md` § "Stalled listings": an 8 s stall watch, a "still waiting" screen, a re-ask after a
stuck read's refusal, and a fresh probe beside a stuck read on a `BackendKind::Local` volume. Verified by reading the
code and docs on this branch (2026-10-20), ❌ not re-run against a wedged NAS. ❗ That section names its NFS behavior as
"reasoned from #305, not reproduced", so a manual QA pass on the QNAP with the service stopped under an OS mount is the
open item, not a code change. Nothing in this plan touches it, and the direct client doesn't depend on it: a connecting
backend gets "no probe, the reconnect manager plus `live-retry` re-list" from the same section, which is the path the
direct client's recovery takes.

⚠️ `infra/nas/CLAUDE.md` § NFS, which the issue cites for the NAS on/off recipe, isn't in the infra clone on the agent
box (last commit there 2026-09-26). The QNAP manual pass needs the laptop's copy.

## Architecture

`crates/cmdr-nfs` in the shape of `crates/cmdr-webdav`, with SFTP's windowed byte path:

- `xdr.rs`: a bounded `Decoder` over `bytes::Bytes` (every length checked against what's left; opaque data as a
  zero-copy `Bytes` slice) and an `Encoder` into `BytesMut`. `proto/`: `rpc.rs` (call/reply headers, `AUTH_SYS`
  credentials), `portmap.rs` (`GETPORT`), `mount.rs` (`MNT`, `UMNT`, `EXPORT`), `nfs3.rs` (the procedures below and
  their result types).
- `transport.rs`: one TCP connection, record marking (RFC 5531 § 11), a writer task and a reader task, a pending map
  `xid → oneshot`, and a `Liveness` it feeds on every byte. Calls pipeline freely; a reply finds its caller by XID. ❗
  **Cancelling a call forgets its XID** (a late reply is dropped), ❌ never drops the connection: v3 is stateless, so
  nothing on the server depends on the reply being read. A framing error (a bad record mark, a record past the cap)
  poisons the connection and every pending call answers `DeviceDisconnected`; a decode error inside a well-framed record
  fails that one call.
- ❗ **A reply that never comes.** An RPC server may drop one request silently (Linux `nfsd` does under memory pressure
  or a full duplicate-request cache), and on a pipelined connection the other replies keep `Liveness` fed, so the
  silence ladder never fires and the fresh-connection `NULL` probe answers: the caller would wait forever. So a call
  unanswered for 60 s while its connection has answered calls sent AFTER it is resent once with the SAME XID (RFC 5531 §
  9, so the server's duplicate-request cache can answer it), but only if the procedure is idempotent (`NULL`, `GETATTR`,
  `LOOKUP`, `ACCESS`, `READLINK`, `READ`, `WRITE`, `COMMIT`, `READDIR`, `READDIRPLUS`, `FSSTAT`, `FSINFO`, `PATHCONF`).
  ❌ Never resend `CREATE`, `MKDIR`, `REMOVE`, `RMDIR`, `RENAME`, or `SETATTR`: a resend after a success answers `EXIST`
  / `NOENT`, which would read as "the claim is taken" or "already gone". Those fail as `IoError` (`ETIMEDOUT`) and the
  connection is replaced. A second silence on a resent call poisons the connection.
- **The reserved source port**: bind before connect, trying a random port in 600–1023 then walking down, `SO_REUSEADDR`
  on (a reconnect within `TIME_WAIT` would otherwise exhaust the range), `EADDRINUSE` moves on, at `bind` AND at
  `connect` (with `SO_REUSEADDR` the clash with a `TIME_WAIT` 4-tuple surfaces at `connect`, as `EADDRINUSE` or
  `EADDRNOTAVAIL`), capped at 32 tries. `EACCES` / `EPERM` (Linux without `CAP_NET_BIND_SERVICE`) falls back to an
  ephemeral port and records `ReservedPort::Unavailable`, so a `secure` export's refusal can say why. Only the MOUNT and
  NFS connections take a reserved port; portmap and the liveness `NULL` probe dial from an ephemeral one (no server
  checks the port on either), so a silence ladder probing every 5 s doesn't burn the range. Hostnames resolve through
  `tokio::net::lookup_host`, IPv4 and IPv6 both.
- **`AUTH_SYS`**: `stamp`, machine name (this Mac's host name, at most 255 bytes), `uid`, `gid`, and `gids = [gid]` (RFC
  5531 caps the list at 16 and the whole credential body at 400 bytes). A person's other groups are deferred: a file
  readable only through a secondary group answers `PermissionDenied`.
- **The dial**: resolve → portmap `GETPORT` for MOUNT v3/TCP and NFS v3/TCP on port 111 (NFS falls back to 2049 when
  portmap has no answer; an explicit port in the URL skips the NFS lookup only, since MOUNT has no well-known port and
  always comes from portmap, which the fixture relocates through the server's `portmap_port` override) → `MNT` with
  `AUTH_SYS` → root handle, and the reply's `auth_flavors` must list `AUTH_SYS` (1), else `ExportRefused` (a
  Kerberos-only export) → `FSINFO` (`rtpref`, `wtpref`, `rtmax`, `wtmax`, `dtpref`, `maxfilesize`, `properties`) →
  `GETATTR` root. `RPC_PROG_MISMATCH` or no v3 in portmap is `NoNfsV3`. `MNT3ERR_ACCES` (or `AUTH_TOOWEAK` /
  `AUTH_BADCRED` on a call) with `ReservedPort::Unavailable` is `ReservedPortRequired`; otherwise `ExportRefused`.
  `MNT3ERR_NOENT` is `ExportNotFound`.
- **Handles**: a path → `nfs_fh3` cache (LRU, bounded) filled by `LOOKUP` and by READDIRPLUS's per-entry handles. ❗
  `NFS3ERR_STALE` on a cached handle drops it and its subtree and re-LOOKUPs ONCE from the nearest live ancestor; a
  second `STALE` is `NotFound(path)`. A stale ROOT handle re-MNTs.
- ❗ **A cached handle can point at the wrong place without going stale.** Another client renames `/a/b` to `/z/b`: our
  cached `/a/b` handle is still valid, but now names `/z/b`, so a `REMOVE(handle(/a/b), "f")` deletes `/z/b/f`. So every
  MUTATION (`CREATE`, `MKDIR`, `REMOVE`, `RMDIR`, `RENAME`, `SETATTR`) revalidates its parent chain first: one pipelined
  round of `GETATTR`s on every cached ancestor, and the chain stands only if each ancestor's `(mtime, ctime)` still
  matches what it had when its child was looked up (renaming a child bumps its parent's `mtime`); otherwise re-LOOKUP
  from the first level that moved. Our own mutations refresh the cached attrs from their `wcc_data`, so a 10,000-file
  delete in one folder doesn't re-LOOKUP after each file. Opening a file for READ LOOKUPs the leaf fresh (its attrs give
  the length anyway). Mutations are always name-based (`dir handle + name`), ❌ never by a leaf's cached handle.
- **Procedures used**: `NULL`, `GETATTR`, `SETATTR`, `LOOKUP`, `ACCESS`, `READLINK`, `READ`, `WRITE`, `CREATE`
  (`GUARDED`), `MKDIR`, `REMOVE`, `RMDIR`, `RENAME`, `READDIRPLUS` (with `READDIR` + `LOOKUP` as the fallback on
  `NFS3ERR_NOTSUPP`), `FSSTAT`, `FSINFO`, `PATHCONF`, `COMMIT`. Not used: `MKNOD`, `LINK`, `SYMLINK`.
- **The lifecycle is `cmdr_fs::volume::http_session`'s**, which fits as it stands except for one seam: NFS has no
  secret. `HttpBackend` today reads the credential store before a dial and answers `NeedsCredentials` on an empty one.
  Add a provided `fn needs_secret(&self) -> bool { true }` (or make `credential_key` an `Option`) so a no-secret backend
  dials unattended with nothing to offer and never rests in `NeedsCredentials`. ❗ That's a `cmdr-fs` change with WebDAV
  and S3 as its other users: their suites must stay green. The `Client` is the transport; `ping` is `NULL` on a fresh
  connection. ❗ With no secret, the rest of the lifecycle must not reach its credential states either: NFS's `dial`
  answers only `DialRefusal::Transient` (a refused export or a missing reserved port on a REDIAL keeps backing off;
  `Rejected` / `NeedsUser` would park the volume in `NeedsCredentials` and publish `NeedsSignIn`, a sign-in view with
  nothing to type), `UnattendedReconnect` never answers `NoStoredSecret`, and `NfsVolume::sign_in_prompt` is
  `SignInShape::Nothing`. A saved NFS place that isn't mounted needs its own arm in
  `commands/network.rs::get_volume_sign_in_state`, whose fallback answers `Password`.
- **Write safety**: WRITE with `UNSTABLE`, `COMMIT` every 8 MiB of acknowledged data and at the end, comparing every
  WRITE's and COMMIT's `writeverf3` against the reference verifier: the first one seen on this file, and after a resend
  the one the resent WRITEs answered. ❗ A changed verifier means the server rebooted and lost uncommitted data, so
  everything since the last good COMMIT is resent; the bytes since that COMMIT stay in memory for exactly that (≤ 8 MiB
  - the window per upload). A COMMIT is good only when its verifier equals the verifier of EVERY WRITE it covers (RFC
    1813 § 3.3.21); a WRITE answered `FILE_SYNC` or `DATA_SYNC` is stable on its own and needs no resend. A verifier
    that changes twice in one file fails the write. `FILE_SYNC` is the fallback when `FSINFO` reports nothing useful.
    Decision (revisit in review) on the 8 MiB cadence.
- **The read window**: SFTP's `ChunkWindow` shape over positioned `READ`s of `min(rtpref, 1 MiB)`, depth 8 (to be
  re-measured; SFTP's curve is the starting point, not the answer). `eof` or a short count at the declared length ends
  the stream; a short read mid-file is re-asked at the new offset, ❌ never assumed to be the end. `read_range` runs the
  same window.
- **Rename without clobbering**: v3 `RENAME` REPLACES the destination, so `rename(force = false)` claims the name first
  with `CREATE GUARDED` (a file) or `MKDIR` (a folder source), refused atomically by the server when taken, then
  `RENAME`s onto the placeholder; a failed rename removes the placeholder. SFTP's § "Renaming without clobbering" is the
  model and its reasoning carries over. `force = true` is one `RENAME`; `supports_atomic_replace_rename` is `true`. RFC
  1813 § 3.3.14 edges: a RENAME whose source and target are the same file succeeds and does nothing, so the claim must
  not run first (it would answer `EXIST` for the source itself); a folder only replaces an EMPTY folder (`NOTEMPTY` /
  `EXIST` otherwise), which is what makes the `MKDIR` placeholder work. If the placeholder's removal also fails, the
  0-byte claim stays and the error names its path, ❌ never silently left.

### What lands where

- `crates/cmdr-nfs/`: the crate (`CLAUDE.md` + `DETAILS.md`, `src/{lib,errors,params,xdr,transport}.rs`, `src/proto/`,
  `src/volume/{mod,paths,handles,query,streams,writes,mutation,copy,scan,reconnect,poll,volume_impl,testing}.rs`,
  `src/volume/fake_nfs.rs` (an in-process server for the timing and verifier cells, `fake_dav.rs`'s twin)).
- `crates/cmdr-fs`: `BackendKind::Nfs`, `VolumeScheme::Nfs`, `nfs_volume_id`, `nfs_app_root`, the `http_session`
  no-secret seam, and a `ListingHost::open_listing_paths(volume_id)` seam for the poller.
- `crates/cmdr-listing`: an `open_listing_paths` sibling of `volume_ids_with_listings` (`engine/oracle.rs`,
  `operations.rs`); `stalled_on_backend` names NFS a server. The `ListingHost` answer itself is the app's
  `listing_host/backends.rs` (`AppListings`), ❗ not this crate; the trait method gets a default (`Vec::new()`) so
  `NoListings`, `ListingRecorder`, and `crates/cmdr-operations/src/host/testing.rs` need no change. ❗ The existing
  `volumes_with_open_listings` answers volume ids, not paths, so it can't drive the poller.
- App: `network/nfs_volume_wiring.rs`, `network/nfs_known_places.rs`, `saved_servers/wire.rs` (`ServerTarget::Nfs`, new
  outcomes), `commands/servers.rs` dispatch, `commands/nfs.rs` (`list_nfs_exports`, `guess_nfs_identity`),
  `server_volumes.rs` rows, `commands/network.rs` (the sign-in shape arm), `network/manual_servers.rs` (stops refusing
  `nfs://`), `file_system/backend_settings.rs` (an `"nfs"` row in `MAX_CONCURRENT_OPERATIONS_SOURCES`),
  `operations_host/backend_suites/nfs_*`.
- Frontend: `lib/servers/` (the toggle, `address-parser.ts`, `server-form.ts`, `server-path-utils.ts`,
  `connect-refusals.ts`, `saved-server-io.ts`, `ServerFormFields.svelte`), `lib/volume-scheme.ts`, the navigation
  labels, `lib/file-explorer/network/ServersHub.svelte` (the hint), `messages/en/*.json`.
- Fixture: `apps/desktop/test/nfs-servers/`, `scripts/check/stacklease/registry.go`, `scripts/check/checks/nfs_ports.go`
  (+ tests), `fixture-lane-coverage.go`, `desktop-rust-integration-tests.go`, `.github/workflows/ci.yml` paths.

## Milestones and parallelism

Budget: about 3 hours of wall clock, up to three executors. ❗ Every wave below names its file owners; two agents never
own one file in the same wave. The lead enters the worktree once before spawning
(`~/.claude/rules/worktree-creation-script.md`).

```
M0 (one agent, ~20 min)  ──►  wave 1: M1 ∥ M2 ∥ M3 (~60 min)
                          ──►  wave 2: M4 ∥ M5 (~45 min; M2 keeps going if it needs to)
                          ──►  wave 3: M6 ∥ M7 (~40 min)
                          ──►  M8 (lead or one agent, ~15 min) + CI
```

### M0, the seams and the contract (sequential, first)

- **Scope**: everything later waves would otherwise collide on. `crates/cmdr-nfs` skeleton (Cargo.toml, workspace
  member, `lib.rs` exporting the stubbed public surface: `NfsConnectionParams`, `NfsConnectError`, `NfsVolume`,
  `connect_nfs_volume`, `list_exports`, `guess_identity`, each `todo!()`-free, answering `NotSupported` or an `Err`),
  stub `C.md` + `D.md`. `BackendKind::Nfs` (fix every exhaustive match, `stalled_on_backend` → `Server`),
  `VolumeScheme::Nfs`, `nfs_volume_id(host, port, export)`, `nfs_app_root`, `RemoteRoot` support. The IPC contract:
  `ServerTarget::Nfs { display_name, host, port: Option<u16>, export, uid, gid, start_folder, auto_reconnect }`, the new
  `ServerConnectOutcome` variants (`export_not_found`, `export_refused`, `reserved_port_required`, `no_nfs_v3`), the
  `list_nfs_exports(host, port)` and `guess_nfs_identity(host, port, export)` commands as stubs, bindings regenerated.
  The fixture port range: 15480+ (SMB 11480, SFTP 12480, WebDAV 13480, S3 14480 are taken).
- ❗ **Frozen in M0 (landed in `77f421a35` and its follow-up), because M5 builds against them while M4 fills them**:
  - `NfsServer { host, port: Option<u16>, portmap_port: Option<u16> }` (the fixture override; MOUNT's port always comes
    from portmap) with `identity_port()` (`port.unwrap_or(2049)`); `NfsIdentity { uid, gid }`;
    `NfsConnectionParams::new(server, export, identity)` with pub `server`, `identity`, `start_folder: Option<String>`,
    `auto_reconnect`, and `export()` / `volume_id()` / `app_root()`.
  - `connect_nfs_volume(name, params, host: VolumeHost, cancel) -> Result<NfsVolume, NfsConnectError>`, ❌ no
    `volume_id` argument: it runs
    `discovery::resolve_export(server, typed, identity, &cancel) -> ResolvedExport { export, folder }` first, so the
    caller registers under the returned `NfsVolume::volume_id()` and saves `NfsVolume::params()` (the resolved split).
    Also on `NfsVolume`: `sharing_connection(name)`, `set_auto_reconnect(on)`, and `sign_in_prompt` answering
    `SignInShape::Nothing`.
  - `list_exports(&server, cancel) -> Result<Vec<String>, NfsConnectError>` (`ListingRefused` distinct from
    `Unreachable`);
    `guess_identity(&server, export, this_mac: NfsIdentity, cancel) -> Result<NfsIdentityGuess { identity, source: MostCommonOwner | RootOwner | ThisMac }, NfsConnectError>`.
  - `NfsConnectError`: `Unreachable(String)`, `TimedOut`, `NoNfsV3`, `ExportNotFound`, `ExportRefused`,
    `ListingRefused`, `ReservedPortRequired`, `Transport(String)` (the strings log-only), `Cancelled`.
  - IPC: `list_nfs_exports(host, port) -> NfsExportList`
    (`listed { exports } | listing_refused | no_nfs_v3 | timed_out | unreachable`) and
    `guess_nfs_identity(host, port, exportPath) -> GuessedNfsIdentity { uid, gid, source }` (`exportPath`: `export` is a
    reserved word in TypeScript), wrapped in `tauri-commands/nfs.ts`. Only M0 regenerates `bindings.ts`; a later IPC
    type change goes through the lead.
- ❗ **The app-path grammar, which `server-path-utils.ts` mirrors exactly**: `nfs_app_root(host, port, export)` =
  `nfs://<host lowercased>:<port>/<segment>`, where the export is first canonical (`canonical_nfs_export`, i.e.
  `normalize_remote_path`: absolute, no empty / `.` / `..` segment, no trailing `/`, bytes otherwise kept), then its
  leading `/` is dropped and only `%` → `%25` and `/` → `%2F` are escaped; the root export `/` is the segment `%2F`.
  Examples: `/Public` → `nfs://nas.local:2049/Public`, `/volume1/data/` → `nfs://nas.local:2049/volume1%2Fdata`, `/` →
  `nfs://nas.local:2049/%2F`, `/100%/x` → `nfs://nas.local:2049/100%25%2Fx`. Everything after the segment is the
  export-relative path. `server_of_path` refuses an empty segment, a missing port, and any escape other than `%25` /
  `%2F`. No `@` and no account in the authority.
- **Intentions**: after M0, M1–M3 touch disjoint files and compile against fixed signatures.
- **Landmines**: `index-crate-isolation` needs a surface ceiling for the new crate; start it at what the stub exposes
  and let later milestones raise it with a reason. `workspace-member-coverage` and `rcgu_prune_test.go` list crates.
  Exhaustive `BackendKind` matches exist in at least 11 files; the compiler finds them.
- **Test plan**: `pnpm check --fast && pnpm check clippy`; unit cells for `nfs_volume_id` folding (host lowercased,
  export by bytes) in `ids_test`.
- **DONE**: green, committed, pushed with `pnpm land -no-wait`.

### M1, the protocol layer (wave 1, crate only)

- **Owns**: `crates/cmdr-nfs/src/{xdr,transport,errors}.rs`, `src/proto/**`, their `_test.rs` files.
- **Scope**: the codec, the RPC framing and `AUTH_SYS`, portmap, MOUNT, every NFS procedure above, the pipelined
  transport (XID matching, per-call cancel by forgetting, record cap, framing poison, `Liveness` feeding), the
  reserved-port bind with its fallback, `NfsStatus` → `VolumeError` mapping by number with the path carried
  (`NOENT`/`STALE` → `NotFound(path)`, `ACCES`/`PERM` → `PermissionDenied(path)`, `EXIST` → `AlreadyExists(path)`,
  `NOTDIR` → `NotADirectory(path)`, `ISDIR` → `IsADirectory(path)`, `NOTEMPTY` → `IoError` with `ENOTEMPTY`,
  `NOSPC`/`DQUOT` → `StorageFull`, `ROFS` → `ReadOnly`, `NAMETOOLONG`, `JUKEBOX` → retry after 1 s up to 3 times then
  `IoError` with `EAGAIN`), and the dial in `connect_nfs_volume`'s transport half (`NfsConnectError` variants).
- **Intentions**: a transport that never hangs, never allocates on a server's say-so, and lets many READs and WRITEs
  overlap on one connection.
- **Landmines**: record marking allows multi-fragment records; reassemble, and cap the total. XDR pads to four bytes.
  `post_op_attr` and `wcc_data` are optional everywhere; ❌ never unwrap them. READDIRPLUS starts at cookie 0 with a
  zero `cookieverf`, echoes the returned `cookieverf` on every later page, and `NFS3ERR_BAD_COOKIE` restarts the
  listing. Its per-entry `name_handle` and `name_attributes` are optional too: a missing one costs that entry a LOOKUP,
  ❌ never a skipped or attribute-less row. `.` and `..` come back as entries on most servers; drop them by name. The
  READDIR fallback LOOKUPs every entry, so pipeline those LOOKUPs. The XID must not repeat across a reconnect (seed
  randomly). The verifier in `AUTH_SYS` is `AUTH_NONE`.
- **Test plan**: round trips per type; decoding of replies captured with `tcpdump` from the M2 fixture (commit them as
  hex fixtures) and checked against `nfs3_types`' public-domain test vectors by hand. ❗ unfs3 answers READDIRPLUS with
  `NFS3ERR_NOTSUPP`, so its captures hold no READDIRPLUS reply: take that one from a knfsd capture if M2 lands one, else
  hand-build it from RFC 1813 § 3.3.17 and cross-check against `nfs3_types`' vectors; hostile inputs (a 4 GiB opaque in
  a 100-byte record, a truncated record, a fragment past the cap) fail cleanly with no allocation; the transport against
  an in-process loopback server answering out of order, dropping a reply, and resetting mid-record; the reserved-port
  bind on macOS (assert `local_addr().port() < 1024` against a loopback listener). TDD where the shape allows.
- **DONE**: green `pnpm check clippy` and the crate's unit tests; committed.

### M2, the Docker fixture and its lane (wave 1)

- **Owns**: `apps/desktop/test/nfs-servers/**`, `scripts/check/stacklease/registry.go` (the `NFS` stack entry),
  `scripts/check/checks/{nfs_ports.go,nfs_*_test.go,fixture-stacks.go,fixture-lane-coverage.go,desktop-rust-integration-tests.go}`,
  `.github/workflows/ci.yml` (paths filter only).
- **Scope**: an image with **unfs3** (user-space NFSv3, honors `secure`/`insecure`, no privileged container; alive: 161
  stars, not archived, 0.11.0 released 2025-03-28, checked 2026-10-20) plus `rpcbind`, a seeded tree (nested folders, a
  200 MiB file, a 0-byte file, names with spaces and composed/decomposed accents, a symlink inside and one pointing out,
  a file owned by uid 1000 and one by 0), and two exports: `/secure` (the default `secure`) and `/insecure`. `start.sh`
  / `stop.sh` on the lease, ports bound to `127.0.0.1` (`${BIND_ADDR:-127.0.0.1}`), a README. Readiness: `rpcinfo -p`
  lists MOUNT and NFS v3.
- **Intentions**: one stack every NFS cell shares by lease, like SFTP's and WebDAV's.
- **Landmines**: ❗ **unfs3 is a partial stand-in for a NAS**, verified by reading its source (`nfs.c`, `mount.c`,
  `unfsd.8` on `master`, 2026-10-20): (a) READDIRPLUS answers `NFS3ERR_NOTSUPP`, so the fixture only ever exercises the
  READDIR + LOOKUP fallback, while every QNAP, Synology, and TrueNAS (Linux knfsd) serves READDIRPLUS; (b) `secure`
  checks the source port on MNT only, while knfsd checks it on every NFS call (answering `AUTH_TOOWEAK`), so a client
  that dropped the reserved port after MOUNT passes the fixture and fails on a real NAS; (c) CREATE sets only the mode,
  ownership and times come from SETATTR; (d) uid 0 is squashed to `nobody` by default, and the euid switch needs unfs3
  running as root. So spend the first 10 minutes trying a knfsd container (privileged, the host VM's `nfsd` module, on
  OrbStack and on the Linux runner); if it runs, it's the primary server and unfs3 drops out, or stays as the
  READDIRPLUS-less second server for the fallback cell. If it doesn't, keep unfs3, and `fake_nfs.rs` must cover
  READDIRPLUS and a per-call port check (M4 and M6 cells). Record what you found, dated, in the README.
- ❗ **portmap answers the CONTAINER's ports.** Run unfs3 with fixed ports equal to the published host ports
  (`-n 15481 -m 15482`, published `15481:15481`), and `rpcbind` at host 15480 → container 111, so `GETPORT` answers
  ports that work from the host. The client then takes the portmap port from the params (fixtures only: an `nfs://`
  URL's port is the NFS port; crate cells set `portmap_port` on the params' `NfsServer` directly, and only the app-side
  cell reads `NFS_FIXTURE_PORTMAP_PORT`, in the test, ❌ never in the crate). ❗ **A published port almost surely
  rewrites the source port**: a `127.0.0.1` publish on Linux goes through `docker-proxy` (loopback can't be DNAT'd
  without `route_localnet`), and OrbStack's forwarding is a proxy too, so `/secure` would refuse every client whatever
  we bind. Measure it FIRST on OrbStack and on a Linux runner (connect from port 900, have the server log the peer
  port). If the port is rewritten, the `/secure` cell dials the container's own IP (on Linux the bridge routes it with
  the port intact; on OrbStack it's reachable from the host, but whether the port survives is the measurement), and the
  Linux CI job runs `sudo sysctl -w net.ipv4.ip_unprivileged_port_start=600` before the lane so the bind works. If no
  macOS path keeps the port, the `/secure` cell runs on the Linux lane only and says so in its skip reason. Record what
  you measured, dated, in the README. unfs3 runs as root inside the container so it can switch to the request's uid; the
  seed `chown`s accordingly. ❌ Never pause or stop the shared container in a cell; drops go through
  `cmdr_fs::testing::tcp_proxy::TcpProxy`.
- **Test plan**: the check runner's Go tests (`TestNfsFixturePortsMatchComposeDefaults`, loopback binding, mode lists
  agree); `start.sh` on this box; `rpcinfo` and a `showmount -e` against it.
- **DONE**: `./scripts/check/check --check desktop-rust-integration-tests` brings the stack up and finds no NFS cells
  yet without failing; committed.

### M3, the frontend and the add form (wave 1, against M0's stubbed commands)

- **Owns**: `apps/desktop/src/lib/servers/**`, `apps/desktop/src/lib/volume-scheme.ts`,
  `apps/desktop/src/lib/file-explorer/navigation/{connection-state,favorite-reach,filesystem-label}.ts`,
  `apps/desktop/src/lib/file-explorer/pane/{volume-capabilities,volume-tint.svelte,place-connect.svelte}.ts`,
  `apps/desktop/src/lib/file-explorer/network/ServersHub.svelte` (the hint),
  `apps/desktop/src/lib/settings/sections/ServersSection.rows.ts`, `src/lib/intl/messages/en/*.json`,
  `apps/desktop/src-tauri/src/network/manual_servers.rs` (+ test).
- **Scope**: NFS on the protocol toggle (after S3); `address-parser.ts` reads `nfs://host[:port]/export/path` and the
  classic `host:/export/path`; NFS fields in `ServerFormFields.svelte`: Address, Name, an Export select filled by
  `listNfsExports` (a typed path still allowed when listing is refused), "Act as user ID" and "Group ID" number fields
  filled by `guessNfsIdentity`, Advanced with "Reconnect automatically" and Start folder. `server-path-utils.ts` mints
  `nfs://<host>:<port>/<escaped export>` with the canonical port (2049 when none was typed) and the same one-segment
  escaping `nfs_app_root` does (§ "Product decisions"), with shared example rows in both test suites. A sentence and a
  field per new refusal in `connect-refusals.ts`. The sign-in flow already answers `SignInShape` `nothing` with no sheet
  (`open-sign-in.ts`); the Rust side sends it (M4, M5). The hub hint. `manual_servers.rs` stops answering
  `UnsupportedProtocol` for `nfs://` and hands it to the NFS prefill (Go to path and ⌘K open the sheet on NFS).
- **Intentions**: the person types a NAS address, picks an export from a list, sees a sensible uid, and connects.
- **Landmines**: the toggle is the person's (`servers/CLAUDE.md`): an `nfs://` scheme sets it only through a prefill.
  Where an export path ends and a folder begins is unknowable from text: send the whole path, and the backend splits it
  by longest match against `EXPORT` (and, if `EXPORT` is refused, by `MNT` on ever-shorter prefixes). ❌ Never translate
  the new strings; write English plus `@key` descriptions. UI copy is a draft for David's review. The path grammar's TS
  and Rust halves must agree exactly (`isUnderServerRoot`, last `@`, last `:`). ❗ The classic `host:/export/path`
  splits at the FIRST `:/` (outside brackets, so `[fe80::1]:/share` works), ❌ not the last `:`: a folder name may hold
  a colon. An `nfs://` authority has no `@` (no account in it). ❗ **Every label shows the export DECODED**: the
  breadcrumb, the volume label, the switcher row, and `serverSidePath` say `/volume1/data/photos`, ❌ never
  `volume1%2Fdata`; the escaped segment is an identity spelling, not words for a person. ❗ **A typed `nfs://` is an
  ADDRESS unless it is at or under a known volume root**: `nfs://nas/volume1/data/photos` typed unescaped would read as
  export `/volume1`, so Go to path and ⌘K hand it to the NFS prefill and the connect-time longest-match split, ❌ never
  straight to `server_of_path`.
- **Test plan**: `address-parser.test.ts` example rows for every NFS shape; `server-form.test.ts`, `connect-refusals`,
  the a11y test, `manual_servers_test.rs`; `pnpm check svelte-tests-related`.
- **DONE**: green fast checks + related tests + clippy; committed.

### M4, the read-side volume (wave 2, crate; on M1)

- **Owns**:
  `crates/cmdr-nfs/src/volume/{mod,paths,handles,query,streams,scan,reconnect,volume_impl,testing,fake_nfs}.rs` and
  their tests, the `http_session` no-secret seam in `crates/cmdr-fs/src/volume/http_session*.rs`. `src/params.rs` is
  M0's and frozen: M4 reads it, and a field it truly needs goes through the lead (M5 constructs it in parallel).
- ❗ **Leaves the wave-3 seams in place, so M6 and M7 never share a file**: `volume/mod.rs` declares every module wave 3
  fills (`writes`, `mutation`, `copy`, `poll`, `conformance_test`, `reconnect_test`, `connection_drop_test`) as stubs;
  `query.rs`'s `list_directory` already calls `poll::listing_opened(..)` (a no-op stub M7 fills); and `fake_nfs.rs`
  ships every knob wave 3 needs (rotate the write verifier, answer a WRITE short, touch a directory's `mtime`, drop one
  reply, go silent, reset mid-record, demand a reserved source port per call, serve or refuse READDIRPLUS). In wave 3,
  `fake_nfs.rs` is M6's; M7 asks M6 (or the lead) for a knob it lacks.
- **Scope**: tiers 1 and 2a of `volume/DETAILS.md` § "Building a new volume": `list_directory` over READDIRPLUS feeding
  `on_progress` per batch, `get_metadata`, `entry_kind` by LOOKUP attrs (which don't follow links), the handle cache,
  `get_space_info` from `FSSTAT` (`Bounded`, polled every 5 s), `write_access_at` from `ACCESS`, the read window,
  `read_range`, `supports_export`, `ScanSource`, `lane_key` (host + port), the reconnect lifecycle through
  `http_session`, `noting` on every delegator, `ConnectionReporter`, `retirement`, `on_unmount` (UMNT best effort),
  `sharing_connection` and `set_redial_root` for live edits, `list_exports`, `guess_identity`. Every write delegator
  wired in `volume_impl.rs` to a `writes.rs` / `mutation.rs` function that answers `NotSupported`, so M6 fills bodies
  only.
- **Intentions**: browse a NAS export, open a file in the viewer at any offset, copy off it, and see a dead server as a
  typed error in seconds.
- **Landmines**: `listing_watch_coverage` stays `None`, so ❌ nothing calls `authoritative_listing`.
  `composes_new_names` is `true` (a Linux server stores bytes; `crates/cmdr-webdav/DETAILS.md` § "New names go out
  composed"). A file's length is what GETATTR at open said, and a lengthless answer is `StreamLength::Unknown`, ❌ never
  `Known(0)` (#538). `paths_are_os_visible` and `local_path` answer false/None. `guess_identity` probes as this Mac's
  uid/gid (a squashed probe may be refused a READDIR on a `700` root; then the root's own GETATTR owner is the answer).
  `sign_in_prompt` is `Nothing`, and `dial` answers only `Transient` (§ Architecture, the `http_session` seam).
- **Test plan**: Docker cells (`#[ignore]`d, the lane runs `--run-ignored only`): list the seeded tree byte-exact,
  stream the 200 MiB file and checksum it, `read_range` at three offsets, the `/secure` export through the reserved
  port, the `/insecure` one from an ephemeral port, a missing export, `list_exports`, `guess_identity` answering 1000.
  `fake_nfs.rs` cells: a stale handle re-LOOKUPs once, a silent server is `DeviceDisconnected` inside the ladder on a
  paused clock, out-of-order READ replies reassemble in order. WebDAV's and S3's suites stay green on the seam change.
- **DONE**: green; committed and pushed.

### M5, the app wiring (wave 2; on M0 and M3, parallel with M4)

- **Owns**: `apps/desktop/src-tauri/src/network/{nfs_volume_wiring,nfs_known_places}.rs` (+ tests), `network/mod.rs`,
  `commands/servers.rs`, `commands/servers/{wire,outcomes,saves}.rs`, `commands/nfs.rs`, `server_volumes.rs`,
  `server_move.rs`, `file_system/backend_settings.rs`, `commands/favorites.rs`, `commands/network.rs` (the
  `get_volume_sign_in_state` arm: a saved, unmounted NFS place answers `Nothing`, ❌ not the `Password` fallback).
- **Scope**: `connect_and_register` for NFS (the S3 wiring is the closest twin: a place per entry, `DialTicket`, the
  attempt table, `install_retiring_incumbent`, pin on first connect), the saved-place store, `update_saved_server` for
  NFS (a changed uid redials the live place over a new connection; a changed name or start folder shares it), the
  outcome mapping, the servers listing rows, `list_nfs_exports` / `guess_nfs_identity`, the `"nfs"` concurrency row.
- **Landmines**: until M4 lands, `connect_nfs_volume` is M0's stub, so the wiring tests use a fake dial seam the way
  `s3_volume_wiring_test.rs` does. A cancelled connect registers, remembers, and stores nothing. Neither an edit nor a
  connect may change `pinned` (`network/DETAILS.md` § "Moving a pin takes its own writer").
- **Test plan**: wiring unit cells; after M4 lands, one app-side Docker cell connecting through `connect_server`.
- **DONE**: green; committed.

### M6, the write side (wave 3, crate + app suites; on M4)

- **Owns**: `crates/cmdr-nfs/src/volume/{writes,mutation,copy}.rs` + tests, `volume/conformance_test.rs`,
  `apps/desktop/src-tauri/src/operations_host/backend_suites/nfs_*`.
- **Scope**: `write_from_stream` (pipelined UNSTABLE WRITEs, rolling COMMIT, verifier check and resend, progress per
  acknowledged chunk, cancel via the progress callback's `Break`, a byte count that disagrees with the declared length
  fails and removes the partial), `create_file` (`CREATE GUARDED`), `create_directory` + `MakesDirectories`, `delete`
  (REMOVE, or RMDIR for a folder by the attrs it already has; ❌ never recursive), `rename` (the claim, above),
  `apply_metadata` (SETATTR mtime, atime, mode), `metadata_caps` (mode and folder dates, no `CREATED`, no `XATTRS`),
  `notify_mutation` through `PatchSource`, `create_directory_errors_on_existing_dir` true.
- **Landmines**: SETATTR after the last WRITE, before the landing rename, or the rename's `ctime` bump is all that
  survives on some servers. Set only `mode` in `CREATE GUARDED`'s `sattr3`: RFC 1813 lets it carry owner, size, and
  times, but unfs3 ignores all but the mode, and SETATTR after the last WRITE is the portable path anyway. A WRITE
  answering a short `count` is resent from the answered offset, ❌ never treated as done. `wtmax` caps a WRITE. Every
  mutation revalidates its parent chain first (§ Architecture, "A cached handle can point at the wrong place").
- **Test plan**: every applicable `cmdr_fs::volume::conformance` cell against `/insecure`; app-side
  `nfs_transfer_integration_test.rs` copying both ways with checksums, pause and resume mid-file, cancel mid-file
  (nothing left behind), a folder tree, dates and mode kept, the loss note naming xattrs; `fake_nfs.rs` cells for the
  verifier change (resend, then success) and two changes (failure).
- **DONE**: green including the integration lane; committed and pushed.

### M7, outages, recovery, and polling (wave 3; on M4, parallel with M6)

- **Owns**: `crates/cmdr-nfs/src/volume/{poll,reconnect_test,connection_drop_test}.rs`, the `open_listing_paths` seam in
  `crates/cmdr-fs/src/volume/host/listings.rs` (with a default, so no other impl changes), its store half in
  `crates/cmdr-listing/src/{engine/oracle,operations}.rs`, and its answer in
  `apps/desktop/src-tauri/src/listing_host/backends.rs` (+ its test).
- **Scope**: the poller (§ "Product decisions"), started on the first open listing and stopped on the last, reporting
  through `ListingHost::directory_changed` with a full refresh; the drop cells through `TcpProxy` (refused: typed error
  at once, back on its own when it returns; silent: given up inside the ladder; the switch off: no unattended dial); a
  transfer surviving a reconnect through `connection_generation`.
- **Landmines**: ❌ the poller must not wake a dead server every 3 s on top of the backoff: poll only while `Connected`.
  A poll that sees a change for a directory a pane just left drops it. One GETATTR per directory per tick, never per
  entry.
- **Test plan**: poller cells on a paused clock against `fake_nfs.rs` (change seen within one tick, no traffic with no
  listing open, none while disconnected); the drop cells against the fixture.
- **DONE**: green; committed.

### M8, docs, roadmap, close (sequential, last)

- **Scope**: the crate's `C.md` (300–400 words) + `D.md` (the decisions above, the polling exception, the verifier, the
  claim-then-rename, the reserved port, the fixture's port trick and its measurement), `docs/architecture.md` line, the
  capability matrix row in `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md`, `network/C+D.md`,
  `servers/DETAILS.md` (the NFS add mode), `apps/website/src/lib/roadmap.ts` (NFS out of "Support more file systems",
  into done: copy for David's review), the i18n brief for the translator agent, the milestone-tag sweep, and this spec's
  entry in `docs/specs/DETAILS.md` § "The specs" kept current. The QNAP manual pass is David's (or the laptop's).
- **DONE**: CI green on `pnpm land`; the docs-audit agent's pass.

## Invariants (the review register)

1. A `force = false` rename never replaces an existing destination; a forced one replaces atomically.
2. `delete` never recurses; `create_file` never truncates; `create_directory_all` reports a pre-existing leaf as
   `AlreadyExisted`.
3. No byte the server hasn't committed is reported as written: every successful write ends in a COMMIT whose verifier
   matches every WRITE it covers, or in `FILE_SYNC` writes.
4. A partial write never wears the user's filename (the engine's temp plus the landing rename) and is removed on every
   failure path, cancel included.
5. No length from the wire is allocated before it's checked against the record; no record exceeds the cap.
6. A dead server becomes `DeviceDisconnected` within the silence ladder, and every waiting call answers at once; a reset
   answers immediately.
7. A failing call loses only the connection it ran on (`ConnectionReporter`'s generation CAS).
8. A cancelled call frees its caller at once and leaves the connection usable.
9. Every `NotFound` / `PermissionDenied` / `AlreadyExists` carries the path, never server text.
10. No polling while nothing is open on the volume, and none while it's disconnected.
11. Volume ids and app paths come only from `cmdr_fs::volume::ids` and `server-path-utils.ts`, and agree.
12. The crate names no `tauri` and no user-facing words; every user-facing string is English with an `@key` description,
    untranslated.
13. A mutation acts on the folder the path names NOW: its parent chain is revalidated first, and it goes by
    `dir handle + name`, ❌ never by a leaf's cached handle.
14. A non-idempotent procedure is never sent twice; a stuck idempotent one is resent once with its own XID.
