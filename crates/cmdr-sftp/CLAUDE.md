# `cmdr-sftp`

Everything Cmdr says to an SFTP server: one SSH connection per volume, one SFTP channel on it. No `tauri`, no app. Every
word a human reads stays app-side.

## Module map

- `transport.rs`: the ONLY module that names `russh`. Dial, config, auth, host-key handler, channel.
- `trust.rs` + `known_hosts.rs`: is this the server we met last time? Pure, so the decision table is unit-tested.
- `auth.rs`: which rung to offer, and what a dropped session may do. `extensions.rs`: what this server can do beyond
  bare v3. `errors.rs`: status codes into `VolumeError`, and the catch-all's resolution.
- `volume/`: `mod` (the volume, `connect_sftp_volume`), `paths`, `query`, `streams` (read window), `writes` (write
  window), `copy` (server-side copy), `scan` (copy and conflict scans), `mutation` (create, delete, rename, pane
  patches), `state` + `reconnect` (connection state, retirement, coming back), `mapping`, `volume_impl`, `testing`.

## Must-knows

- **❗ Keep `russh` in `transport.rs`.** Eight breaking minors in eight months; a bump has to be one file's problem.
- **❌ Never call `Sftp::close()`**: it hangs forever over a `russh` channel. Dropping the session IS the shutdown. ❗
  `File::close()` is the opposite, and the write path awaits it.
- **❗ A cancelled connect panics inside the engine**, so every dial goes through `reconnect::guarded_dial`, which
  awaits a JOIN HANDLE. ❌ Never wrap `Sftp::new` in a timeout.
- **Host-key trust is keyed by `(host, port, algorithm)` AND pins negotiation to what's already trusted.** ❌ Never one
  without the other.
- **❌ Never anchor an out-of-root path; refuse it.** `root_anchored` turns `/etc/passwd` into `/srv/data/etc/passwd`.
- **⚠️ A filename that isn't UTF-8 kills the SESSION**, not just the listing.
- **❌ No `~/.ssh/config` support.** No `ProxyJump`, no `Match`, no aliases. People will expect it; it isn't there.
- **❌ Never read through `File`'s own offset or `read_all`**: it advances by the length it ASKED for, so one short
  answer holes the file. Name an offset on every request, on all three byte paths.
- **❌ Never wire `rename(force = false)` to `Fs::rename`**: it sends `posix-rename@openssh.com`, which REPLACES the
  destination. Claim the name first. `force = true` does want the extension.
- **❌ Never stat a path to decide whether to write it**; stat it to explain a write that already failed. As a guard the
  same question is a TOCTOU window with an overwritten file in it.
- **Read and write depth are both 8**, from measured curves rather than the plan's guesses.
- **❗ Capabilities are read once, at dial, through `SshConnection::extensions()`.** ❌ Never a `Sftp::support_*`
  predicate at a call site: a fallback nobody can drive without a server that lacks the extension is untested.
- **❌ A password is offered once unattended; a key passphrase never is** (`auth::reconnect_policy`). Repeated wrong
  passwords lock accounts, and storing a passphrase would undo encrypting the key.
- **❗ Every wire-touching delegator in `volume_impl.rs` wraps itself in `noting`.** With no watcher, the operations ARE
  how a dead session is found; one added without it leaves a volume showing as connected.
- **❗ Report transitions, never states** (`state.rs`), and a retired volume reports nothing at all.
- **❗ Every `#[ignore]`d test here is a Docker cell**, by construction: the lane runs `--run-ignored only` over the
  package. A measurement that must not gate CI needs an env gate (`CMDR_SFTP_BENCH=1`), and its own scratch directory.
- **❌ Never gate behavior on `cfg(test)`**; use `any(test, feature = "testing")`.

The decisions, the error-policy table, the hazards in full, and the fixture map: `DETAILS.md`. Read it first.
