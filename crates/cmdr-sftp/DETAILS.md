# `cmdr-sftp` details

The reasoning behind `CLAUDE.md`'s guardrails, the shape of the crate, and the things about `russh` and
`openssh-sftp-client` that will corrupt or hang if ignored.

Why these two crates and not the eight others: `docs/notes/sftp-crate-evaluation-2026-08-22.md`. ❌ Don't re-litigate
it.

## The connection model

**One SSH connection per volume, one SFTP channel on it. Concurrency is per volume; the read window is per stream.**

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
measure the same 14–18 MB/s at 50 ms RTT. Raising it is what lets depth pay at all (§ "The read window"). It does
**nothing** for uploads, where the server's window governs and OpenSSH fixes it at 2 MiB.

## The read window

A sequential SFTP read is one request, one round trip, one chunk: 255 KiB over a 50 ms link is about 5 MB/s whatever the
server and its disks can do. `streams.rs` keeps `READ_WINDOW_DEPTH` positioned reads in flight and reassembles them in
file order, which measures 7× that on the same link.

**The shape**, following `cmdr-smb/src/volume/streams.rs`:

- A producer task on `host.runtime()` (❌ never `tokio::spawn` — a backend inherits whatever runtime it is called on),
  feeding a bounded channel two chunks deep. Peak memory per stream is `(depth + 2) × 255 KiB` whatever the file's size.
- `total_size` crosses a oneshot **before the constructor returns**, so a caller's first progress tick is honest.
- Dropping the stream cancels the producer. Both signals work on their own: the cancel oneshot fires from `Drop`, and
  the closed channel stops the next `send`.
- `ChunkWindow` is generic over a `PositionedRead` seam, so reassembly is unit-tested against a double that short-reads
  and answers backwards on purpose. `RemoteFile` is the only production implementation, and `FuturesOrdered` is what
  turns out-of-order completions into an in-order stream with no reassembly buffer to keep.

**The open costs two round trips, not three.** The `fstat` and the FIRST chunk read go out together, so a small file —
the folder-copy case, and the one that multiplies by ten thousand — is open plus one round trip. Reading past the end of
a file is an empty answer rather than an error, which is what makes the speculative first read safe.

**The length is what the `fstat` at open says.** A file that GREW under a running read isn't chased (SMB fixes its
length at the CREATE response for the same reason); a file that SHRANK reports what exists, because a read answering
empty ends the stream wherever it lands. ❗ So the hint on `open_read_stream_with_hint` buys nothing here and the trait
default is the answer: SFTP has no compound open to spend it on, and the `fstat` rides along with a read we were going
to issue anyway. `open_read_stream_at_offset` stays `NotSupported` — nothing calls it with a non-zero offset, and
`CheckpointStream` parks a paused copy in place rather than reopening.

**`read_range` runs the same window** over exactly `[offset, offset + len)`, and returns short only at end of file, so a
caller never loops for a network short read. It is the natural place to reach for `read_all`, which is hazard 3.

**Background scan reads run at `SCAN_WINDOW_DEPTH` = 2.** The index scan's prefetch shares the one channel with whatever
the user is doing, and a background read that fills the channel window is a foreground read waiting for it. ❗ There is
no scan-connection pool like SMB's, because a second connection is a second authentication (§ "The connection model"),
so `begin_scan_session` / `end_scan_session` keep their no-op defaults.

### The depth, and the curve that set it

Measured 2026-08-22 against `sftp-fixture-bench` (Alpine + OpenSSH `sftp-server`, `netem delay 50ms`, 128 MiB export,
release build, 32 MiB read per stream), aggregate MB/s over one SSH channel:

| depth | 1 stream | 4 streams | requests in flight (1 / 4) |
| ----- | -------- | --------- | -------------------------- |
| 1     | 4.8      | 18.9      | 1 / 4                      |
| 2     | 9.4      | 37.0      | 2 / 8                      |
| 4     | 17.7     | 38.7      | 4 / 16                     |
| **8** | **32.5** | **38.7**  | 8 / 32                     |
| 16    | 28.8     | 37.1      | 16 / 64                    |
| 32    | 23.5     | 35.3      | 32 / 128                   |

**Depth 8 is the joint optimum, and 32 is worse than useless.** One stream peaks there and loses a quarter of its
throughput by depth 32; four streams are within noise of their own peak at 8 and decline past it. The plan's starting
number of 32 was the single-stream loopback shape, and the SMB precedent held: useful width is 4–8, not the number a
one-stream benchmark suggests.

Why deeper hurts is the two windows interacting, exactly as the plan predicted: four streams at depth 32 is ~32 MiB of
outstanding read data against a 16 MiB channel window, so the streams spend the difference throttling each other. What
the aggregate column shows is that the ceiling is **total** requests in flight, not per-stream depth: 8 outstanding
requests (four streams at depth 2) already reach 37 MB/s, and nothing above that buys anything.

- **`max_pending_requests` moves nothing**, as the plan said it wouldn't. Raising it from the default 100 to 400
  reproduced the curve within noise (1 stream: 4.8 / 9.4 / 18.4 / 32.6 / 28.4 / 24.3). It is a flush TRIGGER, not a
  ceiling — nothing in the crate blocks a sender — so it stays at the default.
- **Peak resident memory is 25–31 MiB** with eight volumes connected and all thirty-two streams running at once; eight
  idle volumes cost ~14.7 MiB over a 3.1 MiB baseline, so a mounted server is about 1.5 MiB. The 16 MiB channel window
  is an advertised credit rather than an allocation, so principle 5 is comfortable and the window stays where it is.
- **The 50 ms ceiling is the link, not the fixture.** The same curve without `netem` runs 180–280 MB/s at every depth,
  matching the crate evaluation's loopback column, so the ~38 MB/s is not the container's half-CPU cap.

### Measuring it again

`streams_bench.rs`, and ❗ **not** as `#[ignore]`d cells: the integration lane runs `--run-ignored only` over this whole
package, so an ignored measurement would gate CI on a throughput ratio taken under runner contention. They read
`CMDR_SFTP_BENCH=1` and cost nothing in every other run. `.config/nextest.toml` grants them a 600 s slow-timeout, since
the default 8 s cap kills a measurement mid-curve.

```sh
./apps/desktop/test/sftp-servers/start.sh bench
docker exec sftp-fixture-sftp-fixture-bench-1 tc qdisc replace dev eth0 root netem delay 50ms limit 50000
CMDR_SFTP_BENCH=1 cargo nextest run -p cmdr-sftp --release --no-capture streams_bench
```

Two things about the method that are worth more than they look:

- **Warm the connection first, or measure TCP's ramp instead of the window.** `bench_volume` reads and discards 32 MiB
  before anything is timed. Without it, identical runs spread two to one (14.8 to 30.8 MB/s) because a 50 ms link takes
  a few megabytes to open its congestion window.
- **Raise `netem`'s queue limit.** Its default `limit 1000` packets is about one bandwidth-delay product at 30 MB/s, so
  the measurement rides on the edge of packet loss.

⚠️ The absolute numbers are the shape of a curve, not truth: Docker plus `netem` carried ±30% run-to-run spread in the
crate evaluation. Which is why the one assertion in that file compares serial against windowed **in the same run on the
same server** and asks for 4× where the shape shows 7× (7.2×, 7.1×, 7.0× across three consecutive runs).

## The write window

An upload is the read window's mirror: `writes.rs` keeps `WRITE_WINDOW_DEPTH` positioned writes in flight, each naming
its own offset so N clones of one handle write different parts of the file at once.

What differs is the ceiling. Raising `russh`'s channel window is what lets read depth pay; it does **nothing** for an
upload, because the SERVER's window governs and OpenSSH fixes it at 2 MiB. Depth is the whole of the write side's
tuning.

- ❗ **The transport clone is taken up front and no lock is held across the upload.** `write_from_stream` clones the
  `Arc<SshConnection>` out from under a short read guard before a byte moves, then works on the clone; `File: Clone`
  shares the one remote handle through an `Arc`, so the window's depth costs no extra `SSH_FXP_OPEN` and no other
  operation on the channel waits behind the copy. (SMB's "never hold the session mutex across the upload" is the same
  rule in its own shape.)
- **Chunks are 255 KiB**, the read side's number and for the same reason. ⚠️ The engine's own negotiated `max_write_len`
  is behind its `__ci-tests` feature and can't be read, so a server with stingier limits splits each chunk internally
  instead: correct, and narrower in practice than the depth says.
- **A source's chunk size is its own business.** `take_chunk` coalesces whatever arrives (SMB pipelines ~512 KB, a local
  read hands over less) into pieces the server takes in one request.
- **Progress reports bytes that LANDED**, never bytes issued, or the bar would finish long before the file.
- ❗ **Cancellation arrives only as `Break` from the progress callback.** There is no token on this path, so a backend
  that never called back would be uncancelable.
- ❗ **The close is part of the write.** `File::close()` is awaited and its answer propagated; dropping the `File` sends
  the same `SSH_FXP_CLOSE` on a detached task and throws away the one report a server gives of bytes it accepted but
  could not commit (hazard 1's `File::close()` note).
- ❗ **Every error path takes the partial with it**, cancellation included. The staging layer removes the temp too, so
  this is the backend being tidy rather than the safety net — and a failure removing it never replaces the error that
  caused it.
- ❗ **`write_is_single_shot` keeps its `false` default**, so the transfer layer stages every write here on a
  `.cmdr-tmp-*` sibling and a partial never wears a real filename. ❌ Which is also why there is no "the create landed
  but the write didn't" classifier like `cmdr-smb/src/volume/streams.rs`'s — that one exists because SMB's compound path
  SKIPS staging.

### The depth, and the curve that set it

Measured 2026-08-22 against `sftp-fixture-bench` (Alpine + OpenSSH `sftp-server`, `netem delay 50ms`, release build, 32
MiB written per upload), aggregate MB/s over one SSH channel. Three runs, the spread across them in the ranges:

| depth | 1 upload      | 4 uploads     | requests in flight (1 / 4) |
| ----- | ------------- | ------------- | -------------------------- |
| 1     | 4.6           | 15.5-18.7     | 1 / 4                      |
| 2     | 4.1-9.1       | 14.0-31.4     | 2 / 8                      |
| 4     | 17.0-17.3     | 35.3-35.6     | 4 / 16                     |
| **8** | **28.9-29.4** | **35.2-35.6** | 8 / 32                     |
| 16    | 31.2-31.4     | 35.4-35.5     | 16 / 64                    |
| 32    | 31.1-31.6     | 34.9-35.6     | 32 / 128                   |

**Depth 8 is the knee, and the plan's starting number of 16 buys 7% for twice the memory.** One upload reaches 92% of
its own ceiling at 8; four uploads are already at the link's ceiling by 4 and stay there. The in-flight buffer is
`depth × 255 KiB` per upload, so 8 costs 2 MiB per upload against 16's 4 MiB — 64 MiB versus 128 MiB for eight volumes
each running four concurrent uploads. Principle 5 decides a tie the throughput doesn't.

⚠️ **The write curve does NOT fall off past its peak the way the read curve does**, and that asymmetry is the two
windows again: reads at depth 32 put ~32 MiB of outstanding data against a 16 MiB channel window and throttle each
other, while an upload's outstanding data is bounded by the server's own 2 MiB window whatever depth we ask for. So
deeper is merely useless here rather than harmful — which is exactly why the memory argument gets to decide.

The ratio gate reads 6.2-6.3× where it asks for 4× (three runs), against the read side's 7.0×.

❗ **Run the bench cells one at a time.** They share one server over one shaped link, so two at once measure each other:
the ratio cell alongside the depth curve read 4.3× where the same cell alone reads 6.2×. `.config/nextest.toml`'s
`sftp-bench` test group enforces it; ❌ don't remove it to speed a measurement up.

## The error policy

SFTP v3 has no `SSH_FX_FILE_ALREADY_EXISTS` (that arrived in v4), and OpenSSH folds `EEXIST`, `ENOTEMPTY`, `EISDIR`, and
most of the rest of errno into the one catch-all `SSH_FX_FAILURE`. Five shared conformance assertions and the
folder-merge walker branch on exact variants, and `error-string-match` forbids recovering one from the message. So the
variant has to be put back, and `errors::resolve_ambiguity` is where.

**Two inputs, because the code alone can't decide**: what the operation was TRYING to do (`Attempted`) and what the
server says is at the path afterwards (`WhatIsThere`, from one `symlink_metadata`).

| attempted                 | probe found           | answer                         |
| ------------------------- | --------------------- | ------------------------------ |
| any, code isn't `Failure` | not asked             | the code's own mapping         |
| `TakingAName`             | a file or a directory | `AlreadyExists(path)`          |
| `TakingAName`             | nothing               | unclassified `IoError`         |
| `RemovingANode`           | a directory           | `IoError` carrying `ENOTEMPTY` |
| `RemovingANode`           | nothing, or not a dir | unclassified `IoError`         |

Per operation, the primitive and the cell it lands in:

- **`create_file`** opens with `SSH_FXF_EXCL`, so the SERVER refuses the clobber. `TakingAName`.
- **`create_directory`** sends `SSH_FXP_MKDIR`, which refuses an occupied name on every server, extension or not.
  `TakingAName`. This is what lets `create_directory_errors_on_existing_dir` answer `true`.
- **`create_directory_all`** runs the leaf's mkdir first (one round trip when the parent is already there) and reads
  `AlreadyExists` back as `AlreadyExisted`. ❗ Only a `NotFound` earns the ancestor walk; anything else fails the same
  way at every level.
- **`delete`** sends `SSH_FXP_REMOVE`, then `SSH_FXP_RMDIR` if that refused, so a bulk delete of files spends one round
  trip each rather than a stat plus a remove. When both refuse it probes once: a directory means the rmdir's refusal
  describes the path (`RemovingANode`), anything else means the FILE delete's own refusal is the honest answer —
  otherwise a permission-denied file would be reported as the "not a directory" the rmdir complained about.
- **`rename`**'s destination claim is `TakingAName`; § "Renaming without clobbering" has the rest.

**Why `ENOTEMPTY` rather than a shrug.** OpenSSH answers `EACCES` and `EPERM` with `SSH_FX_PERMISSION_DENIED`, so a
permission refusal never arrives as the catch-all; a rmdir that failed on a path that is still a directory is a
directory that still holds something (`sftp-server.c`'s `errno_to_portable`, OpenSSH 9.8, read 2026-08-22). The NUMBER
is what the app renders "this folder still has something in it" from, and MTP and LocalPosix both report the host
platform's, so this one does too.

❗ **The probe classifies, ❌ never guards.** Asked BEFORE an operation, "is anything at this path" is a TOCTOU window,
and on a server with `posix-rename@openssh.com` what fits in that window is a silently overwritten file. Asked after, it
decides nothing that hasn't already happened, so it can only make a report more accurate — which is why a code the
protocol DOES distinguish is never re-read through it.

## Renaming without clobbering

`Volume::rename(from, to, force)` is two different operations, and neither can be written as the other.

**`force = true` uses `Fs::rename` directly**, which reaches for `posix-rename@openssh.com` when the server offers it.
Here that is exactly right: the extension is defined to replace the destination atomically, which is what gives a remote
archive edit its atomic swap (`write_operations/archive_remote_edit.rs::swap_into_place` gates that fast path on
`create_directory_errors_on_existing_dir()` plus a forced rename succeeding, and this backend answers both). A server
without the extension sends plain `SSH_FXP_RENAME`, which REFUSES an occupied destination, so that one gets the
destination cleared first — ❗ and only once the probe proves something is in it, because clearing on any failure is the
shape the app-side landing fix exists to stop.

**`force = false` must never touch `Fs::rename` on a server that has the extension.** `force = false` promises
`AlreadyExists` and an untouched destination, which is the exact opposite of what `posix-rename` does, and every caller
that hasn't asked the user yet relies on the promise. ❗ There is no way to send a plain `SSH_FXP_RENAME` through
`openssh-sftp-client` when the server advertises the extension: `Fs::rename` picks for you, and the lowlevel `WriteEnd`
is private (`lib.rs`'s `use openssh_sftp_client_lowlevel as lowlevel`, not `pub use`). So:

- **Server without the extension**: `Fs::rename` is already the plain request and refuses by itself. One round trip.
- **Server with it**: CLAIM the destination name first with a primitive the server refuses atomically, then let the
  rename land on a placeholder of our own. A file-shaped claim (`SSH_FXF_EXCL`) covers the overwhelming case — a staged
  write landing — in one extra round trip. A directory source can't be renamed onto a file (`ENOTDIR`), so a failed
  rename removes the placeholder, probes the SOURCE, and retries with a directory-shaped claim (`SSH_FXP_MKDIR`, which
  POSIX rename replaces when it's empty).

**Decision: the claim, ❌ not a pre-flight stat.** A stat guard is a TOCTOU window whose failure mode is that the user's
EXISTING file is destroyed. The claim's failure mode is a zero-byte file at a name that was proven FREE a moment
earlier, if the process dies between the two requests. It never destroys data the user already has, which is what
principle 1 is about; and ❗ the claim must never outlive the attempt that made it, or that zero-byte file is left
wearing the name the user chose — `a_forceless_rename_that_finds_the_name_free_leaves_nothing_extra` is the cell that
holds it to that.

**Cost**: two requests where a naive rename spends one, on every staged write that lands. ❗ Measured against the
alternative, that is the price of the promise rather than an oversight.

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

The engine's own `File::read` clamps `n`, may return **fewer** bytes than asked, and then advances the file offset by
**`n`, the requested length**. `read_all` loops doing `n -= bytes.len()`, so a short read re-reads from an offset that
already skipped the gap: a silent hole plus duplicated bytes.

❌ Never use `read_all` or `File`'s own offset for a byte path. Issue positioned reads and track offsets yourself.
`RemoteFile::read_at` seeks before every request for exactly this reason, and `File: Clone` shares the remote handle
through an `Arc`, so N clones each seeked to their own offset give depth N with no extra `SSH_FXP_OPEN`. ✅
`TokioCompatFile` is **not** affected: it advances by bytes consumed.

`sftp-fixture-shortreads` is the server that catches it, and
`a_short_reading_server_still_hands_the_file_back_byte_for_byte` is the cell. Measured against the naive shape
(2026-08-22): a `read_all` over that server's 4 KiB answers runs its offset off the end of a 4 MiB file within a 200 KiB
range read and fails with `UnexpectedEof`; on a file big enough to absorb the runaway it returns bytes from the wrong
places instead.

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
- **`supports_streaming` → true**, with `open_read_stream`, `open_read_stream_for_scan`, and `read_range` behind it (§
  "The read window"). The other read overrides deliberately keep their defaults, and that section says why.
- **`is_writable` → true**, because every mutation is implemented. The shared conformance assertion holds the
  declaration to what the server actually accepts.
- **`create_directory_all` is overridden** where SMB and MTP leave the default. The default spends one `exists()` round
  trip per ancestor before creating anything, which over a 50 ms link is the whole cost of a deep destination; the
  override tries the leaf first and walks only on a `NotFound`.
- **`create_directory_errors_on_existing_dir` → true**, from `SSH_FXP_MKDIR`'s own refusal. It is half the gate on a
  remote archive edit's atomic swap; § "Renaming without clobbering" is the other half.
- **`write_is_single_shot` keeps its `false` default**, so every write here stages (§ "The write window").
- **`notify_mutation` is overridden**, and ❗ it has to be: there is no watcher on this backend, so it is the only thing
  that keeps a destination pane honest after a copy. One call per changed DIRECTORY, never per entry.
  `create_directory_all` patches exactly the SHALLOWEST level it created: its parent is the only level that existed
  before, so it is the only listing a pane could be holding, and the levels under it are brand new. ⚠️ **Known cost, not
  yet paid down**: a `Created` patch stats the new entry, so a staged write spends one extra round trip on the temp and
  the landing rename spends another on the final name — about 100 ms per file at 50 ms RTT, which roughly triples a
  small-file copy. `cmdr-smb` has the same shape and it costs ~1 ms on a LAN. The fix is either building the temp's
  entry locally (nothing shows it — `file_system::staging::is_hidden_from_listings` filters staging temps out of every
  pane) or skipping the temp's patch entirely, and both need the app's listing-mutation contract read first.
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

- **Here**: the contract, the trust table, path translation, the auth ladder, the reading and writing surfaces, and the
  crate hazards. These are white-box tests — several build a volume with no session behind it.
- ❗ **A write cell picks the server that could let its bug through.** `conformance_test.rs` runs on
  `sftp-fixture-openssh` because it HAS `posix-rename@openssh.com`; the same cells against `sftp-fixture-noposixrename`
  pass while a clobbering rename ships. The byte-exactness cell that matters runs on `sftp-fixture-smalllimits`, because
  a stock server never short-writes and so never exposes a mis-advanced write offset (verified 2026-08-22 by breaking
  the offset on purpose: only the small-limits cell went red).
- ❗ **Every write cell works inside a `scratch_dir` of its own.** The whole binary shares one export and `nextest` runs
  its cells in parallel, so a fixed name has two of them deleting each other's files and reporting it as a backend bug.
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
