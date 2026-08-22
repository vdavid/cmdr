# `cmdr-sftp` details

The reasoning behind `CLAUDE.md`'s guardrails, the shape of the crate, and the things about `russh` and
`openssh-sftp-client` that will corrupt or hang if ignored.

Why these two crates and not the eight others: `docs/notes/sftp-crate-evaluation-2026-08-22.md`. ❌ Don't re-litigate
it.

## The connection model

**One SSH connection per volume, one SFTP channel on it. Concurrency is per volume; the read window (when it lands) is
per stream.**

- Concurrency 4 means four Cmdr _operations_ sharing one channel, not four TCP connections. A second connection means a
  second authentication, and with keyboard-interactive that means a second 2FA prompt.
- It keeps us clear of a server's `MaxSessions` and `MaxStartups`.
- `SftpVolumeInner::session` holds an `Arc<SshConnection>` behind an `RwLock`. Every operation **clones the `Arc` out
  from under a short read guard** and then works. ❗ Holding the guard across an operation would serialize every other
  operation behind it, which is exactly the concurrency the one channel exists to provide.
- `Sftp`'s methods take `&self` and `Sftp::fs()` hands back a fresh `Fs`, so N operations genuinely overlap on one
  channel.

`transport.rs` raises `russh`'s channel window from its 2 MiB default to 16 MiB. That number is load-bearing rather than
cosmetic: at 2 MiB, eight 255 KiB reads already fill the channel, so a request window of depth 8 and one of depth 32
measure the same 14–18 MB/s at 50 ms RTT. Raising it is what let depth pay at all — 42 MB/s at depth 32. It does
**nothing** for uploads, where the server's window governs and OpenSSH fixes it at 2 MiB.

## Crate hazards

Read these before writing byte-path code. Each one is a real defect in `openssh-sftp-client` 0.15.7, read from its
source on 2026-08-22.

### 1. `Sftp::close()` hangs forever over a `russh` channel

The engine's `close()` awaits its read task and then its flush task, and the read task only ends at EOF on the reader.
Under the crate's own `openssh` feature that EOF arrives when the child process's stdio closes; a `russh` channel
doesn't give one until the channel is closed, which is what `close()` was supposed to lead to.

**Dropping the session is the clean shutdown, not a leak.** `SftpHandle::drop` orders a shutdown and both tasks exit.
`SftpVolume::disconnect` takes the session out of the lock and drops it; `SshConnection`'s field order (`sftp` first,
then the SSH handle) makes the engine stop writing before the transport goes away.

`disconnecting_drops_the_session_instead_of_closing_it` is the cell that stops someone "fixing" this back — a `close()`
there hangs the test rather than failing it, which is its own kind of loud.

❗ **`File::close()` is a different thing and the write path DOES want it.** Dropping a `File` fires `SSH_FXP_CLOSE` on
a detached task and discards the result; `File::close()` awaits it and returns the error, which for a staged write is
where a server reports a write it could not commit. ❌ Don't read "never `close()`" as covering both.

### 2. A cancelled connect panics the engine's spawned task

The engine's connect task does `tx.send(extensions).unwrap()`, which panics if the `Sftp::new` future was dropped before
the server's hello arrived — that is, on any timed-out or abandoned connect. (Upstream issue #153 is the same `unwrap`,
filed 2026-03-19 and unreproduced there.)

The shape that avoids it, in two layers:

- `connect_sftp_volume` spawns the whole dial on `host.runtime()` and awaits the **join handle**. Dropping the caller's
  future drops the handle, which detaches the task rather than cancelling it.
- `open_sftp_subsystem` does the same again for `Sftp::new` itself, so the subsystem timeout drops a join handle and
  never the future.

❌ Never wrap `Sftp::new` in `tokio::time::timeout` directly. A regression here surfaces as a panic in a spawned task,
which reads as an unrelated test binary crash rather than as a failing assertion —
`abandoning_a_connect_does_not_panic_the_engines_task` is what turns it back into a finding.

### 3. `File::read`'s offset bookkeeping holes a file

Not yet reachable (there is no byte path in this crate yet), but the trap is here for whoever writes it. The engine's
own `File::read` clamps `n`, may return **fewer** bytes than asked, and then advances the file offset by **`n`, the
requested length**. `read_all` loops doing `n -= bytes.len()`, so a short read re-reads from an offset that already
skipped the gap: a silent hole plus duplicated bytes.

❌ Never use `read_all` or `File`'s own offset for a byte path. Issue positioned reads and track offsets yourself.
`File: Clone` shares the remote handle through an `Arc`, so N clones each seeked to their own offset give depth N with
no extra `SSH_FXP_OPEN`. ✅ `TokioCompatFile` is **not** affected: it advances by bytes consumed.

`sftp-fixture-shortreads` is the server that catches it.

### 4. A filename that isn't UTF-8 costs the SESSION

SFTP v3 filenames are bytes with no declared encoding. This is new for Cmdr: SMB is UTF-16 on the wire, so `smb2` could
hand out `String` and never face it.

`openssh-sftp-client` deserializes names through a strict `ssh_format`, and it does so **inside its own read task**,
which then exits. So the damage isn't the one unlistable directory the plan expected: every later request on that
session answers `BackgroundTaskFailure`, and the connection is gone. `map_sftp_error` therefore reports it as
`DeviceDisconnected`, which is the honest answer — the session really is dead.

**It's still the failure worth having.** The alternative crate substitutes U+FFFD, so a name shows in the pane that
addresses nothing, and a folder copy writes it at the destination. Loud and lossless beats quiet and wrong.

The escape hatch, if it bites: vendor `openssh-sftp-protocol` (1 142 lines) plus `ssh_format` under `crates/` as a
**path** dependency and make `NameEntry::filename` byte-backed. ❌ Not `git =`: `deny.toml`'s `unknown-git = "deny"`
forbids it. `a_name_that_is_not_utf8_takes_the_whole_session_down` pins the current behaviour so the day it changes is a
visible day.

## Host-key trust

### The two halves, and why one alone is a hole

A server may hold several host keys (an ed25519 and an rsa, say) and present whichever the negotiation lands on.

- **Keyed by host alone**, a healthy two-key server reads as a CHANGED key, which is the alarm that must stay
  believable. Users who see it on working servers learn to click through it.
- **Keyed by `(host, port, algorithm)` alone**, an attacker offering a type we hold no entry for lands on the
  **unknown** path and collects a one-click approval.

So: key by the triple **and** pin `russh`'s `client::Config::preferred.key` to the algorithms already trusted for that
host. Then a healthy server presents the key we stored, and anything else is a real change. This is what OpenSSH does.

`build_config` filters the pinned names out of russh's **default order** rather than rebuilding the list from them, so
an rsa entry can't outrank an ed25519 one just because of how the names sorted, and an unparseable stored name narrows
nothing instead of emptying the offer.

### The order of consultation

`trust::decide`, strongest signal first:

1. **`@revoked` in `known_hosts`** — the user or their admin was told this exact key is compromised. Nothing outranks
   it, and it must never reach the approval path.
2. **Cmdr's own store** — an approval a human gave in this app. Cmdr's equivalent of `known_hosts`, and authoritative
   for it.
3. **`~/.ssh/known_hosts`** — the fallback, so a server the user's terminal already reaches doesn't ask again. **Read,
   never written**: that file belongs to `ssh`, and a file manager appending to it is a surprise nobody asked for.

### Why the `known_hosts` reader is ours

`russh::keys::known_hosts` splits every line as `hosts keytype blob` and parses the third field as a key. A line
carrying `@revoked` or `@cert-authority` shifts every field by one, so the parse fails and the `?` takes the **whole
lookup** down with it — one certificate-using host in the file and no host in it is readable. Both markers are exactly
the cases that must not be misread.

`known_hosts.rs` therefore parses markers, hashed hostnames (`|1|salt|hash`, HMAC-SHA1, which is the Debian and Ubuntu
default), the `[host]:port` form, and comma-separated host lists — and **skips** any line it can't use rather than
failing the file. ❌ It does not expand `*`/`?` globs: getting that subtly wrong either trusts a host it shouldn't or
alarms on one it should, and an exact match costs at most one extra approval.

### The detached host, and the double that remembers

`VolumeHost::detached()` answers **trust-nothing**. ❌ Not trust-everything: a double that accepted any key is how a
man-in-the-middle regression ships green. But a no-op `record` would leave an approve-then-reconnect harness looping
forever on "unknown → approve → still unknown", so `cmdr-fs` also ships `InMemoryHostKeys`, which actually remembers.
The fixtures use that one.

## The auth ladder

`auth::ladder` offers, in order: the ssh-agent (costs the user nothing), the key file they picked, a password from the
store, keyboard-interactive. A rung with nothing behind it is simply absent; a server refusing one moves the ladder
along, and only running out of rungs is a failure.

**Secrets never travel in `SftpConnectionParams`.** A password and a key passphrase both come from the `CredentialStore`
seam at the moment a session is built and die with it; what the params carry is the key file's **path**, which is a
connection parameter. The store is keyed `service = "host:port"`, `scope = Some(username)` — ❗ not the host alone, or
two accounts on one server would share an entry and a reconnect could retry the wrong account's secret straight into a
lockout. ❗ `credentials()` may block on a Keychain prompt, so it goes to a blocking task.

**A rung with nothing behind it is not a rung the server refused**, and the two carry different typed answers:
`NeedsCredentials` when nothing was ever offered (no agent, no readable key file, no stored secret),
`AuthenticationRejected` when something was offered and turned down. Collapsing them tells someone who has never entered
a password that their password is wrong, and hides the one case a sign-in prompt actually fixes.

**Keyboard-interactive answers a single hidden prompt and no more.** That's the `PasswordAuthentication no` +
`KbdInteractiveAuthentication yes` shape a hardened server without 2FA takes. Anything longer is real 2FA and needs a
human, so it stops rather than guessing and burning an attempt.

**RSA signs with SHA-512.** `PrivateKeyWithHashAlg::new(key, None)` maps to the legacy SHA-1 `ssh-rsa`, which OpenSSH
has refused since 8.8, so an RSA-only server would reject every key we offered.

**Certificates from the agent are skipped.** Validating one needs the CA half of host trust, which this backend
deliberately doesn't do.

### What a dropped session may do, per rung

`auth::reconnect_policy` maps the rung a session was BUILT on:

- **agent** → reconnect freely. A vanished socket or a removed identity surfaces as a refusal on the retry.
- **unencrypted key file** → freely.
- **passphrase-protected key file** → `NeedsCredentials`. The passphrase is a secret and isn't held past the session it
  unlocked, so this genuinely cannot reconnect unattended, however convenient that would be.
- **password** → re-read the store (it may have changed) and try **once**, then `NeedsCredentials`. ❌ Never a loop:
  repeated wrong passwords lock accounts.
- **keyboard-interactive** → never unattended. The server asks the questions and there is nobody to answer them.

The loop that acts on this policy lands with the reconnect work; the policy is here because it's the part that has to be
right rather than the part that's plumbing.

## Path handling

`SftpVolume::to_remote_path` mirrors `SmbVolume::to_smb_path`'s stance: an absolute path outside the volume root is
`NotFound`, and the root is matched by **whole components**.

❌ Never reach for `cmdr_fs::volume::root_anchored` here. It _anchors_: on a volume rooted at `/srv/data` it turns
`/etc/passwd` into `/srv/data/etc/passwd`, which is a real path on a real server and quietly the wrong one.

Two ways of guessing that would each send a request somewhere wrong, both pinned by cells:

- A string prefix compare strips `/srv/data` off a sibling `/srv/data-1/photos` and asks for `-1/photos`, which is a
  legal name.
- `..` has to be resolved **before** the containment check, or `photos/../../etc` is the same escape spelled relatively.

Resolution is lexical — no round trip, no symlink following. The question is "did the caller address something outside
this volume", which is about the path they wrote rather than about what it resolves to, and asking the server would be
both a round trip per path and a TOCTOU window.

There is no mount, so the volume's root IS a remote directory and the paths it hands out ARE remote paths: no second
spelling of the tree, and nothing to translate. Empty, `.`, and a bare `/` all mean the root.

## The `Volume` answers, and why

Beyond the four required methods, `volume_impl.rs` states these deliberately:

- **`lane_key` → server + port + user.** The trait default is the volume root, so two volumes opened at different
  directories on one server would each run full concurrency against the same host and the same single SSH connection.
- **`max_concurrent_ops` reads `settings()` per batch dispatch.** The trait default is **1**. ❗ A namespace with no row
  in `MAX_CONCURRENT_OPERATIONS_SOURCES` silently gets a cautious 2, which is why `"sftp"` has a row with a constant
  accessor even though it is not a user-facing knob.
- **`listing_watch_coverage` → `None`** and there is no watcher, so ❌ nothing here may call `authoritative_listing`.
  Claiming freshness we can't keep is how a pre-flight scan reuses a stale cache and overwrites a file it thought wasn't
  there.
- **`paths_are_os_visible` → false, `local_path` → `None`.** Answering otherwise would let a drag hand Finder a path
  that resolves to nothing, or worse to a local file of the same name.
- **`get_space_info` → `NotSupported`, `space_poll_interval` → `None`.** `statvfs@openssh.com` is **not reachable from
  this crate stack**: `openssh-sftp-client-lowlevel` has no `send_statvfs_request`, and `openssh-sftp-protocol` carries
  only the extension _name_ so the hello parses. There is no `support_statvfs` predicate either — the predicates are
  `expand_path`, `fsync`, `hardlink`, `posix_rename`, and `copy`. Free space needs the protocol crate vendored, which is
  the same escape hatch the filename problem uses. The two answers have to agree, or a pane polls something that always
  refuses.

## Not supported, and say so out loud

**`~/.ssh/config` is not read.** No `ProxyJump`, no `Match`, no per-host aliases, no `IdentityFile` resolution. Someone
whose terminal reaches a server through a jump host will reasonably expect Cmdr to, and it won't: the connection has to
be described in the app. `russh-config` exists and parses most of it, so this is a scope decision rather than a
technical one.

## Which side a test lives on

A cell lives with whatever it **asserts**, never with whatever it connects to.

- **Here**: the contract, the trust table, path translation, the auth ladder, the reading surface, and the crate
  hazards. These are white-box tests — several build a volume with no session behind it.
- **App-side**: anything driving `write_operations`, the volume registry, or the listing cache. ❌ Don't widen this
  crate's public surface to keep a test on that side; move the test instead.

The suites' prelude is `volume/test_support.rs`, ❌ not a `use super::*` glob out of `mod.rs`: what a glob pulls in
isn't determinable without building, which is what made the SMB extraction's suites impossible to size in advance.

**❗ Every `#[ignore]`d test in this crate is a Docker cell**, by construction: `desktop-rust-integration-tests` runs
`--run-ignored only` over the whole package, so an ignored test here runs in CI whatever it's called. Something that
must not gate CI (a throughput measurement, a soak loop) needs its own feature or env gate instead — an `#[ignore]`
would put it in the gating lane under runner contention.

The servers themselves: `apps/desktop/test/sftp-servers/README.md`.

## The public surface is not capped yet

`cmdr-sftp` is in `guardedIndexCrates`, so nothing here may name `cmdr`, `tauri`, or `tauri-specta`. It is deliberately
**not** in `surfaceGuardedCrates` yet: every entry there records David's say-so for its numbers, and the surface is
still growing (the byte path and the IPC surface both add to it). Measure it once the crate is complete and ask him for
the ceiling rather than inventing one.
