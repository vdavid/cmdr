# FTP/FTPS client crates for Rust, evaluated for a Cmdr `Volume` backend

Researched 2026-08-22 against the crates.io registry, the upstream git histories, and the actual crate sources (cloned
and read, not judged from READMEs). Written to answer one question: if Cmdr builds an FTP backend after SFTP, what would
it rest on, and is the protocol worth the milestone at all?

## The short answer

- **There is exactly one living FTP client crate in Rust: `suppaftp` 10.0.2.** Everything else is dead (last release
  2018–2022), a server rather than a client, or a sync-only wrapper around `suppaftp` itself. This is not a "pick the
  best of several" decision; it's a "is the one option good enough" decision.
- **`suppaftp` is good enough.** It streams both directions over `AsyncRead`/`AsyncWrite`, exposes `REST` for resume,
  handles the `ABOR` reply dance correctly, parses MLSD as well as two `LIST` dialects, does explicit and implicit FTPS
  on rustls, defaults to passive, and rides tokio natively. The maintainer closes security reports within hours. It is
  the healthiest single-maintainer crate I've read in a while.
- **The protocol is the problem, not the crate.** FTP can't answer several questions the `Volume` trait asks (change
  notification, free space, precise mtimes, a cheap positioned read), and RFC 959's default transfer type silently
  corrupts files. Every one of those becomes Cmdr's problem, permanently.
- **Recommendation: don't build it next.** Ship SFTP, then WebDAV or S3. Park FTP behind a request counter from beta
  users. If the counter ever justifies it, `suppaftp` makes the build roughly two weeks, and that reversibility is what
  makes parking it cheap. Full reasoning in § "Is FTP worth doing at all".

## The candidate field

| Crate                      | Latest | Released   | Stars | Async                    | Verdict                                             |
| -------------------------- | ------ | ---------- | ----- | ------------------------ | --------------------------------------------------- |
| `suppaftp`                 | 10.0.2 | 2026-08-18 | 190   | tokio and smol, natively | The only viable option                              |
| `ftp` (rust-ftp)           | 3.0.1  | 2018-04-15 | 195   | sync only                | Dead. Its git history IS `suppaftp`'s               |
| `async_ftp`                | 6.0.0  | 2021-11-04 | 26    | async-std                | Dead. Fork of `rust-ftp`, last push 2023            |
| `remotefs-ftp`             | 0.4.0  | 2026-01-18 | 4     | sync only                | Maintained, but sync-only wrapper over `suppaftp` 8 |
| `ftp4`                     | 4.0.2  | 2021-01-09 | n/a   | sync                     | Yanked. The author's own predecessor to `suppaftp`  |
| `ftp-rs`                   | 0.2.0  | 2022-04-26 | n/a   | sync                     | Abandoned after two days of releases                |
| `mcai_ftp`                 | 3.0.1  | 2022-03-30 | n/a   | sync                     | Republished `rust-ftp` snapshot                     |
| `opendal` (`services-ftp`) | 0.58.2 | 2026-08-20 | n/a   | smol inside tokio        | Wraps `suppaftp` 8; see § "The other candidates"    |

Download counts as of 2026-08-22: `suppaftp` 792,207 recent against `ftp`'s 21,432 and `async_ftp`'s 5,464. The
long-tail crates are all under 300.

## `suppaftp` in depth

- **Repository**: `github.com/veeso/suppaftp`. **Licence**: `MIT OR Apache-2.0`, permissive, attribution only, no
  copyleft, so it's compatible with Cmdr's BSL distribution and with vendoring into `crates/` if it ever came to that.
- **Age**: the crate was first published 2021-08-22, but the git history reaches back to Matt McCoy's initial commit on
  2014-11-21. `suppaftp` is the continuation of `rust-ftp` with the full history carried over, which is why both repos
  show ~190 stars for what is really one lineage. Five years at the current name, twelve years of accumulated
  server-compatibility fixes.
- **Size**: 11,137 lines across `crates/suppaftp/src`. The part Cmdr would compile is much smaller: `sync_ftp.rs`
  (2,127), `smol_ftp.rs` (2,249), and the `native-tls` glue are all feature-gated off. The tokio-plus-rustls path is
  roughly **2,600 lines of non-test code** (`async_ftp/tokio_ftp.rs` is 1,190 before its `#[cfg(test)]` block, plus
  `list.rs`, `command.rs`, `status.rs`, `types.rs`, and two small TLS files).
- **Tests**: 246 test attributes, ~66 of them tokio tests, many of which run against a real `pure-ftpd` in a
  testcontainer (`src/test_container.rs` boots `delfer/alpine-ftp-server`). CI runs a clippy matrix across every feature
  combination on Linux, Windows, and macOS, with `dprint` formatting, `cargo-deny`, and `zizmor` supply-chain linting on
  pinned action SHAs. That's better hygiene than most crates this size.

### Contributor shape and bus factor

- **Bus factor is one.** Christian Visintin (`veeso`) authored 59 of the 66 commits in the last twelve months and 85 of
  96 over 24 months. Outside contributors land one or two commits each.
- The mitigating facts: he's been at it since 2021, he maintains a whole ecosystem around it (`termscp`, `remotefs`,
  `tui-realm`), and he is demonstrably present. Closed-issue timings from the last year:
  - #171, a CRLF command-injection report: opened 2026-08-18T15:25Z, closed 2026-08-18T16:03Z. **38 minutes.**
  - #173, the follow-up injection report: opened 03:02Z, closed 09:47Z the same day.
  - #155, the `data_connection_open` wedge bug: opened and fixed within two hours.
  - #157, #160, #131, #128: feature requests, all closed inside 24 hours.
- **Open issues: eight, and zero open PRs.** Of the eight, six are `[QUESTION]` threads from 2023–2025, so the real
  defect backlog is two items.
- **Release cadence**: 20 tags in the last twelve months. Two major versions in nine days during 2026-06 (9.0.0 on
  06-20, 10.0.0 on 06-29). Both breaks were narrow and mechanical: 9.0.0 swapped the async-std runtime for smol after
  RUSTSEC-2025-0052, and 10.0.0 changed `tcp_stream()` from returning `TcpStream` to `FtpResult<TcpStream>`. Expect a
  major bump every few months; expect each migration to be a half-hour, not a rewrite.
- **AI policy**: `AI_POLICY.md` requires disclosure of AI involvement through the PR template and forbids relaying
  messages between a maintainer and an agent. Relevant if Cmdr ever upstreams a patch: David would need to author and
  understand it himself, which is a change from how the rest of this repo gets written.

### Axis 1: streaming reads and writes with progress

**Clean pass.** Nothing here drains a file into a `Vec<u8>`.

- `retr_as_stream(file_name) -> FtpResult<DataStream<T>>` at `crates/suppaftp/src/async_ftp/tokio_ftp.rs:458`. The
  returned `DataStream<T>` implements `tokio::io::AsyncRead`, and it's owned, so it outlives the `&mut self` borrow on
  the control connection. That's exactly the shape `VolumeReadStream::next_chunk` wants.
- `put_with_stream(filename) -> FtpResult<DataStream<T>>` at `:527` is the write counterpart, an `AsyncWrite`.
  `append_with_stream` at `:562` issues `APPE` instead of `STOR`.
- Progress is the caller's to count, which is what Cmdr wants anyway: the transfer layer already owns the
  `on_progress(bytes_written, total_size)` contract, and the byte count comes from the read loop, not the crate.
- The convenience wrappers `retr` (`:436`), `put_file` (`:509`), and `append_file` (`:577`) exist but are not what a
  file manager should use: `retr` takes a closure that must hand the stream back, and `put_file` does a blind
  `tokio::io::copy` with no progress hook.
- **Gotcha, and it is the important one on this page:** `login()` at `:298` does **not** set the transfer type, and
  neither does `connect()`. RFC 959's default is ASCII, so an unmodified session line-ending-translates every byte it
  moves. Open issue #79 is a user who lost `\n` bytes in an uploaded log file this exact way. Cmdr would have to call
  `transfer_type(FileType::Binary)` immediately after login on every connection, including every pooled one, and a test
  would have to pin it. This is a silent data-corruption default in a protocol Cmdr would be moving people's files over.

### Axis 2: cancellation mid-transfer

**Pass, with a contract Cmdr must honor exactly.**

- `abort(data_stream)` at `:591` does the full dance: send `ABOR`, drop the data stream, read the reply, and if the
  server answered 426 `TransferAborted`, read the follow-up 226 as well. That second read is what keeps the control
  connection from desyncing, and getting it right is the part most hand-rolled clients miss.
- **The wedge hazard:** the stream carries a private `data_connection_open: bool` flag. Only `finalize_retr_stream`,
  `finalize_put_stream`, `abort`, and `close_data_connection` clear it. Drop a `DataStream` without calling one of them
  and the flag stays `true` forever, so `guard_multiple_data_connections` (`:1181`) fails every later data command with
  `FtpError::DataConnectionAlreadyOpen`. There is no public reset. A cancelled Cmdr transfer that skips `abort()`
  silently retires the connection.
- **Related trap:** calling `finalize_retr_stream` after a _partial_ read is wrong. The server answers 426, not 226,
  `read_response_in` rejects it, and the trailing 226 stays unread on the socket. `abort()` is the only correct cancel
  path, and it must be reached even on a panic or a dropped future.
- Good news: `VolumeReadStream::cancel_and_release` already exists for precisely this
  (`crates/cmdr-fs/src/volume/mod.rs:59`), and its doc comment says it's a no-op for every current backend and stays as
  "a trait hook for a hypothetical future backend whose stream genuinely holds a resource across chunks". An FTP backend
  is that hypothetical backend. It would be the first real caller, which means the copy path's cancel arm would get
  exercised for the first time.
- **What `suppaftp` does not do:** `ABOR` is written inline on the control channel rather than as a Telnet IP + SYNCH
  urgent sequence. RFC 959 asks for the urgent form. In practice most servers notice the closed data connection anyway,
  and the fixture tests pass against `pure-ftpd`, but a server that only reads its control socket between transfers
  would leave the abort unacknowledged until the data connection closes. Budget a timeout around every `abort()`.

### Axis 3: resume via `REST`

**Pass.**

- `resume_transfer(&mut self, offset: usize) -> FtpResult<()>` at `crates/suppaftp/src/async_ftp/tokio_ftp.rs:618` sends
  `REST <offset>` and requires a 350 `RequestFilePending` reply. The doc comment correctly notes that `REST` does not
  itself start a transfer and that the next `RETR`/`STOR` picks it up, and that `REST 0` cancels a pending restart.
- It works in both directions, so a resumed upload is `REST` plus `STOR` rather than `APPE`, which matters because
  `APPE` can't seek.
- The `usize` rather than `u64` in the signature is a wart on 32-bit targets. Cmdr is macOS 64-bit only, so it doesn't
  bite here.
- Caveat that belongs to the protocol: `REST` is only meaningful in binary mode, and some servers advertise
  `REST STREAM` in `FEAT` while rejecting non-zero offsets on `STOR`. Cmdr would have to probe `FEAT` and degrade to
  restart-from-zero rather than trusting the advertisement.

### Axis 4: listing, and the dialect problem

**Partial pass. This is the axis where Cmdr inherits the most work.**

What the crate gives:

- `mlsd(pathname) -> FtpResult<Vec<String>>` at `:659`, `list(...)` at `:629`, and `nlst(...)` at `:645`. All three
  return **raw lines**, unparsed, fully buffered in memory.
- A separate `ListParser` in `crates/suppaftp/src/list.rs` with three parsers: `parse_mlsd`/`parse_mlst` (`:89`, `:101`,
  both delegating to `parse_mlsx` at `:113`), `parse_posix` (`:201`), and `parse_dos` (`:320`).
- `feat()` at `:742` returns a `Features` map, so Cmdr can ask whether the server advertises `MLSD` before choosing.
- Symlinks are handled: `get_name_and_link` at `:379` splits on `" -> "` and `parse_posix` folds the target into
  `FileType::Symlink`.

What it does not give, and Cmdr therefore owns:

- **Dialect detection and dispatch.** There is no auto-detecting entry point. The module header is candid about it:
  "there's no specification regarding the LIST command output, so it basically depends on the implementation of the
  remote FTP server". Cmdr writes the `FEAT` probe, the MLSD-first policy, the `LIST` fallback, and the per-directory
  posix-versus-dos verdict.
- **Every dialect beyond two.** `POSIX_LS_RE` and `DOS_LS_RE` are single strict regexes. EPLF, VMS, MVS, NetWare, and
  OS/400 all miss, as do `ls -l` variants with a missing group column or a locale-translated month name. `parse_posix`
  returns `ParseError::SyntaxError` on anything it doesn't recognize, which in a file manager renders as an empty
  directory that isn't empty. That is the single worst failure mode a pane can have, and it's the exact historical
  failure mode of FTP clients.
- **Modification times.** `parse_lstime` (`:403`) reconstructs the year for `ls -l` lines that omit it and drops seconds
  entirely, and the timestamp carries no timezone: it is the server's local wall clock. Cmdr's file index, copy conflict
  detection, and sync-status all compare mtimes. Getting a real timestamp means one `MDTM` round trip per file
  (`mdtm(...)` at `:692` returns a `NaiveDateTime`), which is fine for a folder of 50 and ruinous for a folder of
  50,000. MLSD's `modify=` field is exact and UTC, so servers that support it sidestep this and servers that don't
  can't.
- **Non-UTF-8 filenames, unfixably.** `get_lines_from_stream` (`:857`) builds each line with `String::from_utf8_lossy`.
  A Latin-1 filename on an old server comes back with replacement characters, which means Cmdr can display it and can
  never open, copy, rename, or delete it, because the bytes it would send back aren't the bytes the server holds. There
  _is_ a test, `test_should_list_files_with_non_utf8_names` at `:1510`, but it only asserts the listing doesn't error.
  Fixing this requires patching the crate's listing API from `Vec<String>` to `Vec<Vec<u8>>`, which is a breaking change
  upstream would have to accept.
- **`OPTS UTF8 ON`.** Not sent automatically. `opts("UTF8", Some("ON"))` exists (`:770`); calling it is Cmdr's job.

### Axis 5: FTPS

**Pass on rustls, fail on native-tls, and the difference is exactly the session-reuse requirement.**

- **Explicit (`AUTH TLS` on port 21):** `into_secure(tls_connector, domain)` at `:142`.
- **Implicit (port 990):** `connect_secure_implicit(addr, tls_connector, domain)` at `:200`. Both modes, as asked.
- **`CCC`:** `clear_command_channel()` at `:320` downgrades the control channel while keeping the data channel
  encrypted, for NAT traversal through FTP-aware firewalls.
- **TLS backends:** rustls via `tokio-rustls-aws-lc-rs` / `tokio-rustls-ring`, or native-tls via
  `tokio-async-native-tls`. rustls is pure Rust and is what Cmdr should pick.
- **Data-connection session reuse (the thing modern servers enforce):** `data_command` (`:893`) opens the data socket
  and then calls `tls_ctx.connect(domain, stream)` with the _same_ connector it used for the control channel. There is
  no explicit session-caching code, so whether reuse works depends entirely on the TLS backend's own session store:
  - With **rustls** it works. `AsyncRustlsConnector` wraps one `TlsConnector` holding one `Arc<ClientConfig>`, and
    rustls's default `Resumption::in_memory_sessions(256)` is keyed by server name, so the data connection offers the
    ticket the control connection was issued. Issue #93 is a user hitting FileZilla Server's "TLS session of data
    connection not resumed" rejection on native-tls and reporting that switching to rustls fixed it.
  - With **native-tls** it does not. The crate has no session store, and the reporter in #93 traced the failure to
    exactly that.
  - Issue #93 is still open, because what actually needs fixing is documentation rather than code. Treat the rustls
    requirement as load-bearing rather than a preference.
- **Certificate verification is the caller's.** The crate takes a connector, so Cmdr supplies the `ClientConfig`, the
  root store, and any trust-on-first-use policy for the self-signed certificates that FTPS servers on home NAS boxes
  invariably present. That's the right split, and it matches how Cmdr already handles SMB credentials through
  `CredentialStore`.
- **Cmdr's TLS graph already fits.** `Cargo.lock` carries `rustls` 0.23.41, `tokio-rustls`, and `rustls-native-certs`
  today; `suppaftp`'s workspace pins `rustls` 0.23 and `tokio-rustls` 0.26, the same majors. Adding it introduces no new
  TLS stack. ⚠️ One hazard: rustls 0.23 wants exactly one process-wide default `CryptoProvider`, and both `aws-lc-rs`
  and `ring` appear in the lock. Check with `cargo tree -e features` which one the app's `reqwest` and
  `tauri-plugin-updater` actually activate, and pick the matching `suppaftp` feature, or install the provider explicitly
  at startup. Getting this wrong is a panic on first TLS use, not a compile error.

### Axis 6: passive and active mode

**Clean pass, with passive as the default.**

- `Mode` is a three-variant enum: `Passive`, `ExtendedPassive`, `Active` (`crates/suppaftp/src/types.rs:82`).
  Construction sets `mode: Mode::Passive` (`:101`), so the safe default is the default.
- `set_mode(mode)` at `:282` switches at runtime; `active_mode(listener_timeout)` at `:255` is a builder that also sets
  the accept timeout, defaulting to 60 seconds.
- `epsv()` (`:975`) and `pasv()` (`:998`) are both implemented, and `active()` (`:1021`) sends `PORT` for IPv4 and
  `EPRT` for IPv6 automatically. ⚠️ `Mode::ExtendedPassive` is not the default, so an IPv6-only server needs Cmdr to
  select it (or to fall back on a `PASV` failure).
- `set_passive_nat_workaround(true)` at `:288` replaces the server-advertised passive address with the control
  connection's peer address, which is the standard fix for a server behind NAT advertising an RFC 1918 address. Cmdr
  should default this to on: it is right far more often than it is wrong.
- `passive_stream_builder(f)` at `:265` lets the caller supply the data-socket constructor, and `connect_with_stream` at
  `:95` lets the caller supply the control socket. Together they're the injection point for connect timeouts, TCP
  keepalive, and a `Happy Eyeballs` dialer, none of which the crate does for you.

### Axis 7: concurrency

**The crate is honestly single-threaded per connection, and the pooling is Cmdr's to write.**

- One `ImplAsyncFtpStream` is one control connection carrying at most one data connection.
  `guard_multiple_data_connections` (`:1181`) enforces it, and the protocol demands it.
- Every method takes `&mut self`, so a connection lives behind a mutex the way `SmbVolume`'s session does. A transfer
  holds that connection for the whole file, so a `list_directory` during a copy needs a **second** connection.
- Concurrency therefore equals connection count, and that collides with how FTP servers are configured.
  `transfer-subtree-concurrency-bench-2026-08-13.md` measured the useful transfer window at four to eight on David's
  real hardware. Shared hosts and NAS boxes commonly cap per-IP connections in that same four-to-eight range
  (`MaxClientsPerHost` on ProFTPD, `max_per_ip` on vsftpd), and exceeding the cap gets an IP temporarily banned rather
  than queued. So an FTP backend has to read `settings().max_concurrent_operations("ftp")` and default it **low**, and
  it has to treat a 421 `NotAvailable` as "back off", not "retry".
- Nothing in the crate pools. `crates/cmdr-smb/src/volume/scan_pool.rs` (646 lines) is the model to copy, and copying it
  is a meaningful slice of the build.

### Axis 8: async and tokio compatibility

**Clean pass.** The `tokio` feature builds `ImplAsyncFtpStream` on `tokio::net::TcpStream` with `tokio::io` traits
throughout. Nothing blocks; there is no `spawn_blocking` anywhere in the tokio path. Edition 2024, MSRV 1.88.0, and
Cmdr's pinned toolchain is 1.97.1, so there's headroom.

⚠️ **No I/O timeouts except two.** `connect_timeout` (`:83`) covers the initial TCP connect, and `active_timeout` covers
the active-mode accept. Every other await, including `read_response`, has no deadline: a server that accepts the
connection and then stops answering hangs the future forever. Given design principle two ("handle the hostile case (dead
mount, huge dir, crash mid-operation)"), Cmdr would wrap every call in `tokio::time::timeout`. Related: FTP control
channels get idle-timed-out by servers at around 300 seconds, so a pooled connection needs a `noop()` keepalive (`:366`)
or a reconnect path like `crates/cmdr-smb/src/volume/reconnect.rs`.

### Error classification

**Passes Cmdr's `error-string-match` rule without an exception.** `FtpError` is a six-variant enum, and the interesting
variant is `UnexpectedResponse(Response)` where `Response.status` is a `Status` enum of the numeric reply codes with
named variants (`FileUnavailable = 550`, `NotLoggedIn = 530`, `ExceededStorage = 552`, `NotAvailable = 421`,
`TransferAborted = 426`, and so on, `crates/suppaftp/src/status.rs`). Cmdr maps `Status` to `VolumeError` by matching
the enum, never the message text.

⚠️ The protocol limits how precise that mapping can be: 550 means "file unavailable" and covers not-found, permission
denied, and is-a-directory alike. Cmdr's `friendly_error` classification would be genuinely coarser over FTP than over
SMB, and no crate choice changes that.

### Version and the ≥3-day rule

- **Latest: 10.0.2, published 2026-08-18T16:03:59Z.** Four days old as of 2026-08-22, so it satisfies the three-day
  safety window.
- **10.0.2 is a security floor, not just the newest.** It fixed a CRLF command injection: an argument containing CR or
  LF ended the intended command line and smuggled a second command onto the control channel. In a file manager the
  arguments are filenames, and filenames arrive both from the user _and from a remote listing_, so a hostile server
  could hand Cmdr a name that turns into a command when Cmdr sends it back in a `RETR`. Pin `>=10.0.2` and never
  downgrade.

## The other candidates

- **`ftp` / rust-ftp is dead, definitively.** Last release 3.0.1 on 2018-04-15, last commit 2023-08-04, 36 open issues,
  sync-only, native-tls-only. Its git history is literally `suppaftp`'s history, up to the 2021 handover. Nothing to
  evaluate.
- **`async_ftp` is dead.** Dani Garcia's async-std fork of `rust-ftp`, last release 2021-11-04, 26 stars, 12 open
  issues. Even if it were alive, it's on async-std, which is unmaintained upstream (RUSTSEC-2025-0052) and is precisely
  what `suppaftp` 9.0.0 migrated off.
- **`remotefs-ftp` is maintained but disqualified by shape.** Same author as `suppaftp`, four stars, 1,518 lines,
  released 2026-01-18. It implements the `remotefs::RemoteFs` trait, which is **synchronous**:
  `open(path) -> RemoteResult<ReadStream>` and `create(path, metadata) -> RemoteResult<WriteStream>` where those are
  `std::io::Read` and `Write` (`src/client.rs:531`, `:516`). Wrapping it means `spawn_blocking` per chunk on the host
  runtime, which violates principle two ("never block the main thread") in spirit and burns a blocking thread per
  transfer in practice. It also pins `suppaftp` 8, two majors behind, so it's below the 10.0.2 injection fix. Skip it.
- **`opendal` with `services-ftp` is a real alternative worth naming and rejecting.** `opendal-service-ftp` 0.58.2
  depends on `suppaftp ^8.0.3` with the `async-std-rustls-ring` feature (the smol runtime under a tokio app) and adds
  `fastpool` for connection pooling, so it would hand Cmdr the pool for free. Against it: it pins `suppaftp` 8 (again,
  below the security floor), it runs a second async runtime inside Cmdr's, and its `Operator` API is object-store
  shaped, so `Volume`'s rename, per-entry metadata, and directory semantics would all have to be reconstructed from a
  flatter abstraction. Taking OpenDAL to get one protocol is a large dependency for a small win. Its existence is still
  a useful signal: the one production-grade Rust project that ships FTP chose `suppaftp` too.

## Fit against the `Volume` trait

Good fit overall. The specific frictions, in order of how much work each is:

- **The pool.** One connection serves one operation, so `list_directory` during a transfer needs a second connection,
  and background scan reads (`open_read_stream_for_scan`) want a third lane. This is `scan_pool.rs`'s job description
  and it's the largest single piece of the build.
- **The cancel contract.** `VolumeReadStream::cancel_and_release` must call `abort()`, and no path may drop a
  `DataStream` without it. Cmdr's conformance suite (`crates/cmdr-fs/src/volume/conformance.rs`) would want a new
  assertion for this, since it's a class of bug the trait can't currently prevent.
- **`read_range`, for remote archive browsing.** Implementable as `REST offset` + `RETR` + read `len` bytes + `abort`,
  but that's a whole data connection and an abort per range, where SMB does one positioned READ. `rc-zip` asks for
  several ranges per archive, so browsing a remote `.zip` over FTP would work and would feel slow. Implementing it is
  correct; setting expectations about it is a product decision.
- **`listing_watch_coverage` stays `None`, permanently.** FTP has no change notification of any kind. Per the
  new-backend recipe step five (`crates/cmdr-fs/src/volume/host/DETAILS.md`), that also means never calling
  `authoritative_listing`. Panes over FTP go stale until the user refreshes, and there is no fixing it.
- **`get_space_info` has no answer.** There's no standard free-space command. Some servers support `SITE DF` or the
  non-standard `AVBL`; most don't. The default returns `NotSupported` and the UI shows nothing, which is honest but
  visibly worse than every other backend.
- **`write_is_single_shot` answers `false`, always**, so every write stages on a `.cmdr-tmp-*` sibling and renames. That
  works (`RNFR`/`RNTO` are universal) and it's the right answer, but it doubles the round trips per small file.
- **The things that map cleanly**, for balance: `rename` is `RNFR`/`RNTO`, `delete` is `DELE`/`RMD`, `create_directory`
  is `MKD` with a `PathCreated` (257) reply, `exists` is `SIZE` or `MLST`, `lane_key` is the server plus port plus user,
  and `max_concurrent_ops` is a settings read. None of those are interesting, which is the point.

## How hard would this be to fork in six months?

**Easy, by the standards of this repo.**

- ~2,600 lines of non-test code on the tokio-plus-rustls path, which is roughly the size of
  `crates/cmdr-smb/src/volume/`'s four largest files. The 246 tests come along, and the Docker-backed integration tests
  would slot into the fixture infrastructure the SMB suite already runs.
- `MIT OR Apache-2.0` puts no conditions on vendoring into `crates/`.
- David wrote `smb2` and `mtp-rs` from scratch, so adopting 2,600 lines of someone else's protocol code is by a wide
  margin the least novel thing in this repository.
- The realistic stall scenario isn't abandonment (the maintainer is twelve years into this lineage), it's a major
  version Cmdr doesn't want to follow. In that case pinning and cherry-picking is a viable long-term position, which is
  not something you can say about most dependencies.

## Would Cmdr own the `LIST`-dialect parsing regardless?

**Yes, and concretely more of it than the feature list suggests.** No crate choice changes this, because no Rust crate
solves it. Cmdr would own:

1. **The `FEAT` probe and the MLSD-first policy**, since `suppaftp` parses `MLSD` lines but never decides to use them.
2. **The `LIST` fallback and dialect guess**, per directory, with a sticky verdict so 10,000 lines don't each pay two
   failed regex attempts.
3. **Every dialect past posix and dos.** EPLF, VMS, MVS, NetWare, OS/400, and the `ls -l` variants with a missing group
   column or a translated month name. Today those produce an empty pane.
4. **The unknown-line policy.** `ParseError::SyntaxError` per line must not become an empty directory. It has to surface
   as "this listing is partial", which is a UI concept Cmdr doesn't have yet.
5. **The mtime story**: minute granularity, no year on old entries, server-local timezone, and the `MDTM`-per-file
   escape hatch that doesn't scale.
6. **Non-UTF-8 names**, which can't be owned without patching the crate, because the lossy conversion happens below the
   API surface.

For scale: this is a few hundred lines of parsing plus a corpus of real server outputs to test against, and it's the
part that determines whether FTP in Cmdr feels solid or feels like 2003.

## Is FTP worth doing at all?

**My straight opinion: no, not next, and quite possibly not ever. The crates are fine; the protocol isn't worth the
milestone yet.**

The case against:

- **The install base is shrinking fast.** Censys counted roughly 5.94 million public-facing FTP servers in 2026, down
  from 10.1 million in 2024, a 40% drop in two years, and the remainder is concentrated in shared hosting and consumer
  broadband, which are the two segments migrating hardest.
- **Almost nobody is FTP-only by choice.** Every NAS that matters (Synology, QNAP, TrueNAS) ships SFTP. Every cPanel
  host ships SFTP. Every VPS has it by definition. The genuine FTP-only population is old routers, cameras, scanners,
  printers, and industrial gear, which is real but is not a paying-file-manager audience. **Shipping SFTP captures most
  of the people who would otherwise have asked for FTP**, and the sibling evaluation covers that.
- **FTPS specifically doesn't rescue the case.** It exists in banking and EDI workflows, but those are scripted
  integrations, not people browsing two panes.
- **The protocol fights Cmdr's principles at four separate points**, and none of them is fixable by trying harder: no
  change notification (stale panes), no free space (an empty status bar), imprecise mtimes (a weaker index and weaker
  conflict detection), and ASCII-by-default (a silent corruption footgun Cmdr must defend against forever).
- **The opportunity cost is the real argument.** WebDAV reaches Nextcloud, ownCloud, Box, Synology, QNAP, and Fastmail
  over stateless HTTP, which pools trivially, supports real ranged reads (so `read_range` and remote archives are cheap
  rather than expensive), and carries real timestamps. S3 reaches a larger and _growing_ audience, and it's where an
  AI-native file manager's story actually lives, because that's where people keep the datasets they want to search and
  understand. Either one buys more users per week of work than FTP does.

The case for, stated fairly:

- **It's table stakes on a comparison table.** Transmit, ForkLift, Commander One, Cyberduck, and Total Commander all
  ship FTP. A reviewer building a protocol matrix will mark the gap.
- The counter: those products are 10 to 20 years old and shipped FTP when it was the only option. A 2026 launch is
  judged on what it does that they don't, and Cmdr's answer there is SMB plus MTP plus archives plus AI, which none of
  them match. An FTP row wouldn't move that.
- **It's cheap once SFTP exists.** The pool, the reconnect loop, the credential flow, the transfer wiring, and the
  conformance tests are all shared. FTP after SFTP is maybe two weeks; FTP before SFTP would be four.

**What I'd actually do:** ship SFTP, then pick WebDAV or S3 by whichever the beta users ask for more. Put a line in the
feedback triage for FTP requests and count them. If the count crosses whatever bar feels right, `suppaftp` 10 is a solid
foundation and this document is the spec's starting point. If it never crosses, nothing was lost, and that's the whole
value of parking a decision that stays this cheap to reverse.

## If it gets built anyway: the shape

Recorded so a future spec doesn't re-derive it.

- `crates/cmdr-ftp`, depending only on `cmdr-fs`, following the ten-step recipe in
  `crates/cmdr-fs/src/volume/host/DETAILS.md` § "Writing a new backend". `const BACKEND: BackendName = "ftp"` (the
  DETAILS.md already uses `cmdr-ftp` as its worked example, which is a nice coincidence).
- `suppaftp = { version = "10.0.2", default-features = false, features = ["tokio", "tokio-rustls-<provider>"] }` where
  `<provider>` matches whatever the rest of the graph already activates. No `native-tls`, ever, because of the
  session-reuse finding in axis five.
- `transfer_type(FileType::Binary)` and `opts("UTF8", Some("ON"))` immediately after every login, on every pooled
  connection, pinned by a test that would fail if either is dropped.
- `set_passive_nat_workaround(true)` by default. `Mode::Passive`, with `ExtendedPassive` on an IPv6 server or a `PASV`
  failure.
- A connection pool modeled on `crates/cmdr-smb/src/volume/scan_pool.rs`, defaulting **low** (two or three), with 421
  `NotAvailable` treated as back-off rather than retry.
- `tokio::time::timeout` around every crate call, and a `noop()` keepalive on idle pooled connections.
- A cancel path that always routes through `abort()`, wired into `VolumeReadStream::cancel_and_release`, plus a
  conformance assertion so no future backend can regress it.
- `listing_watch_coverage` left at `None`, `authoritative_listing` never called, `get_space_info` left at its default.
