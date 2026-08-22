# An SFTP volume, fast enough that the network is the only thing you wait for

**Problem**: "More file systems" is the top planned feature in `feature-status.json`, and SFTP covers most of the people
asking. Every NAS, VPS, and cPanel host speaks it. The seams to write it behind exist and have survived a real network
backend (`crates/cmdr-smb/`), so this is a build rather than a rearrangement.

**Scope**: the backend crate and its IPC surface. **No frontend.** David designs the sign-in UX separately and builds
the FE against the commands this plan lands. The success test is that the FE work is wiring, never protocol.

**Out of scope**: FTP/FTPS (`docs/notes/ftp-crate-evaluation-2026-08-22.md`), SCP, S3, WebDAV, and `~/.ssh/config`
parsing (`ProxyJump`, `Match`, per-host aliases). ❗ Say the last one in the crate's `CLAUDE.md`: a user whose terminal
reaches a server through a jump host will expect Cmdr to, and it won't.

**Read before implementing**: `crates/cmdr-fs/src/volume/host/DETAILS.md` § "Writing a new backend" (a **ten**-step
recipe), `crates/cmdr-smb/CLAUDE.md` + `DETAILS.md`, `crates/cmdr-fs/src/volume/conformance.rs`, and
`docs/notes/sftp-crate-evaluation-2026-08-22.md` (❌ don't re-litigate the crate choice).

## Three premises that turned out to be wrong

1. **⌘R is not a pane refresh, and no shortcut re-reads a directory.** `⌘R` is `network.refresh`
   (`commands/sources/browsers.ts:20-26`); `pane.refresh` has `shortcuts: []` (`sources/mcp.ts:50-56`). Milestone 6 adds
   the binding.
2. **The refresh lie does not affect SFTP.** `refresh_listing` short-circuits only on `WatchCoverage::EveryWriter`
   (`commands/file_system/listing.rs:442-461`); SFTP answers `None`. The lie is SMB's alone.
3. **A backend does not populate `VolumeCapabilities`.** Two fields, and `Volume::capabilities` is a non-overridable
   fold over `is_writable()` / `supports_export()` (`volume/capabilities.rs:36-46`, `volume/mod.rs:690-705`).

**Also**: unused generated TS bindings trip nothing (`knip.json` ignores `bindings.ts` and `tauri-commands/**`), and a
Rust command in `ipc_command_manifest!` is reachable from the macro. **The IPC surface lands with no frontend and no
`#[allow(dead_code)]`.**

## Four crate hazards that will corrupt or hang if ignored

These are the sharp edges in `openssh-sftp-client` 0.15.7. Read them before writing byte-path code.

**1. ❌ Never use `File::read`'s offset bookkeeping or `read_all` for a byte path.** `file/mod.rs:403-428`: `read`
clamps `n`, may return **fewer** bytes than asked (its own note says so), and then advances the file offset by **`n`,
the requested length** — not by what it returned. `read_all` (`:543`) loops doing `n -= bytes.len()`, so a short read
re-reads from an offset that already skipped the gap: **a silent hole plus duplicated bytes.** The window must issue
positioned reads and track offsets itself. ❗ `read_range` is the natural `read_all` site, so it is where this lands if
nobody is watching.

The remedy works because `File: Clone` (`file/mod.rs:199`) shares the remote handle through an `Arc`, so N clones each
seeked to their own offset give depth N with no extra `SSH_FXP_OPEN` — which is exactly what the benchmark measured. ✅
`TokioCompatFile` is **not** affected: it advances by bytes consumed (`tokio_compat_file.rs:455-457`) and is a fine
basis for the write side.

**2. `File::write` is fine.** `file/mod.rs:434-464` clamps to the server's negotiated limit and writes exactly that,
returning the count. Honor the returned count; the offset advances correctly.

**3. `Sftp::close()` hangs forever over a `russh` channel** (`sftp.rs:305` awaits `read_task` then `flush_task`). **Drop
the session instead** — that's clean, not a leak: `SftpHandle::drop` orders a shutdown and both tasks exit
(`auxiliary.rs:160-165`). Two places depend on it: `disconnect_sftp_volume`, and the approval flow that abandons a dial.
Give the drop path a test.

❗ **`File::close()` is a different thing and you DO want it.** Dropping a `File` fires `SSH_FXP_CLOSE` on a detached
task and discards the result (`handle.rs:22-56`); `File::close()` awaits it and returns the error (`handle.rs:88-105`).
For a staged write the close is where a server reports a write it could not commit, so the write path **closes
explicitly before `land` and treats that error as a write failure**. ❌ Don't read "never `close()`" as covering both.

**4. A cancelled connect panics the crate's spawned task.** `tasks.rs:215` does `tx.send(extensions).unwrap()`, which
panics if the `Sftp::new` future was dropped before the server hello arrives — i.e. on any cancelled or timed-out
connect. ✅ **Not the approval flow**: host-key rejection happens in russh's `check_server_key` during kex
(`client/mod.rs:2306`), before `Sftp::new` exists, so an abandoned dial never constructs the task that panics. The two
that do reach it are a timed-out connect and `disconnect_sftp_volume`. **Structure connect so the future is never
dropped mid-handshake**: run it to completion under a timeout that is enforced _inside_ the task, and discard the
result. Open an upstream issue; note it in `DETAILS.md` so nobody removes the workaround as superstition.

## Numbers, in one place

From `docs/notes/sftp-crate-evaluation-2026-08-22.md`: Alpine + OpenSSH `sftp-server`, `netem delay 50ms`, 128 MiB. ⚠️
**The note's caveat governs**: the 50 ms column is the shape of the curve, not absolute truth, and the read lines
carried ±30% run-to-run spread.

|       | serial   | windowed              |
| ----- | -------- | --------------------- |
| read  | 4.2 MB/s | 42.4 MB/s at depth 32 |
| write | 4.1 MB/s | 29.4 MB/s at depth 16 |

- **Chunk size is 255 KiB on both sides**, and it is load-bearing: every figure was measured there. Depth 32 × 32 KiB is
  1 MiB in flight, roughly 20 MB/s at 50 ms.
- **`russh`'s default 2 MiB channel window caps reads at 14-18 MB/s** regardless of depth. Raise
  `client::Config::window_size` to 16 MiB. It does **nothing** for writes — the server's window governs and OpenSSH
  fixes it at 2 MiB — so depth is the only write-side lever.
- ⚠️ **`max_pending_requests` (default 100) is a flush TRIGGER, not a cap.** `wakeup_flush_task` (`auxiliary.rs:94-106`)
  notifies the flush task when the pending count reaches it; nothing blocks a sender, and there is no request-count
  ceiling in the crate. ❌ Don't "raise it to 256 for depth 128" and expect a change. Leave it at the default unless
  milestone 2's curve says otherwise.
- ⚠️ **Depth 32 is a starting point, not a commitment.** The note's own read of the SMB precedent is that useful width
  on real hardware was 4-8 against a much higher loopback number, and "expect the same discount here". Milestone 2
  measures the four-concurrent-streams curve and sets the shipped depth from it. Decisions 4 and 5 name defaults so
  there's something to measure against, ❌ not a number to defend.

❌ **Never ship a serial loop on either side as a first cut "to optimize later."**

## The connection model

**One SSH connection per volume, one SFTP channel on it. The read window is per stream; concurrency is per volume.**

- Concurrency 4 means four Cmdr _operations_ sharing one channel, not four TCP connections. A second connection means a
  second auth — with keyboard-interactive, a second 2FA prompt.
- It keeps us clear of the server's `MaxSessions` / `MaxStartups`.
- ❗ **The two windows interact.** Four streams at depth 32 × 255 KiB is ~31.9 MiB of outstanding read data against a 16
  MiB channel window, so they throttle each other. Milestone 2 measures this and sets per-stream depth accordingly.
- **Memory is per volume plus per stream**: the channel window plus each stream's reassembly buffer. Milestone 2 records
  measured peak RSS. If eight mounted servers cost more than a couple of hundred MiB, lower the channel window and
  re-measure — principle 5 is not optional.
- ❗ **Keep the `russh` wiring in one small module.** The note counted eight breaking minors in eight months; a bump
  should touch one file.

## Decisions

1. **Host-key approval uses two carriers, because there are two moments.**
   - **At connect**: `connect_sftp_volume` returns a typed outcome carrying
     `NeedsHostKeyApproval { fingerprint, algorithm, kind }`, `kind` being `Unknown` or `Changed`. Authoritative.
   - **Mid-life**: a **payload-free** `VolumeConnection::NeedsHostKeyApproval`. ❗ Payload-free because
     `VolumeConnection` is `Copy` on both sides of `events/volume_mapping.rs:39` (`wire_state`, whose doc says widening
     either end is a compile error here) and crosses IPC as a `specta::Type`.
   - ❗ A changed key is a possible man-in-the-middle and must never take the same one-click path as a first-seen one.
2. **Trusted host keys reach the backend through a new `HostKeys` seam** (the eighth trait; `runtime.rs` is a handle,
   not a trait). ❗ The obvious design doesn't compile: `config::durable_write_json` is app-side (`config.rs:76`) and
   `index-crate-isolation` forbids `cmdr` in a guarded crate's tree. Mirror `CredentialStore`: a trait in
   `crates/cmdr-fs/src/volume/host/`, implemented app-side over a durably-written `known-sftp-hosts.json`. Look up by
   `(host, port, algorithm)` → known-matching / known-mismatched / unknown, plus a record call.
   - ❗ **Keying by algorithm is only half the fix, and alone it is a security hole.** A server may offer several
     host-key types and present either depending on negotiation. Key by `(host, port, algorithm)` **and** pin `russh`
     `client::Config`'s `preferred.key` (`negotiation.rs:70`) to the algorithms already trusted for that host. Without
     the pin, an attacker who offers ed25519 where we hold an rsa entry lands on the **unknown** path and gets the
     one-click approval. With both, a healthy server presents the key we stored and any mismatch is a real change. This
     is what OpenSSH does.
   - ❗ **Every seam must degrade under `VolumeHost::detached()`.** A detached `HostKeys` answers **trust-nothing**
     (every key unknown). ❌ Don't make detached mean "trust everything" — a double that silently accepts any key is how
     a MITM regression ships green. ❗ But a no-op `record` would leave the fixture harness looping forever on "unknown
     → approve → still unknown", so the crate also ships an **in-memory `testing` double that actually remembers**,
     mirroring `credentials.rs`'s `InMemoryCredentials`.
   - `~/.ssh/known_hosts` is **read as a fallback and never written**.
   - Alternative rejected: resolving trust app-side and passing a verdict through `SftpConnectionParams`. Works at
     connect, falls apart at mid-life reconnect where the backend must re-verify without the app in the loop.
3. **Secrets go through the existing `CredentialStore` seam, never a JSON file**, keyed `service = "host:port"`,
   `scope = Some(username)`. ❗ Not `(host, None)`: two accounts on one server would share an entry, and a reconnect
   could retry the wrong account's secret into a lockout. The **key file path** is a connection parameter, not a secret.
   ❗ `credentials()` may block on a Keychain prompt — hand it to a blocking task, ❌ never call it on the async
   runtime. ❌ Never hold a secret past the session it built.
4. **Read-window depth starts at 32**, separate from `max_concurrent_operations`, and is set for real by milestone 2.
5. **Write-window depth starts at 16**, same.
6. **Auto-reconnect policy is per auth rung**: agent → reconnect freely, but a vanished socket or removed identity is
   `NeedsCredentials`; unencrypted key file → freely; **passphrase-protected key file → cannot reconnect unattended**
   (the passphrase isn't held past its session, decision 3) so report `NeedsCredentials`; password → re-read from the
   store and retry **once**, then `NeedsCredentials`, ❌ never a loop (lockout); keyboard-interactive → never
   unattended.
7. **Concurrency 4, no user-facing knob** — David's call, 2026-08-22, closing the product decision
   `docs/specs/backend-as-a-crate.md` recorded as blocking the next backend. Needs a row in
   `MAX_CONCURRENT_OPERATIONS_SOURCES` with a constant accessor, and that table's doc comment (which says a row means a
   _user-facing knob_) updated. ❗ A namespace with no row silently gets 2.
8. **A new known-servers store**, not a widened `KnownNetworkShare` (`share_name` and a guest-versus-credentials
   `AuthOptions` can't express key or agent auth).
9. **`Volume::rename` never uses `posix-rename` on the `force = false` path.**
10. **Acceptance gates assert shape, not absolute throughput**: each bench cell measures serial and windowed **in the
    same run on the same fixture** and asserts windowed ≥ 4× serial, recording the absolute without asserting on it. ⚠️
    A ≥10× gate against a ±30% measurement has 1.7% headroom; the first flake gets it lowered and then it means nothing.
11. **Volume ids get an `sftp-` scheme through `crates/cmdr-fs/src/volume/ids.rs`**, the one funnel every id is built
    through. ❗ This is durable identity — `index-{id}.db`, `lastUsedPaths`, tab fields — so it must separate two
    accounts on one host: key on `host:port:username`. Getting this wrong is a migration later, not a bug fix.

## Data safety

**1. `posix-rename` silently clobbers, and the obvious call reaches for it.** `Fs::rename` uses
`posix-rename@openssh.com` when the server offers it (`fs/mod.rs:214-236`); the extension is defined to replace the
destination atomically. `Volume::rename(from, to, force)` promises the opposite for `force = false`: return
`AlreadyExists`, touch nothing (`volume/mod.rs:466-490`).

- ❌ Never wire `Volume::rename` to `Fs::rename` unconditionally. `force = false` uses plain `SSH_FXP_RENAME`.
- `posix-rename` on `force = true` is a genuine win: it makes the archive swap's fast path atomic.
- ❗ The fixture that catches this is a server that **has** the extension. Milestone 3 red test.
- ❗ **Its detection therefore belongs in milestone 3, not milestone 4's probing.**

**2. SFTP v3 collapses errors into `SSH_FX_FAILURE`, and five conformance assertions branch on exact variants.**
`SSH_FX_FILE_ALREADY_EXISTS` is v4+. OpenSSH maps `EEXIST`, `ENOTEMPTY`, and most of errno onto the catch-all.
Downstream: all five `conformance.rs` assertions, and the folder-merge walker reading `AlreadyExists` from
`create_directory` as "merge into it". `error-string-match` forbids recovering it from the message.

Milestone 3 writes an explicit **error policy** with a fixture cell per variant:

- `create_file` uses `SSH_FXF_EXCL` so the server refuses the clobber rather than us racing a stat.
- Disambiguate an ambiguous `SSH_FX_FAILURE` with a stat probe. ❗ Use it to _classify a failure that already happened_,
  never as a pre-flight guard — as a guard it is a TOCTOU window.
- ❗ **`create_directory_all` needs its own cell**: a `Created` answer makes the transfer driver skip its per-file
  destination conflict probe, turning "would have prompted" into "overwrote" (trait doc above `mod.rs:361`). With
  `SSH_FX_FAILURE` collapsing mkdir-over-existing, that is exactly the ambiguity.
- Document the table in `crates/cmdr-sftp/DETAILS.md`.

**3. `land` deletes the destination on ANY rename error, and no backend-side policy can fix that.**
`write_operations/transfer/staged_write.rs:232-241`:

```rust
let Err(first) = dest_volume.rename(temp, final_path, false).await else { return Ok(()) };
match dest_volume.delete(final_path).await { ... }
```

It clears the way on every `Err`, so however precisely the backend classifies a transient `SSH_FX_FAILURE`, `land` still
deletes the user's file. ❗ **The app-side fix is in scope for milestone 3**: clear the way only on
`VolumeError::AlreadyExists`, and propagate anything else. This benefits every network backend, not just SFTP — SMB has
the same exposure whenever a rename fails for a reason other than a live destination.

**4. `write_is_single_shot` is `async fn(&self, size: u64) -> bool` and answers `false`**, so every SFTP write stages on
a `.cmdr-tmp-*` sibling and the partial never wears the user's filename. ❗ Do **not** copy SMB's
`create_succeeded_but_write_failed` classifier (`cmdr-smb/src/volume/streams.rs:47`) — it exists because SMB's compound
path _skips_ staging.

**5. The archive swap can be atomic.** `create_directory_errors_on_existing_dir()` gates the atomic fast path at
`archive_remote_edit.rs:250`, so answering it correctly **plus** `posix-rename` on `force = true` gives SFTP the atomic
swap instead of the delete-then-rename window. ❗ Milestone 3 connects these deliberately. (In a crash at that window
the data is not lost — it sits under the temp name, as `archive_remote_edit.rs:255-259` says.)

## Architecture

`crates/cmdr-sftp`, a crate from day one. `russh` 0.62.7 + `openssh-sftp-client` 0.15.7. ⚠️ `russh` 0.63.0 shipped
2026-08-21 and becomes eligible under the three-day rule on 2026-08-24; verify on crates.io.

**Crate wiring**: workspace member, `guardedIndexCrates` **and** `surfaceGuardedCrates` in `index-crate-isolation.go`.
❗ Every existing `surfaceGuardedCrates` entry's comment records David's say-so for its numbers, so **measure
`cmdr-sftp`'s surface at the end of milestone 5 and ask him for the ceiling** rather than inventing one. C+D.md pair,
`docs-reachable` link, cargo-deny, `specta` pinned to the app's exact version.

**The app wires it; the backend never registers itself.** `mtp/volume_wiring.rs` is the closer template than SMB's (no
OS mount, no upgrade path). ❗ Registration asks `would_keep_incumbent` before retiring a predecessor
(`manager.rs:135-150`). **`Retirement` + `SelfHandle`** are needed the moment anything spawned outlives a call — by
milestone 4's reconnect loop at the latest. ❌ Never retire on a re-root or promotion.

**Every trait answer SFTP owes:**

- Required, no default: `list_directory`, `get_metadata`, `exists`, `is_directory`.
- ❗ **Override `create_directory_all`** — the default (`mod.rs:361-400`) calls `exists()` once per ancestor, one 50 ms
  round trip per level, and conformance assertion 4 runs through it.
- ❗ **`open_read_stream` is the primitive the copy path uses.** `open_read_stream_at_offset` defaults to `NotSupported`
  and nothing calls it with a non-zero offset today. Also answer `open_read_stream_with_hint` and
  `open_read_stream_for_scan`, which exist precisely for high-latency backends and the scan pool.
- `read_range` — ❗ where hazard 1 lands. Positioned reads, self-managed offsets.
- `list_directory_with_cancel` and `list_directory_for_scan`: a large listing over a high-latency link is exactly their
  case.
- ❗ `begin_scan_session` / `end_scan_session` bracket the **index-scan lifecycle's** background walk, paired with
  `list_directory_for_scan` (`volume/mod.rs:234-257`). ❌ They do **not** wrap `scan_for_copy`.
- `notify_mutation` is the override; `listings().directory_changed` is what it calls.
- `lane_key()` → `server + port + user`. ❗ The default is the volume root, so two volumes on one server would each run
  full concurrency at the same host.
- `max_concurrent_ops()` → reads `settings().max_concurrent_operations(BACKEND)` **per batch dispatch**. ❗ The trait
  default is **1**.
- `create_directory_errors_on_existing_dir()` → follows from the error policy; see data safety 5.
- `attempt_reconnect` / `reconnect_with_credentials` → ❗ the signature is
  `reconnect_with_credentials(username: String, password: String)`, which fits neither key-file, agent, nor
  keyboard-interactive auth. Milestone 4 settles it: answer `NotSupported` for non-password rungs (and the banner must
  then not offer the button) or widen the trait. ❌ Don't leave a "Sign in" button that does nothing.
- `supports_local_fs_access` false, `paths_are_os_visible` false, `local_path()` `None`, `operations_are_local` false,
  `supports_streaming` true, `can_watch_listings` false, `listing_watch_coverage` `None` (❌ never call
  `authoritative_listing`), `connection_liveness` `None`. All four pause/foreground-yield opt-ins at defaults.
- ❗ **`get_space_info` answers `NotSupported`, and `space_poll_interval` is therefore `None`.** `statvfs@openssh.com`
  is **not reachable from this crate stack**: `openssh-sftp-client-lowlevel` has no `send_statvfs_request` and
  `openssh-sftp-protocol` carries only the extension _name_ so the hello parses. There is no `support_statvfs` predicate
  either (the predicates are `expand_path`, `fsync`, `hardlink`, `posix_rename`, `copy`). Free space needs the protocol
  crate vendored, which is the same escape hatch the filename problem uses — ❌ don't promise it in v1.

**Path handling**: mirror `SmbVolume::to_smb_path`, which **refuses** out-of-root paths with `NotFound`. ❗ Don't reach
for `cmdr_fs::volume::root_anchored` (`volume/mod.rs:1265-1281`): it _anchors_ rather than refuses, turning
`/etc/passwd` on a volume rooted at `/srv/data` into `/srv/data/etc/passwd`.

## Milestones

### 0. The check runner stands up two Docker stacks — **done**

Landed. What it means for the milestones after it:

- **The lease library is `scripts/check/stacklease`**, parameterized over a `Stack` value (compose project, `/tmp` lock
  file, `/tmp` lease dir, compose dir + files, mode → service table, no-healthcheck set, port-env prefix). The CLI is
  `scripts/check/stack-lease`, every verb taking the stack name first. The runner-level orchestrator is
  `StackOrchestrator`, holding one lease per stack under its own PID.
- **A check declares `NeedsContainers []StackMode`**, so `desktop-rust-integration-tests` can ask for the SMB and SFTP
  stacks together. `TestEveryDeclaredStackModeResolves` resolves every declared pair against the registry.
- **The SFTP stack is registered** as project `sftp-fixture`, compose dir `apps/desktop/test/sftp-servers` (the compose
  file sits there directly; `.compose/` is SMB's marker for a vendored tree), port env prefix `SFTP_FIXTURE_`, lock
  `/tmp/cmdr-sftp.lock`, leases `/tmp/cmdr-sftp-leases` — with an **empty service table**. Registration is inert:
  nothing asks for the stack, `Acquire` refuses every mode, and `Up` reports the missing compose dir rather than letting
  docker guess at a compose file.
- **The lane's filter is `fixtureIntegrationFilter`** (`scripts/check/checks/fixture-lane-coverage.go`), built from one
  fixture table that `desktop-fixture-lane-coverage` also guards. It already carries `test(sftp_integration_)`.
- ❗ **`+ package(cmdr-sftp)` could not land ahead of the crate.** `cargo nextest` fails to _parse_ a filterset naming
  an unknown package (`error: operator didn't match any packages`, verified on `cargo-nextest` 0.9.136, 2026-08-22), so
  the clause would have taken the whole SMB lane down. The filter therefore adds a backend crate's clause only once
  `crates/<name>/Cargo.toml` is on disk — so it appears on its own the moment `crates/cmdr-sftp` exists, with no edit.
- ❗ **The guard pairs marker with prefix.** An SFTP cell is one whose `#[ignore]` reason names `sftp-servers/start.sh`
  or `sftp-fixture`, and it must carry `sftp_integration_`. Wearing `smb_integration_` is a finding. The out-of-lane
  opt-out is `// allowed-out-of-lane-fixture-cell: <why>`.

**What milestone 1 owes this wiring**, beyond the fixture itself:

1. Fill the SFTP stack's `modeServices` table in `scripts/check/stacklease/registry.go`, in lock-step with
   `apps/desktop/test/sftp-servers/start.sh`, plus its `servicesWithoutHealthcheck` set.
2. Add `checks.SftpCore` to `desktop-rust-integration-tests`' `NeedsContainers` (the constant already exists), and add
   the SFTP services to the lane's `waitForSmbContainers` guard.
3. Add an `ApplySftpPortEnv` and its `portEnvAppliers` row if the fixture publishes host ports (a new pinned range clear
   of 11480+ and 10480+).
4. Name the compose project `sftp-fixture` and prefix every service `sftp-fixture-`, or the coverage guard's markers
   stop matching.

The model, the asymmetries, and why SMB's `/tmp` paths are frozen: `scripts/check/DETAILS.md` § "Two fixture stacks, two
lease namespaces" and § "How the integration lane selects fixture cells".

❗ **Milestones 2 through 5 are strictly sequential; milestone 6 is independent of all of them.**

### 1. The crate connects

Skeleton and wiring, `russh` transport in one module, the auth ladder, the `HostKeys` seam and its app-side store,
host-key TOFU, `SftpConnectionParams`, the `sftp-` id scheme, `list_directory`, `get_metadata`, `exists`,
`is_directory`, and the Docker fixture.

- **TDD, red→green — the host-key decision table.** Cells: unknown; known-and-matching; known-and-**changed**; approval
  recorded; `known_hosts` matching; ❗ **`known_hosts` present and mismatched** (the strongest MITM signal available);
  ❗ **`@revoked` never counts as a match**; `@cert-authority` recognized rather than misread as a plain key; ❗
  **detached `HostKeys` trusts nothing**.
- ❗ **Different key algorithm is not a changed key**, and the pin is what makes that true: a store keyed only by host
  cries MITM on a healthy server, training users to click through the one alarm that matters. Decision 2 settles the
  shape (key by `(host, port, algorithm)` **and** pin `preferred.key`); milestone 1 implements both halves. A cell for
  "server offers a second algorithm" belongs in the table above.
- **TDD, red→green**: path anchoring refuses out-of-root, matches by whole component, pins the `/srv/data-1` trap.
- Written after: connect, auth ladder, listing.
- **Docs**: `crates/cmdr-sftp/CLAUDE.md` + `DETAILS.md` (including the two crate workarounds and the `~/.ssh/config`
  non-support), `docs/architecture.md`, the `HostKeys` seam in `host/CLAUDE.md` + `DETAILS.md`, **the
  `docs/specs/index.md` line for this plan** (❗ `docs-reachable` is error-level and red until it lands).
- **Checks**: `--fast`, then `rust`, `index-crate-isolation`, `docs-reachable`, `rustdoc`.

### 2. The read path, with the window

Read window behind `VolumeReadStream`, `open_read_stream` (+ hint and scan variants), `read_range`,
`list_directory_with_cancel`, short reads, cancellation. Follow `cmdr-smb/src/volume/streams.rs`: bounded-channel
consumer, producer on `host.runtime()`, `total_size` through a oneshot before the constructor returns,
drop-as-cancellation.

- **TDD, red→green**: window reassembly under short reads and out-of-order completion, ❗ including a cell that would
  catch hazard 1's offset bug (a deliberately short-reading fixture, asserting byte-exact output).
- **TDD, red→green**: cancel-by-drop leaves the session usable.
- **Acceptance**: `netem` bench measuring serial and windowed in the same run, asserting windowed ≥ 4× serial. ❗ **Not
  as an `#[ignore]`d cell in `crates/cmdr-sftp`.** The lane runs `--run-ignored only` over the whole package, so every
  ignored test in the crate runs in CI by construction — a throughput ratio measured under runner contention would gate
  CI and flake. Put it behind its own feature or a `CMDR_SFTP_BENCH=1` env gate, run it locally and on demand, and carry
  the "every ignored test in this crate is a Docker cell" convention into `crates/cmdr-sftp/CLAUDE.md`.
- **Measure and record in `DETAILS.md`**: the four-concurrent-streams curve (which sets the shipped per-stream depth,
  and `max_pending_requests` with it), and peak RSS with the channel window open.

### 3. The write path, the error policy, and the `land` fix

Write window, `write_from_stream`, `create_file` with `SSH_FXF_EXCL`, `create_directory`, `create_directory_all`,
`delete`, `rename` with `posix-rename` detection, the error policy, `notify_mutation`, and the app-side `land` change.

- **TDD, red→green**: `rename(force = false)` refuses an existing destination **on a server that has `posix-rename`**.
- **TDD, red→green**: `land` clears the way only on `AlreadyExists` and propagates every other error. App-side, benefits
  SMB equally, and it's the difference between a transient blip and a deleted file. ❗ `InMemoryVolume` has no
  rename-failure injection today (`crates/cmdr-fs/src/volume/in_memory.rs:71-207` has `with_delete_failing` and friends
  but nothing for rename), so this milestone also adds one to `cmdr-fs`. ✅ The same fix closes a second flavor nobody
  has reported: today a `rename` returning `NotSupported` still runs the `delete`, so a backend that can delete but not
  rename destroys the file and then reports `NotSupported`.
- **TDD, red→green**: the error-mapping table, one cell per `VolumeError` variant the conformance assertions and the
  merge walker depend on, including `create_directory_all`'s honesty.
- **TDD, red→green**: cancellation arrives only via `on_progress` returning `ControlFlow::Break(())`, and every error
  path removes the staged temp.
- **Acceptance**: `netem` write bench, same shape assertion.
- ❗ Acquire the transport clone up front. (❗ The "never hold the session mutex across the upload" rule is SMB's
  concrete form of a general point: whatever ownership shape you pick, an upload must not serialize other operations.
  Pick the shape, then state it in `DETAILS.md`.)

### 4. Capabilities, space, reconnect

Extension probing (`fsync`, `copy-data`, `hardlink`, `expand-path` — ❗ only the `support_*` predicates are readable;
`max_read_len` / `max_write_len` are behind the crate's `__ci-tests` feature, and `statvfs` is unreachable entirely),
server-side copy via `copy-data`, `scan_for_copy`, conflict scanning, the reconnect loop with `Retirement` +
`SelfHandle`, the per-rung policy, and the `reconnect_with_credentials` seam decision.

- **TDD, red→green**: a server _without_ each extension still behaves correctly.
- ❗ Compare against previous state before emitting a connection event.
- Run all five `conformance.rs` assertions.
- **Required**: `host_seam_test.rs` asserting `RecordingListings::change_count` doesn't move while walking a real
  directory (recipe step 10).

### 5. The IPC surface

`connect_sftp_volume` (typed outcome), `approve_sftp_host_key`, `forget_sftp_host_key`, the credential trio, the
known-servers trio, `disconnect_sftp_volume`.

- **Two-phase approval, specified**: connect returns `NeedsHostKeyApproval` and **drops the session** (hazard 3: drop,
  never `close()`), holding no pending handle across the prompt. The FE calls
  `approve_sftp_host_key { host, port, algorithm, fingerprint }`, which records the key **only if that exact fingerprint
  is still what the server presents**, then calls `connect_sftp_volume` again for a fresh dial. ❗ Re-verifying the
  fingerprint is what stops an approval being replayed against a different key.
- ❌ Never a stringly-typed result.
- One line at the **end** of the right group in `ipc_command_manifest!` (group order is binding order), plus
  `#[specta::specta]`, `#[derive(specta::Type)]` on every DTO, `pnpm bindings:regen` committed, a typed wrapper in
  `src/lib/tauri-commands/`.
- ❗ **The wire half of the connection variant lands here**: `crate::network::VolumeConnection`
  (`network/mod.rs:230-242`) gains the variant and `wire_state` gains its arm — the compile error the doc promises. The
  FE reconnect manager will receive a state it doesn't handle; **it ignores it silently until David wires the banner**,
  with a comment saying so, so the gap reads as deliberate.
- ❗ **No `stubs/network.rs` entry** — that file exists because SMB browsing is macOS-only; stubbing SFTP would disable
  it on Linux, where the Docker E2E lane runs.
- **Docs**: a "connecting from the FE" section in `crates/cmdr-sftp/DETAILS.md`, so David's follow-up needs one file.
- Measure the crate's public surface and ask David for its `surfaceGuardedCrates` ceiling.

### 6. Refresh honesty (severable, touches no SFTP code)

1. Bind a shortcut to `pane.refresh`, so a manual re-read exists at all.
2. Add a force parameter to `refresh_listing`, guarding the `EveryWriter` short-circuit, so the user path and the MCP
   `refresh` tool genuinely re-read. ❗ `NewFolderDialog.svelte:207` also calls it and stays non-forcing. ❌ The
   write-op pre-flight oracle keeps its bright-line contract.

- **TDD, red→green**: forcing re-reads on an `EveryWriter` volume; not forcing still short-circuits. Assert the re-read
  happened, not that the call returned.
- The short-circuit exists because a 1k-entry MTP folder takes ~17 s, so forcing only on explicit user or agent refresh
  is the right scope.

## The Docker fixture

Mirror `apps/desktop/test/smb-servers/`, but SMB's compose is **vendored from the smb2 crate** while SFTP's is
first-party, removing the two-file `-f` layering constraint. Ports: a new pinned range clear of 11480+ and 10480+.

Servers covering what breaks clients: a large directory, deep nesting, **non-UTF-8 filenames**, **a stock OpenSSH server
that HAS `posix-rename`**, one without it, a small-`limits` server, **one offering two host-key algorithms**, **one that
short-reads** (hazard 1), key-only auth, password auth, passphrase-protected key, keyboard-interactive, and a changed
host key.

**Test geography**: a cell lives with whatever it **asserts**, never with whatever it connects to. Contract, byte-path,
conformance, window, and retirement cells live in the crate; anything driving `write_operations`, the volume registry,
or the listing cache lives app-side. ❌ Don't widen the backend's public surface to keep a test app-side. Prelude in
`test_support.rs`, ❌ not a `use super::*` glob.

**Lane coverage**: a cell in `crates/cmdr-sftp` is selected by the package clause, which `fixtureIntegrationFilter` adds
on its own once the crate exists. An app-side cell needs the `sftp_integration_` prefix, which
`desktop-fixture-lane-coverage` enforces against the fixture markers. ❗ Selection is not execution — the SFTP stack
needs its service table filled and a check pointing at it before those cells get a server.

## Filenames are bytes, and this is new

SFTP v3 filenames are bytes with no declared encoding; SMB is UTF-16 on the wire, so `smb2` could hand out `String`.

`openssh-sftp-client` fails a whole `readdir` on a non-UTF-8 name — loud, no corruption, one unlistable directory. **v1
accepts that** and surfaces it as a typed error naming the directory, because the alternative crate silently substitutes
U+FFFD and a folder copy then writes files under names that address nothing.

If it bites: vendor `openssh-sftp-protocol` (1,142 lines) plus `ssh_format` under `crates/` as a **path** dependency and
make `NameEntry::filename` byte-backed. ❌ Not `git =` — `deny.toml`'s `[sources] unknown-git = "deny"` forbids it.

⚠️ **A pattern, not an incident**: `russh-sftp` and `suppaftp` both mangle non-UTF-8 names through `from_utf8_lossy`,
and Cmdr's own `statfs` reader did until `a19efbffc`.

## Rules that bite a new backend

- ❌ Never `tokio::spawn` in a backend; use `host.runtime()`. It _inherits_ an ambient runtime and panics on watcher OS
  threads and during synchronous startup.
- ❌ Never gate behavior on `cfg(test)`; use `#[cfg(any(test, feature = "testing"))]`. **Bitten three times.**
- ❌ No user-facing prose in the backend; the host renders every human word.
- ❌ Nothing identifying in an analytics property — not hashed, not truncated.
- ❌ Don't depend on `cmdr-index`: a quarter of the codebase inside the inner loop for two method calls.
- ❗ Check every ``[`Type::method`]`` rustdoc link for an app-side target before finishing; it surfaces only at the end
  and has broken a merge here once already.
- ❗ Errors are typed, never string-matched, in tests too.

**What this does not buy**: `pnpm check` will not get faster (one shared `rustInputs` set, lanes run `--workspace`), and
full app builds get ~11% slower after a backend edit. The win is `cargo check -p cmdr-sftp` as a complete inner loop.
