# `cmdr-sftp`

Everything Cmdr says to an SFTP server: one SSH connection per volume, one SFTP channel on it. No `tauri`, no app. Every
word a human reads stays app-side.

## Module map

- `transport.rs`: the ONLY module that names `russh`. Dial, config, auth execution, the host-key handler, the channel.
- `trust.rs` + `known_hosts.rs`: is this the server we met last time? Pure, no SSH types, so the whole decision table is
  unit-tested.
- `auth.rs`: which rung to offer, and what a dropped session may do about it.
- `errors.rs`: `SftpConnectError`, and SFTP status codes into `VolumeError`.
- `volume/`: `mod.rs` (the volume, its params, `connect_sftp_volume`), `paths`, `query`, `streams` (the read window),
  `mapping`, `volume_impl`, `testing` (the Docker fixtures).

## Must-knows

- **❗ Keep `russh` in `transport.rs`.** Eight breaking minors in eight months; a bump has to be one file's problem.
- **❌ Never call `Sftp::close()`.** It awaits a read task that only ends at reader EOF, which a `russh` channel never
  reaches, so it hangs forever. Dropping the session IS the clean shutdown.
- **❗ A cancelled connect panics inside the engine**, so `connect_sftp_volume` runs the dial in a task and awaits the
  JOIN HANDLE. ❌ Never wrap `Sftp::new` in a timeout directly.
- **Host-key trust is keyed by `(host, port, algorithm)` AND pins negotiation to what's already trusted.** ❌ Never one
  without the other: keying alone lets an attacker offer a type we hold no entry for and collect a one-click approval.
- **❌ Never anchor an out-of-root path; refuse it.** `root_anchored` turns `/etc/passwd` into `/srv/data/etc/passwd`,
  which is real and wrong. `to_remote_path` matches by whole component and resolves `..` first.
- **⚠️ A filename that isn't UTF-8 kills the SESSION**, not just the listing: the engine's read task exits and every
  later request reads as disconnected.
- **❌ No `~/.ssh/config` support.** No `ProxyJump`, no `Match`, no host aliases. Someone whose terminal reaches a
  server through a jump host will expect Cmdr to, and it won't.
- **❌ Never read through `File`'s own offset or `read_all`.** It advances by the length it ASKED for, so one short
  answer holes the file and duplicates the bytes after it. `streams.rs` names an offset on every request.
- **Read depth is 8, and 32 is worse than useless.** Four streams at depth 32 outrun the 16 MiB channel window and
  throttle each other; the curve and the memory numbers are in `DETAILS.md`.
- **❗ Every `#[ignore]`d test here is a Docker cell**, by construction: the lane runs `--run-ignored only` over the
  whole package. A measurement that must not gate CI needs an env gate instead (`CMDR_SFTP_BENCH=1`).
- **❌ Never gate behavior on `cfg(test)`**; use `any(test, feature = "testing")`.

The decisions, the hazards in full, and the fixture map: `DETAILS.md`. Read it first.
