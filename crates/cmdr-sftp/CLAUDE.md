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
- **❗ `File::close()` is the opposite: awaited, and only by its LAST clone.** A surviving clone makes it a silent
  no-op and the upload reports success on bytes nobody committed.
- **❗ Every dial goes through `reconnect::guarded_dial`**, which awaits a JOIN HANDLE: a cancelled connect panics
  inside the engine. ❌ Never time out `Sftp::new`.
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
- **❌ A password is offered once unattended; a passphrase never is** (`auth::reconnect_policy`). ❗ The gate is the
  RUNG, not an empty store.
- **❗ Operations ARE how a dead session is found** (no watcher here), so every wire-touching delegator in
  `volume_impl.rs` wraps itself in `noting`.
- **❗ Report transitions, never states** (`state.rs`); a retired volume reports nothing.
- **⚠️ A non-UTF-8 filename kills the SESSION**, not just the listing.
- **❗ Every `#[ignore]`d test here is a Docker cell** (the lane runs `--run-ignored only`). ❌ Never gate on
  `cfg(test)`; use `any(test, feature = "testing")`.

The rest (decisions, error policy, hazards, depth curves, fixtures, the frontend's commands, the known gaps):
`DETAILS.md`. Read it first.
