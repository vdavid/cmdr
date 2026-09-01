# What the WebDAV backend still owes

The backend and its IPC surface are done: `crates/cmdr-webdav` connects to Nextcloud, ownCloud, Synology, Fastmail, and
a generic NAS over `reqwest` + `quick-xml`, lists with PROPFIND, reads with ranged GETs, writes through a staged
PUT+MOVE so a partial upload never wears the user's filename, handles MKCOL, DELETE, MOVE, and COPY, takes its Basic-auth
password from the `CredentialStore` seam, holds a three-valued connection state with one unattended re-probe before it
asks a person, and answers every connect with a typed outcome. TLS trust comes from the system roots; there are no host
keys, no Digest auth, no watcher, and no locks. `crates/cmdr-webdav/DETAILS.md` is the canonical account of all of it,
the app-side stores and wiring live in `apps/desktop/src-tauri/src/network/DETAILS.md` § "The WebDAV twin", the commands
in `apps/desktop/src-tauri/src/commands/DETAILS.md`, and the Docker fixtures in
`apps/desktop/test/webdav-servers/README.md`. This file exists so what is left stays schedulable.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## Before merge: three things David runs locally

This branch was built in a cloud box without the app's system libraries, so three steps that need a full local toolchain
are still open. Each is minutes, not hours.

- [ ] **Regenerate `bindings.ts`.** The new `commands/webdav.rs` types are on the Rust side; the TS wrappers in
      `apps/desktop/src/lib/tauri-commands/webdav.ts` compile against the regenerated file. The procedure is the usual
      one in `apps/desktop/CLAUDE.md`.
- [ ] **Run `pnpm check --include-slow`.** The `webdav_integration_` cells (the crate's Docker cells plus
      `write_operations/webdav_transfer_integration_test.rs`) need the `webdav` stack up, which only a Docker-capable
      machine gives. Ports 13480+ under the `cmdr-webdav.lock` + `cmdr-webdav-leases` lease namespace:
      `scripts/check/DETAILS.md` § "Two fixture stacks, two lease namespaces".
- [ ] **Measure the public surface for `index-crate-isolation`.** The check caps each backend crate's public surface at
      the number its audit landed on (`surfaceGuardedCrates` in `scripts/check/checks/index-crate-isolation.go`).
      `cmdr-webdav` has no entry yet; run the count, review the surface once, and pin it.

## 1. There is no WebDAV frontend (shared with SFTP, the bigger item)

**The gap**: a connected WebDAV volume is registered and navigable by `volumeId`, and every write path can reach it,
but nothing puts it on screen. `volume_listing::complete` has no WebDAV arm, so the sidebar never shows one, and
`resolve_path_volume` / `resolve_location` don't answer for a remote path. This is the same gap SFTP has
(`docs/specs/later/sftp-follow-ups.md` § 1), and the two want one design: one "Servers" section, one sign-in dialog,
one connect form that branches on the scheme. The WebDAV form is simpler (URL + username + password, no key file, no
host-key approval step), which makes it the easier first arm to build.

**What already exists**: `crates/cmdr-webdav/DETAILS.md` § "Connecting from the frontend" carries every command, the
connect outcomes a sign-in UI branches on, and the four lines that wire the cancel button; `getVolumeSignInState` answers
live what a banner should ask for, and the `volume-connection-changed` event drives the reconnect banner unchanged.

**Cost**: the UI work, roughly a week for both backends together. David designs and builds it.

## 2. Certificate trust-on-first-use for self-signed NAS certificates

**The gap**: most home NAS boxes present a self-signed certificate, and today that connect answers
`certificate_untrusted` and stops. There is no way to say "trust this one".

**The shape**: a fingerprint prompt (SHA-256 of the leaf, the way the SFTP host-key prompt shows a key fingerprint), an
app-side trusted-certificate store mirroring `apps/desktop/src-tauri/src/network/sftp_host_keys.rs` (keyed
`(host, port)`, one entry per fingerprint, with `approve` / `forget` / `list` commands beside the WebDAV ones), and a
`reqwest` client built with a custom root or verifier for that host. The tricky half is the verifier: `reqwest`'s
`add_root_certificate` accepts a CA, not a leaf, so a self-signed leaf either goes in as its own root or the client
uses a `rustls` verifier that compares the presented chain against the pinned fingerprint. Decide once, write it down in
`crates/cmdr-webdav/DETAILS.md`.

**Cost**: two to three days, most of it the verifier and its tests against a fixture that serves a self-signed
certificate (the Apache stack can grow a third service for it).

## 3. Digest auth, or a typed refusal

**The gap**: the crate speaks Basic only. The `webdav-fixture-digest` service (port 13481) exists so the "this server
only offers Digest" path is covered, and today it lands on `authentication_rejected`.

**Two ways to close it**: implement RFC 7616 Digest in the client (a challenge round trip plus MD5 / SHA-256 hashing,
about a day with the fixture already there), or add a typed `digest_only` connect outcome so the UI can say what the
server wants rather than "wrong password" (an afternoon). Synology's default is Basic over HTTPS and Fastmail is Basic,
so the refusal is enough for the servers the crate names; do the full implementation only when a real user's server needs
it.

**Cost**: an afternoon for the refusal, a day for Digest.

## 4. Nextcloud chunked upload for large files

**The gap**: RFC 4918 PUT is single-shot, and Nextcloud's reverse proxy defaults cut a request at a few hundred MB.
Nextcloud's own clients use the `remote.php/dav/uploads/<user>/<id>` chunking API (MKCOL a staging collection, PUT
numbered chunks, MOVE the collection's `.file` to the destination) for anything over its chunk size.

**The shape**: detect a Nextcloud server once per connect (the `OC-` response headers or the `/status.php` probe),
and route writes above a threshold through the chunking API instead of the staged PUT+MOVE. The staged write already
ends in a MOVE, so the assembly step is the same last line.

**Cost**: two days, including a Nextcloud container in the fixture stack (heavier than Apache; keep it out of the
default lane).

## 5. Server-side quota via RFC 4331

**The gap**: `quota-available-bytes` / `quota-used-bytes` on the root collection tell the free-space indicator what the
server has. Without it a WebDAV volume reports no free space, and the copy preflight can't warn before a transfer that
won't fit.

**Cost**: half a day. One extra property on the root PROPFIND, and a `None` when the server omits it (Apache `mod_dav`
does, so the fixture covers the absent case only).

## 6. WebDAV locks: deliberately not

RFC 4918 LOCK / UNLOCK guard against concurrent editors, which Cmdr is not: a file manager copies, moves, and renames
whole files, and the staged PUT+MOVE already keeps a partial off the user's filename. Locks add a server-side state that
outlives a crash (a dead lock a person has to clear) for no operation Cmdr performs. Revisit only if a server refuses
unlocked writes in practice.
