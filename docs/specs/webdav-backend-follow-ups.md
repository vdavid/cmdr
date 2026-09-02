# What the WebDAV backend still owes

The backend and its IPC surface are done: `crates/cmdr-webdav` connects to Nextcloud, ownCloud, Synology, Fastmail, and
a generic NAS over `reqwest` + `quick-xml`, lists with PROPFIND, reads with ranged GETs, writes through a staged
PUT+MOVE so a partial upload never wears the user's filename, handles MKCOL, DELETE, MOVE, and COPY, takes its
Basic-auth password from the `CredentialStore` seam, holds a three-valued connection state with one unattended re-probe
before it asks a person, and answers every connect with a typed outcome. TLS trust comes from the system roots; there
are no host keys, no Digest auth, no watcher, and no locks. `crates/cmdr-webdav/DETAILS.md` is the canonical account of
all of it, the app-side stores and wiring live in `apps/desktop/src-tauri/src/network/DETAILS.md` § "The WebDAV twin",
the commands in `apps/desktop/src-tauri/src/commands/DETAILS.md`, and the Docker fixtures in
`apps/desktop/test/webdav-servers/README.md`. This file exists so what is left stays schedulable.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## Still open before this is trusted against a real server

The local toolchain steps are done: `bindings.ts` is regenerated from the Rust types, `pnpm check` is green, and the
Apache stack has been up (`LOCK` answers 200, `HEAD large.bin` reports 4,194,304, the Digest server challenges with
`Digest realm="cmdr"` alone and accepts `curl --digest`, and startup logs no `AH00526`). The `webdav_integration_` lane
runs 178 cells green on ports 13480+ under the `cmdr-webdav.lock` + `cmdr-webdav-leases` namespace
(`scripts/check/DETAILS.md` § "Two fixture stacks, two lease namespaces").

What no Apache fixture can answer, and what a Nextcloud one now does:

- [x] **The two claims about real servers are observed**, by `webdav-fixture-nextcloud` (port 13482, its own stack mode)
      and `crates/cmdr-webdav/src/volume/nextcloud_test.rs`. The answers, with their anchors and what they change:
      `crates/cmdr-webdav/DETAILS.md` § "What a real server answers". One of the two came back the other way round,
      which is the part worth knowing before reading it.
- [ ] **Nothing exercises `streams.rs`'s skip-locally branch.** It handles a 200 to a ranged GET, and no server has been
      watched answering one, so it is data-path code no test covers. A fixture that strips `Range` (a fourth httpd
      service, an hour) would close it without needing a server that does it in the wild.
- [ ] **A Synology, and a Nextcloud behind nginx + php-fpm.** The 411 claim is plausible for a deployment where PHP
      never sees a chunked body, and that is the shape the Docker image doesn't have. Neither is automatable here;
      `CMDR_WEBDAV_TEST_URL` is what points the whole suite at one by hand (`apps/desktop/test/webdav-servers/README.md`
      § "Against a server of your own").

The public surface IS pinned (6 / 1 / 8, measured 2026-09-01), so widening it is the usual conversation.

Two smaller things the review pass flagged and did not settle, each an hour at most:

- [ ] **Self-entry skip behind a rewriting proxy.** `query.rs` skips the collection's own row by comparing its href with
      the base path; a reverse proxy that rewrites hrefs would leave a phantom child named after the directory. Test
      against a proxied Nextcloud.
- [ ] **A file where an ancestor directory should be.** `create_directory_all` reads a 405 on an ancestor MKCOL as "it
      exists", so a FILE in the way surfaces as the leaf's `NotFound` rather than a clear refusal.

## 1. There is no WebDAV frontend (the bigger item, and shared with SFTP)

`docs/specs/servers-in-the-sidebar.md` owns it: one design for both backends, five milestones, roughly a week. Nothing
on the backend side is missing for it, and this crate's § "Connecting from the frontend" is the contract it builds
against.

## 2. Certificate trust-on-first-use for self-signed NAS certificates

**The gap**: most home NAS boxes present a self-signed certificate, and today that connect answers
`certificate_untrusted` and stops. There is no way to say "trust this one".

**The shape**: a fingerprint prompt (SHA-256 of the leaf, the way the SFTP host-key prompt shows a key fingerprint), an
app-side trusted-certificate store mirroring `apps/desktop/src-tauri/src/network/sftp_host_keys.rs` (keyed
`(host, port)`, one entry per fingerprint, with `approve` / `forget` / `list` commands beside the WebDAV ones), and a
`reqwest` client built with a custom root or verifier for that host. The tricky half is the verifier: `reqwest`'s
`add_root_certificate` accepts a CA, not a leaf, so a self-signed leaf either goes in as its own root or the client uses
a `rustls` verifier that compares the presented chain against the pinned fingerprint. Decide once, write it down in
`crates/cmdr-webdav/DETAILS.md`.

**Cost**: two to three days, most of it the verifier and its tests against a fixture that serves a self-signed
certificate (the Apache stack can grow a third service for it).

## 3. Digest auth, or a typed refusal

**The gap**: the crate speaks Basic only. The `webdav-fixture-digest` service (port 13481) exists so the "this server
only offers Digest" path is covered, and today it lands on `authentication_rejected`.

**Two ways to close it**: implement RFC 7616 Digest in the client (a challenge round trip plus MD5 / SHA-256 hashing,
about a day with the fixture already there), or add a typed `digest_only` connect outcome so the UI can say what the
server wants rather than "wrong password" (an afternoon). Synology's default is Basic over HTTPS and Fastmail is Basic,
so the refusal is enough for the servers the crate names; do the full implementation only when a real user's server
needs it.

**Cost**: an afternoon for the refusal, a day for Digest.

## 4. Nextcloud chunked upload for large files

**The gap**: RFC 4918 PUT is single-shot, and Nextcloud's reverse proxy defaults cut a request at a few hundred MB.
Nextcloud's own clients use the `remote.php/dav/uploads/<user>/<id>` chunking API (MKCOL a staging collection, PUT
numbered chunks, MOVE the collection's `.file` to the destination) for anything over its chunk size.

**The shape**: detect a Nextcloud server once per connect (the `OC-` response headers or the `/status.php` probe), and
route writes above a threshold through the chunking API instead of the staged PUT+MOVE. The staged write already ends in
a MOVE, so the assembly step is the same last line.

**Cost**: two days. The Nextcloud container it needs already exists: `webdav-fixture-nextcloud`, in its own stack mode
outside the default lane (`apps/desktop/test/webdav-servers/README.md` § "The Nextcloud server").

## 5. Server-side quota via RFC 4331

**Observed on Nextcloud.** `get_space_info` reads `quota-available-bytes` / `quota-used-bytes` off the root collection
and answers `NotSupported` when the server omits them or reports either as negative, polled every 60 s. The Nextcloud
fixture carries both properties on two accounts, and the cells confirm the numbers are the ACCOUNT's quota rather than
the disk's; `crates/cmdr-webdav/DETAILS.md` § "What a real server answers" has them. The catch worth knowing: a stock
Nextcloud account has no quota and answers the `-3` sentinel, so the free-space indicator shows nothing for most real
users.

**What's left**: the same look at a Synology, by hand through `CMDR_WEBDAV_TEST_URL`. An hour.

## 6. WebDAV locks: deliberately not

RFC 4918 LOCK / UNLOCK guard against concurrent editors, which Cmdr is not: a file manager copies, moves, and renames
whole files, and the staged PUT+MOVE already keeps a partial off the user's filename. Locks add a server-side state that
outlives a crash (a dead lock a person has to clear) for no operation Cmdr performs. Revisit only if a server refuses
unlocked writes in practice.
