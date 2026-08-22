# `cmdr-sftp`

Everything Cmdr says to an SFTP server: one SSH connection per volume, one SFTP channel on it. No `tauri`, no app. Every
word a human reads stays app-side.

## Module map

- `transport.rs`: the ONLY module that names `russh`. Dial, config, auth, host-key handler, channel.
- `trust.rs` + `known_hosts.rs`: is this the server we met last time? Pure, so the decision table is unit-tested.
- `auth.rs`: which rung to offer, and what a dropped session may do about it. `errors.rs`: status codes into
  `VolumeError`, and the catch-all's resolution.
- `volume/`: `mod` (the volume, `connect_sftp_volume`), `paths`, `query`, `streams` (read window), `writes` (write
  window), `mutation` (create, delete, rename, pane patches), `mapping`, `volume_impl`, `testing` (Docker fixtures).

## Must-knows

- **❗ Keep `russh` in `transport.rs`.** Eight breaking minors in eight months; a bump has to be one file's problem.
- **❌ Never call `Sftp::close()`.** It hangs forever over a `russh` channel. Dropping the session IS the clean
  shutdown. ❗ `File::close()` is the opposite: the write path awaits it, because a drop throws away the server's last
  word on bytes it couldn't commit.
- **❗ A cancelled connect panics inside the engine**, so `connect_sftp_volume` awaits a JOIN HANDLE. ❌ Never wrap
  `Sftp::new` in a timeout directly.
- **Host-key trust is keyed by `(host, port, algorithm)` AND pins negotiation to what's already trusted.** ❌ Never one
  without the other: keying alone collects a one-click approval for an algorithm we hold no entry for.
- **❌ Never anchor an out-of-root path; refuse it.** `root_anchored` turns `/etc/passwd` into `/srv/data/etc/passwd`.
- **⚠️ A filename that isn't UTF-8 kills the SESSION**, not just the listing.
- **❌ No `~/.ssh/config` support.** No `ProxyJump`, no `Match`, no aliases. People will expect it; it isn't there.
- **❌ Never read through `File`'s own offset or `read_all`.** It advances by the length it ASKED for, so one short
  answer holes the file. Name an offset on every request, both windows.
- **❌ Never wire `rename(force = false)` to `Fs::rename`.** It sends `posix-rename@openssh.com` where the server offers
  it, and that REPLACES the destination — the opposite of what every unanswered conflict prompt relies on. Claim the
  name first. `force = true` does want the extension.
- **❌ Never stat a path to decide whether to write it; stat it to explain a write that already failed.** As a
  pre-flight guard the same question is a TOCTOU window with an overwritten file in it.
- **Read and write depth are both 8**, from measured curves rather than the plan's guesses.
- **❗ Every `#[ignore]`d test here is a Docker cell**, by construction: the lane runs `--run-ignored only` over the
  package. A measurement that must not gate CI needs an env gate (`CMDR_SFTP_BENCH=1`), and its own scratch directory.
- **❌ Never gate behavior on `cfg(test)`**; use `any(test, feature = "testing")`.

The decisions, the error-policy table, the hazards in full, and the fixture map: `DETAILS.md`. Read it first.
