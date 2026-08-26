# `cmdr-sftp`

Everything Cmdr says to an SFTP server: one SSH connection per volume, one SFTP channel on it. No `tauri`, no
user-facing words.

## Module map

- `transport.rs` (the ONLY module naming `russh`), `trust.rs` + `known_hosts.rs` (host-key decisions), `auth.rs` (the
  rung ladder), `extensions.rs`, `errors.rs`.
- `volume/`: the `Volume` impl by job — `paths`, `query`, `streams`, `writes`, `copy`, `scan`, `mutation`, `state` +
  `reconnect`, `mapping`, `volume_impl`, `testing`.

## Must-knows

- **❗ Keep `russh` in `transport.rs`.** Eight breaking minors in eight months; a bump stays one file's problem.
- **❌ Never call `Sftp::close()`** — it hangs forever over a `russh` channel; dropping the session IS the shutdown.
- **❗ `File::close()` is the opposite: awaited, and only by its LAST clone.** A surviving clone makes it a silent no-op
  and the upload reports success on bytes nobody committed.
- **❗ A hello that never arrives ends in `transport::stop_engine`, ❌ never a bare `drop(session)`**: the engine's
  tasks hold the channel and a sender the session lives on, so the socket would stay open for the life of the process.
  `transport::PendingEngine` owns the pair so every ending reaches it, an ABANDONED dial's included, from its `Drop`.
  `DETAILS.md` § "2. An abandoned `Sftp::new`".
- **❗ A connect is called off with a `CancellationToken`**, which is what makes it answer `Cancelled` and register,
  remember, and store nothing. Every phase stops where it stands, the hello included.
- **Host-key trust keys on `(host, port, algorithm)` AND pins negotiation to it** — ❌ never one without the other. ❌
  Never record a fingerprint without re-asking the server (`volume::approve_host_key`).
- **❌ Never anchor an out-of-root path; refuse it.** `root_anchored` turns `/etc/passwd` into `/srv/data/etc/passwd`.
- **❌ Never read through `File`'s own offset or `read_all`**: it advances by the length it ASKED for, so one short
  answer holes the file. Name every offset.
- **❌ Never wire `rename(force = false)` to `Fs::rename`** — `posix-rename@openssh.com` REPLACES the destination.
- **❌ Never stat a path to decide whether to write it**: a TOCTOU window with an overwritten file in it.
- **❗ Capabilities are read once, at dial** (`SshConnection::extensions()`). ❌ Never a `Sftp::support_*` predicate at
  a call site.
- **❗ What this backend SAYS is as load-bearing as what it does.** The copy engine gates on `supports_export` before
  calling anything, and `NotFound` / `PermissionDenied` carry the PATH — ❌ never the server's wording, which the
  frontend renders as the missing file's name.
- **❗ Two independent switches gate a reconnect, `auto_reconnect` first, then the rung.** Off means no unattended dial
  at all: `NotSupported` + `Disconnected`, ❌ never `NeedsCredentials`. Neither switch changes the other's meaning, so
  an attended sign-in REFRESHES a remembered secret and never seeds one. `auth::unattended_reconnect` is what tells the
  frontend a switch is on and can't work.
- **❌ A secret is offered once unattended, then a person** (`auth::reconnect_policy`), on both the password and the
  encrypted-key rungs.
- **❗ Operations ARE how a dead session is found** (no watcher here), so every wire-touching delegator in
  `volume_impl.rs` wraps itself in `noting`.
- **❗ Report transitions, never states** (`state.rs`); a retired volume reports nothing.
- **⚠️ A non-UTF-8 filename kills the SESSION**, not just the listing.
- **❗ Every `#[ignore]`d test here is a Docker cell** (the lane runs `--run-ignored only`). ❌ Never gate on
  `cfg(test)`; use `any(test, feature = "testing")`.

The rest (decisions, error policy, hazards, depth curves, fixtures, the frontend's commands, the known gaps):
`DETAILS.md`. Read it first.
