# Which Rust crate the SFTP backend is built on (2026-08-22)

Evaluation for the planned `cmdr-sftp` backend crate (a `Volume` implementation, following the nine-step recipe in
`crates/cmdr-fs/src/volume/host/DETAILS.md`). Every candidate was cloned and read; the throughput numbers at the bottom
were measured against a real OpenSSH server, not quoted from a README.

**All version, date, and activity facts here were checked against crates.io and the GitHub API on 2026-08-22.**

## The recommendation

**Build on `russh` (transport) + `openssh-sftp-client` (protocol engine), and write the read/write window yourself.**

Ranked, with the reasoning so you can argue with the reasoning rather than the conclusion:

1. **`russh` + `openssh-sftp-client`**: the best protocol engine in the set: modeled line-for-line on
   `openssh-portable/sftp-client.c`, the only one with a real test suite and CI against a live `sshd`, the only one with
   documented per-method cancel safety, and the only one carrying `posix-rename` (atomic overwrite, which safe-overwrite
   needs) and `copy-data` (server-side copy). Its transport is a plain `AsyncRead`/`AsyncWrite` pair, so it rides a
   `russh` channel; **that combination is proven here, not assumed** (benches C, D, F, G below ran over it). One real
   cost: non-UTF-8 filenames make a listing fail (§ "The filename problem").
2. **`russh` + `yazi-sftp`**: Yazi's fork of `russh-sftp`, and the only crate in the set that gets filenames right
   (bytes, not `String`). Pick this if the non-UTF-8 case outranks engine maturity, or take it as the base if Cmdr ever
   decides to own the protocol layer.
3. **`russh` + `russh-sftp`**: the popular default (62 reverse-dependencies, most blog examples). Weakest on the two
   axes Cmdr cares about most: it mangles non-UTF-8 filenames silently, and its high-level API can't pipeline reads, so
   you drop to `RawSftpSession` anyway. Zero unit tests, by the author's own README checklist.
4. **Own crate (`cmdr-sftp-proto`) on `russh`**: the `smb2` move. Highest ceiling, ~2.4k lines by Yazi's precedent, and
   you own filenames, the window, cancellation, and extension negotiation outright. Not the day-one choice; the sane
   fallback if a crate blocks you.
5. `ssh2` / `async-ssh2-lite`, 6. `openssh` (system `ssh` binary), 7. `libssh-rs`, 8. `remotefs-ssh`, 9. `thrussh`, 10.
   `puressh`, each disqualified for a specific structural reason, all listed below.

**It is close between 1 and 2, and the tiebreaker is which failure you'd rather ship.** `openssh-sftp-client` fails
loudly and never writes wrong bytes; `russh-sftp` silently substitutes U+FFFD into filenames, which means a folder copy
can write files under mangled names at the destination. Cmdr's first principle is protecting the user's data, so the
loud failure wins. `yazi-sftp` has neither problem but has no tests, no docs, and a date-shaped version (`26.5.6`) that
tracks a TUI app's release train rather than semver.

**What no crate gives you, so budget for it regardless:** a pipelined **read** window. Every candidate issues one
`SSH_FXP_READ` and awaits it before sending the next, which is the 4.2 MB/s line in the measurements; building the
window is worth **10×** (§ Measurements). It's ~150 lines, and each crate exposes the primitive for it
(`RawSftpSession::read`, a cloned `File`, or `Operator::read`). Writes are less dire but still need a decision:
`russh-sftp` keeps 8 acks in flight by default and hits 25 MB/s, while `openssh-sftp-client` needs configuring to pass
it (29.4 MB/s once you do).

**What the SFTP backend will own on top of whichever crate wins**, roughly in build order:

1. The read window (N outstanding `read(handle, offset, len)`, reassembled in order) behind `VolumeReadStream`, plus the
   matching write window, both sized from `settings().max_concurrent_operations("sftp")`.
2. `russh::client::Config::window_size` raised well above the 2 MiB default, or the window depth buys nothing.
3. Host-key TOFU: `check_server_key` → `known_hosts` lookup → an IPC prompt → `learn_known_hosts`.
4. Auth ladder: agent → key file (with passphrase prompt) → password → keyboard-interactive, each mapped to the
   `NeedsCredentials` connection state so the existing reconnect banner and sign-in flow are reused.
5. Extension probing at connect (`posix-rename`, `limits`, `fsync`, `statvfs`, `copy-data`) mapped onto
   `VolumeCapabilities`, and honest fallbacks where a server lacks them.
6. Short-read handling, byte-exact filenames, and per-directory incremental listing progress.

Sibling note: `docs/notes/ftp-crate-evaluation-2026-08-22.md` answers the same question for FTP and argues for SFTP
first. The non-UTF-8 filename trap it names in `suppaftp` is the same class of problem as § "The filename problem" here.
Worth solving once, in whatever shape the network backends end up sharing.

## What the backend has to do (the eight axes)

From `crates/cmdr-fs/src/volume/mod.rs` and the finished SMB backend in `crates/cmdr-smb/`:

- `open_read_stream` → a `VolumeReadStream` yielding chunks, with `bytes_read()` driving progress; drop = cancel (SMB's
  `SmbReadStream` sends a oneshot cancel from `Drop`).
- `write_from_stream` → pull a chunk, push it into the backend writer, call `on_progress`, honour `ControlFlow::Break`.
  Must never buffer the whole file.
- `open_read_stream_at_offset` / `read_range(offset, len)` → resumable reads and the file viewer.
- `list_directory(on_progress)` and `list_directory_with_cancel`.
- `max_concurrent_ops()` (4 by default, read per batch from `settings().max_concurrent_operations(BACKEND)`).
- `attempt_reconnect` / `reconnect_with_credentials` + connection-state events through the host seam.

## Candidate matrix

| Crate                        | Pure Rust              | Async                               | Streams               | Cancel mid-transfer                | Offset read/write | Pipelining                                     | Filenames            | posix-rename      | Tests           | LoC (code)                               | Bus factor | License             |
| ---------------------------- | ---------------------- | ----------------------------------- | --------------------- | ---------------------------------- | ----------------- | ---------------------------------------------- | -------------------- | ----------------- | --------------- | ---------------------------------------- | ---------- | ------------------- |
| `russh-sftp` 2.4.0           | yes¹                   | tokio                               | yes                   | drop, clean (verified)             | yes (raw)         | writes 8-deep by default, reads never          | `from_utf8_lossy` ⚠️ | no                | none            | 3 627                                    | 1          | Apache-2.0          |
| `openssh-sftp-client` 0.15.7 | yes¹                   | tokio                               | yes                   | designed for, documented, verified | yes               | engine holds ≤100 pending; caller must fill it | errors on non-UTF-8  | yes               | real suite + CI | 3 182 (+1 172 lowlevel, +1 142 protocol) | 1          | MIT                 |
| `yazi-sftp` 26.5.6           | yes¹                   | tokio                               | yes                   | drop, clean                        | yes               | caller-driven (`Receiver`)                     | bytes ✅             | yes               | none            | 2 390                                    | 2          | MIT                 |
| `ssh2` 0.9.6                 | no (libssh2 + OpenSSL) | blocking                            | `Read`/`Write`/`Seek` | not possible                       | yes               | 30 KB chunks, read-ahead                       | `PathBuf` (bytes)    | via `RenameFlags` | yes             | 2 811                                    | 2          | MIT/Apache-2.0      |
| `async-ssh2-lite` 0.5.0      | no                     | yes                                 | yes                   | poll-based                         | yes               | inherits libssh2                               | as `ssh2`            | as `ssh2`         | integration     | 1 894                                    | 1          | MIT/Apache-2.0      |
| `libssh-rs` 0.3.8            | no (**libssh, LGPL**)  | blocking                            | `Read`/`Write`/`Seek` | not possible                       | yes               | none                                           | `String`             | no                | some            | 2 098                                    | 1          | MIT wrapper, LGPL C |
| `remotefs-ssh` 0.8.5         | depends on backend     | **no** (owns a runtime, `block_on`) | `Read`/`Write`        | no                                 | limited           | no                                             | `String`             | no                | testcontainers  | 10 298                                   | 1          | MIT                 |
| `openssh` 0.11.6             | n/a (spawns `ssh`)     | tokio                               | via sftp-client       | process kill                       | n/a               | n/a                                            | n/a                  | n/a               | yes             | n/a                                      | 2          | MIT/Apache-2.0      |
| `thrussh` 0.48.0             | yes¹                   | tokio                               | no SFTP               | n/a                                | n/a               | n/a                                            | n/a                  | n/a               | n/a             | 8 830 (SSH only)                         | 1          | Apache-2.0          |
| `puressh` 0.1.3              | **yes, fully**         | both                                | yes                   | ?                                  | ?                 | ?                                              | ?                    | ?                 | claimed         | ~79 000                                  | 2          | MIT/Apache-2.0      |

¹ "Pure Rust" for anything on `russh` means pure-Rust SSH with a C/assembly crypto core: `russh` fails to compile unless
`ring` or `aws-lc-rs` is enabled (`src/lib.rs:90-93`, `compile_error!`). **This costs Cmdr nothing new**: `Cargo.lock`
already carries `aws-lc-rs 1.17.1`, `aws-lc-sys 0.42.0`, and `ring 0.17.14` through `rustls`, and CI already builds
them.

## Per-candidate detail

### `russh` (the transport, needed by candidates 1-4)

- **Age**: forked from `thrussh` in 2022-02; first crates.io release `0.34.0-beta.1` on 2022-03-13. Now at **0.63.0
  (2026-08-21)**, which is one day old, so the ≥3-day rule points at **0.62.7 (2026-08-17)**.
- **Stars / shape**: 1 834 stars, 283 forks, 100+ contributors (`warp-tech/russh`, run by Eugene of Warp/Tabby). Bus
  factor is better than any other candidate but still leans on one person: 88 of the most recent 200 commits are his,
  against 15 for the next human.
- **Activity**: 189 commits in 12 months, 136 in 6, 72 in 3; last push 2026-08-21. 63 open issues, most answered.
- **Churn is the real cost**: eight breaking minors in eight months (0.56 in 2025-12 → 0.63 in 2026-08). Keep the
  transport wiring in one small module so a bump touches one file.
- **Security**: one advisory in its history, RUSTSEC-2026-0154 (2026-05-15, unbounded 32-bit allocation in agent frame
  parsing, DoS), patched in ≥0.60.3. That it has a published advisory and a `SECURITY.md` is a positive signal.
- **Auth (axis 6)**: everything Cmdr needs, all on `client::Handle`: `authenticate_password`, `authenticate_publickey`,
  `authenticate_publickey_with` (custom `Signer`), `authenticate_keyboard_interactive_start` / `_respond`,
  `authenticate_openssh_cert`, `authenticate_none`, `authenticate_gssapi_with_mic`. Encrypted private keys:
  `russh::keys::decode_secret_key(secret, password)` and `load_secret_key(path, password)`. ssh-agent:
  `russh::keys::agent::client::AgentClient::connect_env()` / `connect_uds()` / `connect_pageant()` /
  `connect_named_pipe()`, so agent auth works on Windows too.
- **Host-key verification (axis 7)**: exactly the shape Cmdr needs.
  `client::Handler::check_server_key(&mut self, key: &ssh_key::PublicKey) -> impl Future<Output = Result<bool>>` is
  **async**, so the handler can await a real user prompt over IPC and answer TOFU. `russh::keys::known_hosts` supplies
  `check_known_hosts`, `known_host_keys`, and `learn_known_hosts` (plus `_path` variants) so the `~/.ssh/known_hosts`
  half is done for you. The default implementation rejects every key, which is the right default.
- **`~/.ssh/config`**: `russh-config` (567 lines) parses `Host`, `User`, `Port`, `IdentityFile`, `ProxyCommand`,
  `ProxyJump`. It does **not** handle `Match` directives (open issue, 2026-05-03). `ProxyJump` is implementable on
  `Handle::channel_open_direct_tcpip`.
- **Throughput levers**:
  `client::Config { window_size: 2097152, maximum_packet_size: 32768, channel_buffer_size: 100 }`. The 2 MiB channel
  window is OpenSSH's own default and it is **the binding ceiling on a high-latency link**; see the measurements, where
  raising it to 16 MiB is what let request depth matter at all.

### 1. `openssh-sftp-client` 0.15.7: recommended

- **Age**: first release 2021-12-30; on the 0.15 line since **2024-08-10**, so two years at its current API. Latest
  0.15.7 released **2026-04-28** (well past the ≥3-day rule).
- **Stars**: 62 (`openssh-rust/openssh-sftp-client`), and only 7 reverse-dependencies against `russh-sftp`'s 62.
  Popularity here measures the crate's niche, not its quality: its most prominent consumer is **Apache OpenDAL**
  (`opendal-service-sftp` depends on `openssh-sftp-client ^0.15.3`), which is a far stronger signal than the star count.
- **Contributor shape**: 8 contributors, 308 of the commits by Jiahao XU (NobodyXu). Bus factor 1. Non-bot commits in
  the last 12 months: 4 (2026-04-27, 2026-04-15, 2026-04-07, 2025-11-21) plus release automation. **Responsiveness is
  the redeeming number**: issue #153 was filed 2026-03-19 and answered 2026-03-20.
- **Size**: 3 182 code lines in the client, 1 172 in `openssh-sftp-client-lowlevel`, 1 142 in `openssh-sftp-protocol`,
  70 in `openssh-sftp-error`. Around 5.6k lines total if you ever had to own it.
- **Transport (axis 8)**:
  `Sftp::new<W: AsyncWrite + Send + 'static, R: AsyncRead + Send + 'static>(stdin: W, stdout: R, options: SftpOptions)`.
  The `openssh` crate is an **optional, off-by-default feature**, so the default build is a pure protocol engine over
  any byte stream. Feeding it `tokio::io::split(channel.into_stream())` from russh works (benches C, D, F, G).
- **Streaming + progress (axis 1)**: `File::read(&mut self, n: u32, buffer: BytesMut) -> Result<Option<BytesMut>>` gives
  the caller one request per call, so progress is whatever the caller counts. `TokioCompatFile` wraps a `File` into
  `AsyncRead + AsyncBufRead + AsyncWrite + AsyncSeek` when you want the tokio shape.
- **Cancellation (axis 2)**: designed for it, not incidental. Every public method carries a `# Cancel Safety` doc
  section; responses live in an arena (`awaitable_responses.rs`) whose slot survives a dropped future
  (`awaitables.rs:97-107`), so a cancelled read doesn't desynchronise the response stream. `ReadDir` even holds its own
  `WaitForCancellationFutureOwned` and aborts between batches. Dropping the last `File` reference sends `SSH_FXP_CLOSE`
  without awaiting (`handle.rs:22-45`), which is exactly `SmbReadStream`'s drop-cancel shape.
- **Random access (axis 4)**: `File` implements `AsyncSeek` (`Start`/`Current`; `End` is explicitly unsupported), and
  `File: Clone`, and a clone shares the server handle but keeps its own offset, which is the primitive a concurrent
  range reader wants.
- **Concurrency (axis 5)**: the engine is built for it. `SftpOptions::max_pending_requests` defaults to **100**, a flush
  task batches requests with a 0.5 ms `flush_interval`, and the write path is double-buffered. The lowlevel crate even
  names openssh-portable's own constants (`OPENSSH_PORTABLE_DEFAULT_NUM_REQUESTS: usize = 64`). What it does _not_ do is
  pipeline on your behalf: `File::read` and `File::write` each await their own response, so the naive call is one RTT
  per chunk in both directions (4.2 MB/s at 50 ms). `SftpOptions::tokio_compat_file_write_limit` (640 KiB default) is
  the one knob that buys pipelining without writing a window; 8 MiB took writes from 9.2 to 19.4 MB/s.
- **Extensions and quirks**: `limits`, `expand-path`, `fsync`, `hardlink`, `posix-rename`, `copy-data`, each with a
  `Sftp::support_*()` predicate. `Fs::rename` uses `posix-rename` when the server has it and falls back to plain
  `SSH_FXP_RENAME` otherwise (`src/fs/mod.rs:214-236`). `File::copy_to` / `copy_all_to` are server-side copy (OpenSSH ≥
  9.0), which would make same-server copies nearly free.
- **Licence**: MIT (the `openssh-sftp-*` family), already on Cmdr's `deny.toml` allow list.
- **Three real warts**, all small and all worth knowing before you commit:
  - **`Sftp::close()` never returns over a russh channel.** It awaits `read_task`, which only ends at EOF on the reader
    (`src/sftp.rs:328`). Under the `openssh` feature the child's stdio EOFs on drop; a russh channel does not until you
    close it. Hit during benchmarking and worked around by closing the channel (or just dropping) instead.
  - **An `unwrap` on a dropped receiver**: `tx.send(extensions).unwrap()` in `src/tasks.rs:213` panics in a spawned task
    if the `Sftp::new` future is dropped before the server hello arrives, which is to say if you cancel or time out a
    connection attempt, which Cmdr will. Open as issue #153 since 2026-03-19, unreproduced upstream. One-line fix,
    trivial to carry as a patch or upstream.

    **Fixed upstream and shipped**: openssh-rust/openssh-sftp-client#176 landed in 0.15.8 (2026-08-24), which Cmdr is
    on. The shape the crate grew to route around it is still in place; `crates/cmdr-sftp/DETAILS.md` § hazard 2 carries
    what it would take to unwind.

  - **`File::read` advances the offset by the _requested_ length, not the returned one** (`src/file/mod.rs:426`), and
    `read_all` inherits it. A server that legally short-reads would leave a hole in a sequential download. It cannot
    bite a reader that tracks its own offsets (which a windowed reader does anyway), and `TokioCompatFile` handles short
    reads correctly. Also note `Sftp::max_read_len()` / `max_write_len()` are gated behind the internal `__ci-tests`
    feature, so you can't ask the negotiated limit; pass a large `n` and read the returned length.

### 2. `yazi-sftp` 26.5.6: the filename-correct fork

- **What it is**: the Yazi file manager's fork of `russh-sftp`, co-authored by `russh-sftp`'s own author (AspectUnk) and
  Yazi's (sxyazi). Its README states the fork rationale: paths containing invalid UTF-8, `nlink`/user/group from
  `longname`, copy-on-write packets, precomputed packet lengths, no buffer cloning in `AsyncRead`/`AsyncWrite`.
- **Age / activity**: released with Yazi (41 586 stars, last push 2026-08-21), version `26.5.6` published 2026-05-05.
  Two authors on this crate. Date-shaped versions mean **semver tells you nothing** about a bump.
- **Size**: 2 390 code lines. Zero `cfg(test)` blocks.
- **Filenames**: `DirEntry::name() -> &[u8]`, `long_name() -> &[u8]`, paths as `typed_path::UnixPathBuf`. This is the
  only candidate that can round-trip a name that isn't valid UTF-8, which for a file manager is a correctness axis, not
  a nicety.
- **Concurrency**: `Operator::read(&self, handle, offset, len) -> Result<Receiver>` hands back the pending-response
  receiver **without awaiting**, which is the cleanest windowing primitive in the whole set. `File`'s `AsyncRead`, like
  everyone else's, is one request at a time (`src/fs/file.rs:63-88`), and its `AsyncWrite` keeps only one write in
  flight, worse than `russh-sftp`'s 8.
- **Extensions**: `rename_posix`, `hardlink`, `fsync`, `limits`, `statvfs`.
- **Coupling**: `Operator::make(stream: ChannelStream<Msg>)` is hard-wired to russh's channel type, so you can't unit
  test the protocol over an in-memory pipe, and a russh bump can break it independently of Yazi's own schedule.

### 3. `russh-sftp` 2.4.0: the popular one

- **Age**: first release 2022-12-09; 2.x since 2024-05-13. Latest **2.4.0 (2026-08-03)**, past the ≥3-day rule.
- **Stars / shape**: 115 stars, 21 contributors, but **166 of ~180 commits are by one person** (AspectUnk / "Roman").
  Bus factor 1. 30 commits in the last 12 months, 10 in the last 3, so it is alive.
- **Adoption**: 62 reverse-dependencies on crates.io, the most of any pure-Rust SFTP crate, and the path most Rust SSH
  GUIs took.
- **Size**: 3 627 code lines. **Zero unit tests**: the README's own checklist has "Unit tests" unticked, and
  `grep -c 'cfg(test)' src` returns 0. There is one criterion upload benchmark and two examples.
- **Architecture**: `RawSftpSession` demultiplexes responses by request id through a
  `DashMap<Option<u32>, oneshot::Sender<...>>` with two background tasks, so the wire genuinely supports interleaved
  requests. Everything above it is where the pipelining is lost.
- **Reads are one round trip per chunk**: `File::poll_read` creates one `session.read(handle, offset, len)` future,
  awaits it, then starts the next (`src/client/fs/file.rs:155-200`), and `len` is capped by the caller's buffer. This is
  open issue #70 ("Performance of download", 2025-03-15), where a user reports 6 MB/s against a 1 Gbps LAN, and the
  maintainer's own conclusion (2026-04-30) is that the fix attempt in #85 didn't resolve it.
- **Writes are pipelined 8 deep**: `poll_write` sends through `write_nowait` and queues the ack, blocking only when
  `write_acks.len() >= max_concurrent_writes` (default 8, `Config::max_concurrent_writes`). This landed in 2026-05 in
  response to the same issue.
- **The filename problem (see below)**: `try_get_string` in `src/buf.rs:22-26` is
  `Ok(String::from_utf8_lossy(&bytes).into())`, with the strict version commented out directly above it. Open issue #42
  ("Support non-utf8 file names") since 2024-07-11, 12 comments, and the maintainer has rejected both proposed fixes
  (`OsString` and `Vec<u8>`) on API-ergonomics and cross-platform-serialization grounds.
- **Other gaps**: no `posix-rename` (`rename()` is plain `SSH_FXP_RENAME`, which OpenSSH refuses when the destination
  exists, so safe-overwrite would need `RawSftpSession::extended()` by hand); `Features` is `pub(crate)` so the
  high-level session can't tell you which extensions the server advertised; `SftpSession::read_dir` reads the entire
  directory into a `Vec` before returning, so there's no incremental `on_progress`; `File::sync_all()` **returns `Ok`
  without doing anything** when the server lacks `fsync@openssh.com` (`src/client/fs/file.rs:88-96`), which is a silent
  lie to a durability check; the default per-request timeout is 10 s (`Config::request_timeout_secs`, settable).
- **`SftpSession` doesn't expose its `Arc<RawSftpSession>`**, so a windowed reader means building on `RawSftpSession`
  directly and reimplementing the ~290 lines of `session.rs` conveniences.

### 4. Writing `cmdr-sftp-proto` on `russh`

The honest fourth option, and the one that matches how `smb2` and `mtp-rs` came to exist.

- SFTP v3 is small: ~30 packet types, and `yazi-sftp` is a complete client in 2 390 lines. The hard parts (crypto, kex,
  auth, host keys, channels, rekeying) stay in `russh`.
- You would own the things every candidate gets wrong or leaves out anyway: byte-exact filenames, the read/write window,
  per-request cancellation and progress, extension negotiation, and the short-read rule.
- Cost is real: a few weeks including a conformance suite, against days for adopting a crate. Recommend it as the
  deliberate fallback, or as a later consolidation once the backend's needs are known from shipping.

### The disqualified, with the specific reason

- **`openssh` 0.11.6 + `openssh-sftp-client`**: the transport OpenDAL uses. Its own doc line one: _"Scriptable SSH
  through OpenSSH (**only works on unix**)"_. It spawns the system `ssh` binary and multiplexes over a `ControlMaster`
  socket. That kills Windows outright, and it fails **axis 7 structurally**: host-key prompts, passphrase prompts, and
  password auth all belong to the `ssh` process and its tty, so Cmdr can't intercept an unknown host key and turn it
  into a UI. In a GUI app you'd be reduced to `BatchMode` plus `askpass` shims.
- **`ssh2` 0.9.6 (libssh2)**: 565 stars, 77 contributors, but the API is **blocking**: `Sftp::open` returns a `File`
  implementing `Read`/`Write`/`Seek`. Under `spawn_blocking` a dropped future cannot stop an in-flight read, so
  "everything cancelable" fails at the first hostile mount. It is also the heaviest supply chain here: `libssh2-sys`
  builds a **git submodule pointing at `Manishearth/libssh2`** (a fork, not upstream) and links **OpenSSL** on unix
  (`openssl-sys`, or `vendored-openssl` to build OpenSSL from source), plus `libz`. That is a new CVE surface Cmdr would
  own, and cross-compiling it to Windows and Linux is a build-system project. On the plus side it is the only C option
  with genuine pipelining: libssh2 does optimistic read-ahead (`src/sftp.c`, "we send off as many as possible … this is
  the key to fast reads"), though each request is capped at **30 000 bytes** (`MAX_SFTP_READ_SIZE` /
  `MAX_SFTP_OUTGOING_SIZE` in `src/sftp.h`) against OpenSSH's 255 KiB.
- **`async-ssh2-lite` 0.5.0**: a 1 894-line async wrapper over `ssh2` in non-blocking mode, and the only way to use
  libssh2 without `spawn_blocking`. **Last commit 2024-07-21**, last release the same day: two years dormant, with a bus
  factor of one. Adding it as a direct workspace dependency would put it in scope for `cargo deny`'s
  `unmaintained = "workspace"` rule the day RustSec files it.
- **`libssh-rs` 0.3.8**: **licence-disqualified.** The wrapper is MIT but `libssh-rs-sys` vendors
  `gitlab.com/libssh/libssh-mirror`, and **libssh is LGPL-2.1**. Statically linking it into a closed-source BSL binary
  triggers the LGPL's relinking obligations, and `deny.toml`'s allow list has no LGPL entry. Worse, `cargo deny` would
  **not** catch it: the sys crate declares `license = "MIT"`, so the C code's licence never reaches the check. The API
  is also blocking (`SftpFile: Read + Write + Seek`), and activity is 5 commits in 12 months.
- **`remotefs-ssh` 0.8.5**: **API-shape disqualified**, and it's the clearest example of the mismatch the question asked
  about. Its `RemoteFs` trait is synchronous (`fn open(&mut self, path) -> RemoteResult<ReadStream>`), and the `russh`
  backend owns a `tokio::runtime::Runtime` and calls `block_on` in ~20 places (`src/ssh/backend/russh.rs:91,108,145,…`).
  Calling `Runtime::block_on` from inside Cmdr's runtime panics, so it would have to live behind `spawn_blocking` with
  no cancellation, no progress callback, and a second runtime. It also has no streaming progress hook and 10 298 lines
  you'd inherit for the privilege.
- **`thrussh` 0.48.0, alive but not a candidate.** The "is it dead?" answer is no: Pierre-Étienne Meunier still ships it
  (0.48.0 on 2026-08-21, edition 2024) as Pijul's in-house SSH library at `nest.pijul.com/pijul/ssh`. But it has no SFTP
  client, no GitHub issue tracker or stars, 6 305 recent downloads against `russh`'s 2.47M, and no crate ecosystem above
  it. `russh` is its fork and inherited the community. (Note that `Eugeny/thrussh` on GitHub now redirects to
  `warp-tech/russh`, which makes the two easy to confuse.)
- **`puressh` 0.1.3**: the only _fully_ pure-Rust option: no C, no FFI, SFTP included, sans-I/O core with blocking,
  async, tokio, and mio frontends, MIT/Apache-2.0. And it's disqualified on age: first release **2026-05-27**, 1 star, 2
  contributors, and every cryptographic primitive comes from `purecrypto`, first published **2026-05-25** and unaudited.
  Shipping user data over a three-month-old unaudited crypto stack isn't a trade worth making. Worth re-checking in a
  year.
- **`makiko` 0.2.5, `sunset`, `ssh-rs`, `async-ssh2-russh`, `sftp` (jelmer)**: checked and set aside. `makiko` (pure
  Rust, 60 stars) has no SFTP and hasn't been touched since 2025-03-29. `sunset` targets embedded/no_std. `ssh-rs` last
  released 2023-12. `async-ssh2-russh` is a thin convenience wrapper over russh. `sftp` (jelmer/sftp-rs, 3 110 lines,
  Apache-2.0, 8 stars) is the dark horse: it speaks v3-v6, exposes `pread`/`pwrite`/`block`/`unblock` directly, and has
  both russh and ssh2 backends, but it's `String`-pathed, two contributors, and its `default = ["bin"]` feature drags in
  `rustyline`.

## The filename problem

SFTP v3 filenames are **bytes** with no declared encoding (the v3 draft leaves charset unspecified; v6 added one and no
common server implements it). A Linux or BSD server can hand back any byte sequence. This is a new class of problem for
Cmdr, since SMB is UTF-16 on the wire, so `smb2` never had to face it.

- `russh-sftp`: `String::from_utf8_lossy` → **silent corruption**. The pane shows a name containing U+FFFD, and every
  operation Cmdr performs on that name addresses a path that doesn't exist. A folder copy would write the mangled name
  at the destination.
- `openssh-sftp-client`: filenames deserialize into `Box<Path>` through `ssh_format`, whose `deserialize_str` does
  `str::from_utf8(bytes)?` (`ssh_format-0.14.1/src/de.rs:221-228`) → **the whole `readdir` errors**. Loud, no data
  written, one unlistable directory.
- `yazi-sftp`: bytes end to end. Correct.

**If you take candidate 1 and want this fixed**, the patch is contained: `[patch.crates-io]` a vendored
`openssh-sftp-protocol` (1 142 lines) plus `ssh_format` (untouched since 2022-10-18, so nothing to rebase against),
change `NameEntry::filename` to a byte-backed type, and leave the two crates above them on crates.io. `deny.toml`'s
`[sources] unknown-git = "deny"` means the fork has to be a path dependency under `crates/` (or a crates.io publish),
not a `git =` dependency, and that's the same rule any fork in this list would face.

## Measurements

Method: a purpose-built bench (`sftp-bench`, described under "Reproducing the benchmark" below) against Alpine 3.21 +
OpenSSH `sftp-server` in Docker, 128 MiB of `/dev/urandom`, `--cap-add=NET_ADMIN` with
`tc qdisc add dev eth0 root netem delay 50ms` for the latency runs. Release build, macOS host, one run per line.

**Treat the loopback column as a correctness signal and the 50 ms column as the shape of the curve, not as absolute
numbers**, the same caution `docs/notes/transfer-concurrency-window-bench-2026-08-02.md` records for Docker SMB.

Read, 128 MiB:

| Path                                                      | loopback     | 50 ms RTT, 2 MiB channel window | 50 ms RTT, 16 MiB window |
| --------------------------------------------------------- | ------------ | ------------------------------- | ------------------------ |
| A `russh-sftp` `File`, sequential `AsyncRead`, 256 KiB    | 208-292 MB/s | **4.19 MB/s**                   | n/a                      |
| C `openssh-sftp-client` `File::read`, sequential, 255 KiB | 212-281 MB/s | **4.17 MB/s**                   | n/a                      |
| B `russh-sftp` `RawSftpSession`, window 8                 | 260 MB/s     | 14.1 MB/s                       | 19.6 MB/s                |
| B window 16                                               | 257 MB/s     | 15.1-16.1 MB/s                  | 39.9 MB/s                |
| B window 32                                               | n/a          | n/a                             | **41.1 MB/s**            |
| D `openssh-sftp-client` cloned `File`, window 8           | 260 MB/s     | 17.2-17.9 MB/s                  | 27.5 MB/s                |
| D window 16                                               | 441 MB/s     | 14.2 MB/s                       | 21.8 MB/s                |
| D window 32                                               | n/a          | n/a                             | **42.4 MB/s**            |

What the numbers say, in order of how much they should change the design:

1. **Sequential SFTP is one round trip per chunk, and that is the whole story on a WAN link.** 255 KiB / 50 ms ≈ 5 MB/s
   theoretical, 4.2 MB/s measured, and both crates land on the same number because they have the same shape. A naive
   backend would show a QNAP over Tailscale crawling at ~4 MB/s while `scp` does 40.
2. **A request window is worth 10×**: 4.2 → 41-42 MB/s at depth 32.
3. **The SSH channel window gates the request window.** At russh's default 2 MiB, depth 8 and depth 16 measure the same
   (14-18 MB/s) because 8 × 255 KiB already fills the channel. Raising `client::Config::window_size` to 16 MiB is what
   makes depth pay: 41-42 MB/s at depth 32. **Tune both or neither.** This finding is crate-independent: it applies to
   any russh-based stack, and it's the single most useful thing in this document.
4. **The two crates are within noise of each other on reads once you drive them yourself.** Run-to-run spread was ±30%
   on individual lines (docker + netem), so read no ranking into B-vs-D at a given depth.

Write, 128 MiB, 50 ms RTT:

- **E `russh-sftp` `File::write_all`** (8 acks in flight, the default): **24.2-25.2 MB/s**
- **F `openssh-sftp-client` `File::write_all`** (one ack per request): 4.1-4.2 MB/s
- **G `openssh-sftp-client` `TokioCompatFile`, default 640 KiB buffer**: 8.8-9.2 MB/s
- **G same, `tokio_compat_file_write_limit(8 MiB)`**: 19.4 MB/s
- **H `openssh-sftp-client` cloned `File`s, window 8**: 17.8 MB/s
- **H same, window 16**: **29.4 MB/s**

Two more findings from those:

- **`russh-sftp` is the only crate that pipelines anything out of the box, and it's writes**: 25 MB/s against
  `openssh-sftp-client`'s 4.2 MB/s on the equivalent naive call. Configure `openssh-sftp-client` (an 8 MiB
  `tokio_compat_file_write_limit`, or a 16-deep window over cloned `File`s) and it passes it at 29.4 MB/s. Neither
  default is what you'd ship; both crates need the caller to say what it wants.
- **The channel-window lever does nothing for uploads** (24.2 vs 25.2, 8.8 vs 9.2 at 2 MiB vs 16 MiB), and correctly so:
  `russh::client::Config::window_size` is the window Cmdr advertises for data it _receives_. The upload direction is
  gated by the server's window, which OpenSSH fixes at 2 MiB.

**Cancellation, verified rather than reasoned about** (same harness, `CANCEL=1`): dropping a `russh-sftp` `File`
mid-read, and cancelling an `openssh-sftp-client` read _future_ after 1 ms and then dropping the `File`, both leave the
session fully usable: a following `metadata` and a fresh 255 KiB read on the same session both succeed. The
`openssh-sftp-client` case is the harder one (the future is dropped while its response is in flight) and it is what the
response arena is for.

**Not measured, and worth measuring before shipping**: real-hardware numbers against David's QNAP over Tailscale, and
the concurrency curve for _several files at once_ (Cmdr's `max_concurrent_ops` = 4) rather than one file split N ways.
`docs/notes/transfer-subtree-concurrency-bench-2026-08-13.md` found the useful width for SMB was 4-8 on real hardware
against a much higher number on loopback; expect the same discount here.

## Answers to the three side questions

**How hard is it to patch or fork in six months?** All three pure-Rust candidates are in the 2.4k-3.6k line range, which
is small enough to own. `openssh-sftp-client` is the most work to fork (three crates: client, lowlevel, protocol) but
also the least likely to need it, since the two known defects are one-liners. `russh-sftp` and `yazi-sftp` are single
crates. The binding constraint isn't size, it's `deny.toml`: `unknown-git = "deny"` means a fork lands as a vendored
path dependency under `crates/` (the `fsevent-stream` precedent) or as a crates.io publish. Upstream responsiveness:
NobodyXu answers in a day but merges rarely; AspectUnk ships regularly but has already refused the filename fix twice.

**Does any candidate handle server quirks, or does Cmdr own that?** Cmdr owns it, with one exception. Every candidate
speaks SFTP v3 only, which is what OpenSSH and essentially every NAS speaks. `openssh-sftp-client` is the only one that
both negotiates `limits@openssh.com` _and_ exposes per-extension `support_*` predicates plus a real fallback
(`Fs::rename` → `posix-rename` or plain rename), which covers the common "proprietary firmware lacks the OpenSSH
extensions" case. `russh-sftp` negotiates limits but hides the extension list and silently no-ops `fsync`. Nothing in
the set handles short reads, servers that lie about limits, `readdir` EOF quirks, or dialect differences beyond v3.
That's Cmdr's, whichever crate wins.

**Is any candidate's API shape a bad fit for `Volume` despite a good feature list?** Yes, two:

- **`remotefs-ssh`**: sync trait plus its own runtime and `block_on`; detailed above. Its feature list (SFTP, SCP, three
  backends, ssh_config support) looks like the best fit in the set and it is the worst.
- **`russh-sftp`'s high-level `SftpSession`**: every individual feature is present, but the three things `Volume`
  actually asks for don't fit: no incremental `read_dir` for `list_directory(on_progress)`, no way to reach the raw
  session for a read window, and no extension visibility for the capability flags. You end up using `RawSftpSession`, at
  which point you've adopted the crate for its serializer.

## Version and freshness check (crates.io, 2026-08-22)

- `russh` **0.63.0** released 2026-08-21, **1 day old, fails the ≥3-day rule**. Use **0.62.7** (2026-08-17, 5 days).
- `russh-sftp` **2.4.0** (2026-08-03): fine. Not version-coupled to `russh`; it takes any `AsyncRead + AsyncWrite`, and
  only its example pins a russh version.
- `openssh-sftp-client` **0.15.7** (2026-04-28): fine. Pulls `openssh-sftp-client-lowlevel` 0.7.2,
  `openssh-sftp-protocol` 0.24.2, `openssh-sftp-error` 0.5.1, `ssh_format` 0.14.1.
- `yazi-sftp` **26.5.6** (2026-05-05): fine.
- `ssh2` **0.9.6** (2026-06-30), `libssh-rs` **0.3.8** (2026-07-05), `remotefs-ssh` **0.8.5** (2026-06-08),
  `async-ssh2-lite` **0.5.0** (2024-07-21), `thrussh` **0.48.0** (2026-08-21), `puressh` **0.1.3** (2026-07-09).

Licences: `russh` and `russh-sftp` are Apache-2.0; `openssh-sftp-client`, `yazi-sftp`, and `libssh-rs` are MIT; `ssh2`
and `openssh` are MIT/Apache-2.0. All except libssh's LGPL C code are already on `deny.toml`'s allow list and are
compatible with shipping a BSL binary, with the usual attribution in `THIRD-PARTY-NOTICES.md`.

## Reproducing the benchmark

The server, as a `Dockerfile.sshd`:

```dockerfile
FROM alpine:3.21
RUN apk add --no-cache openssh iproute2
RUN ssh-keygen -A
RUN echo 'root:cmdrtest' | chpasswd
RUN sed -i 's/^#\?PermitRootLogin.*/PermitRootLogin yes/' /etc/ssh/sshd_config
RUN sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config
RUN mkdir -p /data && dd if=/dev/urandom of=/data/test_128m.bin bs=1M count=128
CMD ["/usr/sbin/sshd", "-D", "-e"]
```

```sh
docker build -f Dockerfile.sshd -t cmdr-sftp-bench .
docker run -d --name cmdr-sftp --cap-add=NET_ADMIN -p 127.0.0.1:12222:22 cmdr-sftp-bench
docker exec cmdr-sftp tc qdisc add dev eth0 root netem delay 50ms
SSH_WINDOW=16777216 cargo run --release -- "50 ms RTT, 16 MiB window"
```

The bench binary connects with `russh` (password auth, `check_server_key` → `Ok(true)`), opens the `sftp` subsystem, and
runs the same 128 MiB read four ways: `russh-sftp`'s `File` as `AsyncRead`; `openssh-sftp-client`'s `File::read`;
`RawSftpSession::read(handle, offset, len)` fanned out over a `FuturesOrdered` of depth N; and cloned
`openssh-sftp-client` `File`s seeked to their own offsets, same depth. `SSH_WINDOW` sets
`russh::client::Config::window_size`.
