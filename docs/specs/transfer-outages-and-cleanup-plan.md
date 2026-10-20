# Transfer outages and cleanup plan

Issues: [#489](https://github.com/vdavid/cmdr/issues/489) (copies to session volumes fail after a successful reconnect,
and the error blames a file) and [#490](https://github.com/vdavid/cmdr/issues/490) (a failed SMB copy leaves temps and
open handles). Incident: `ERR-VEZUB` (0.54.0). Plan written 2026-10-19 against `8e3891a89`.

## Status and decisions for David

Status (2026-10-20): milestones 1–11 are done, including the close-out soak, conformance review, and docs audit. Once
this lands, #489 and #490 can close and this spec can be wiped (`docs/specs/DETAILS.md` § "Wiping a shipped spec"). What
the close-out left open is filed: #569 (pre-flight scan and compress planning walk don't wait), #570 (moving a folder
link on SFTP or ADB), #571 (same-volume renames and instant ops don't wait), #572 (a failed op doesn't name the bytes it
kept), #573–#577 (smaller landing and cleanup edges the conformance review found), #578 (a live cell that's flaky under
parallel load). The lead made these calls; they're yours to confirm or overturn:

1. **A Cancel never waits on a dropped share**, and a failed op takes the same shape: it's reported at once, cleanup
   finishes in the background (one dial, 20 s), and what's left goes to the ledger and is swept when the share
   reconnects.
2. **An Overwrite never empties the name of the only copy.** On SMB and local disks the swap is one atomic replacing
   rename, so a give-up leaves the original whole. Everywhere else (MTP, SFTP, WebDAV, ADB) the original is renamed
   aside under a ledger record, the new file renamed in, and the aside deleted last: a Cancel, give-up, drop, or crash
   at any step puts the original back when the share answers, else the next sweep that reaches the share does (possibly
   a launch later). Bytes whose way was already cleared (a claimed name's clearing delete) are still finished, not
   rolled back.
3. **A delete STOPS when an entry changed while the share was away** (`ChangedWhileDisconnected`) rather than skipping
   it: someone is working in that tree right now, and a delete has no rollback.
4. **Accepted residual risk**: a byte-identical file someone saved at the name during an outage is claimed as ours (an
   undo would remove it), over failing every NAS copy whose reply a drop swallowed.
5. **A reconnect sends `PreviousSessionId`** (#581, smb2 0.33.0), so the server frees the dropped session's handles
   before the new session is up. Safe because Cmdr opens no durable handles (Windows 8.1+ closes those too). The
   cleanup's refusal retries stay: Samba ignores the id for a guest login. How: `crates/cmdr-smb/DETAILS.md` § "A
   replacement session names the one it replaces".
6. **All new user-facing copy is a v0 draft for your review**: the "Lost the connection to {server}" error (copy, move,
   delete, compress, archive edit), "changed while disconnected", the cancel summary's "removed once it's back" and
   "lands once it's back" lines, and the unfinished-landing notice. ❗ The 17 new keys aren't translated yet and 2
   reworded ones are stale; `i18n-coverage` fails CI until a translator pass runs.

## Loud rules (every milestone)

- ❗ **Read first**: `crates/cmdr-operations/src/transfer/CLAUDE.md`, `transfer/volume/CLAUDE.md`,
  `transfer/transfer_driver/CLAUDE.md`, and the `transfer/DETAILS.md` sections "File writes are staged", "Retrying one
  FILE", "Waiting out a volume that went away", "Two tiers of cancel", "Cancel and rollback reach a parked driver". The
  code moved since this plan was written if a line ref is off: trust the code, keep the intent.
- ❗ **Data safety outranks the fix.** A temp that may be the only complete copy (the way was cleared) is ❌ never
  discarded or swept. The user's original is ❌ never deleted before a complete temp exists. When in doubt, leave the
  file and report it.
- ❌ **No string matching** on errors, ever (`error-string-match`, `cmdr/no-error-string-match`). Classify by
  `VolumeError` variant, errno, or `smb2::Error::kind()`.
- ❌ `through_outages` wraps ONLY a call that's safe to run twice. A write that may have half-landed goes through the
  per-file runner in `stream_pipe.rs`, which clears the attempt first.
- ❌ Never race a write against tier 1 (`backend_cancel`); ❌ nothing a user clicks reaches tier 2 (`backend_abort`).
- Retry stays per-file inside `stream_pipe_file` (the runner); ❌ never a retry loop in a driver.
- A `FaultyVolume` cell **asserts `fault_fired(op)`**; an `OutageVolume` cell asserts the drop actually happened
  (`reconnect_attempts() > 0` or an equivalent counter).
- **TDD**: see each red test fail for the RIGHT reason before writing the fix. Characterization pins reality, not this
  plan.
- ❌ **Never translate** new strings: English plus a good `@key` description. UI copy is a draft for David's review.
- ❌ No milestone tags in code, comments, test names, `reason` strings, or docs
  (`rg -n '\b(M[0-9]+[a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b'` before reporting).
- ❌ Don't edit the smb2 repo from here. This plan builds on the released 0.32.0 (see "The smb2 dependency"); a further
  smb2 change (smb2#24) is its own effort.
- **Docs move with the code**: every touched dir's `CLAUDE.md` + `DETAILS.md`. In particular `transfer/DETAILS.md` §
  "Not covered, deliberately" and § "Known gap, deliberately open: the abandoned SMB handle" shrink as milestones close
  them; describe current state, not history.
- Checks: `pnpm check --fast` per commit (scanners only: it compiles and tests nothing), plus
  `pnpm check svelte-tests-related` when FE files changed. Red/green single tests by name per `docs/tooling/testing.md`
  (`cargo nextest run -p <crate> <name>`). Before reporting a Rust milestone: `pnpm check clippy` and
  `pnpm check rust-tests` (these milestones are data-safety work; don't leave the suite to CI),
  `desktop-rust-integration-tests` for any Docker SMB cell, `desktop-bindings-fresh` when an IPC type changed.
  Foreground only.

## The smb2 dependency

smb2 0.32.0 is released (crates.io, 2026-10-19); milestone 1 bumps to it. What it gives us (its `CHANGELOG.md`):

1. **Typed loss for the failing sender**: the request whose own send broke (EPIPE, ECONNRESET, ECONNABORTED from the
   writer) gets `Error::SendFailed { command, source: io::Error }`, `kind() == ErrorKind::ConnectionLost`,
   `is_retryable()`. Every other waiter still gets `Error::Disconnected` (same kind). On an established connection
   `Error::Io` now means only a LOCAL failure (a streamed upload's own callback, say), never a lost connection. So cmdr
   classifies by `err.kind()` alone: ❌ no errno arm decides "the session is lost".
2. **A dropped handle sends CLOSE**: `FileWriter`, `FileReader`, `FileUpload`, `FileDownload`, `DirectoryReader`, and
   `Watcher` queue a CLOSE (no flush) before the drop returns, so it reaches the server after that handle's in-flight
   requests and before anything sent next: deleting the temp right after dropping its writer works. Two residual leaks:
   with no credit on hand the CLOSE goes out a moment later, and an `async fn` that holds a handle only for the call
   (`write_file_compound`, the pipelined reads and writes, `copy`) still leaks it when its future is dropped between
   CREATE and CLOSE ([smb2#24](https://github.com/vdavid/smb2/issues/24)). Milestone 9 handles both.

Baseline on `main`: smb2 0.31.1. Its compound write is CREATE+WRITE+FLUSH in one frame, then a standalone CLOSE; there
is no re-flush step any more, and a refused FLUSH comes back as `Error::Protocol { command: Flush }`. As a protocol
error on a non-CREATE command, `streams.rs::create_succeeded_but_write_failed` treats it as "created, then refused" and
deletes the file (ours); verify that shape before relying on it. That release removed the incident's TRIGGER (old Samba
resetting the connection on a mid-chain FLUSH); every escape below still happens on a drop from any other cause. ❌
Don't plan against a re-flush or "FLUSH retried" path: a FLUSH refusal is a refusal, never a connection loss.

`PreviousSessionId` on cmdr's reconnects (#490 layer 5) landed after this plan (decision 5): smb2 0.33.0 added
`ClientConfig::previous_session_id` and `SmbClient::session_id`, and each fresh client names the session it replaces.

## Diagnosis recap (what the code does today)

- **Landing escape**: a safe-replace (`Replaces::ViaTemp`) lands through `finalize::finalize_safe_replace` AFTER
  `stream_pipe_file` returns, at five sites (`merge.rs` ~285, `copy_concurrent_task.rs` ~274, `copy_serial.rs` ~540,
  `move_cross.rs` ~604, `sequential_extract.rs` ~212). No outage wait reaches it; `delete(orig)` on a dropped session →
  `what_a_refused_delete_left` → `Unknown` → `FinalizeFailure { DeviceDisconnected, new_data_at: None }` → terminal. The
  caller's temp isn't even in `in_flight_temps` (`StagedWrite::begin_unannounced` only `adopt`s it).
- **Ok on a dying connection**: the compound write's `set_times` (`cmdr-smb` `streams.rs` ~592) and `stamp_writer`
  (`stamping.rs`) log a connection loss as "keeps the server's date" and return `Ok`; the next step meets the dead
  session.
- **Untyped loss** (smb2 0.31.1): the failing sender gets `Error::Io(BrokenPipe)`, `ErrorKind::Io`, which
  `map_smb_error` (`mapping.rs` ~137) maps to `IoError { raw_os_error: None }`, so `retry::is_retryable` says no and
  `handle_smb_result` never marks the session `Disconnected`. smb2 0.32.0's `SendFailed` (kind `ConnectionLost`) fixes
  the type; `map_smb_error`'s existing `ConnectionLost` arm then maps it to `DeviceDisconnected` unchanged. The `None`
  errno stays a bug for the local `Io` errors that remain.
- **Stale error**: `volume_outage::wait_while_out` (~112) skips a volume that reads live, so a task's error from the OLD
  connection arriving after a sibling reconnected is `NotWaitable` → terminal.
- **Four funnels decide "the session is lost", each with its own copy of the kind check**:
  `session.rs:: handle_smb_result` (~127, ~35 call sites), `session.rs::update_state_on_smb_error` (the streaming-read
  producers in `streams.rs` ~285/336/358 and `random_reader.rs` ~35/73, which hold a bare `Arc<AtomicU8>` state), a
  hand-written mirror in `scan.rs` (~366, `scan_for_copy_batch_impl`, on a `clone_session` pair), and the watcher-death
  loop (`reconnect.rs` ~376, the watcher's own session). Plus `on_unmount`'s direct store (`volume_impl.rs` ~785).
- **Stale error moves the NEW session**: `session.rs::handle_smb_result` (~127) calls `transition_to_disconnected()` for
  any `ConnectionLost` / `SessionExpired` / `SignatureInvalid`, unconditionally (`state.rs` ~132 swaps the state with no
  check of which session the call ran on). The other three funnels do the same. A late error from the old connection,
  arriving after a sibling's reconnect installed a new session, flips that healthy session to `Disconnected`; the next
  waiter's `do_attempt_reconnect` then sees "not Direct" and rebuilds it, and every sibling on it takes an outage it
  never had, spending its `MAX_OUTAGES_PER_ITEM`.
- **A dropped task's safe-replace temp is cleaned as a partial, mid-landing**: a top-level concurrent task pushes its
  safe-replace temp into `in_flight_partials` (`copy_concurrent_source.rs` ~180) and only `record_success` forgets it. A
  task dropped INSIDE `finalize_safe_replace` (the driver breaking on a sibling's failure, or a cancel's drain deadline
  expiring) after `delete(orig)` went through and before the rename leaves the temp listed, and the post-loop's
  `clean_partial_writes` (`copy.rs` ~1205; the Rollback branch via `append_own_partials`) deletes it: the destination
  ends with neither the original nor the new file, and the failure names a sibling. The source still holds the bytes.
- **The landed-flags step has no error to classify**: `landed_flags::put_on` reads `apply_metadata`'s `MetadataReport`,
  whose `ClassOutcome` is `Kept` / `Refused` / `NotApplicable` / `Absent`; a flag the drop cut off is a `Refused`,
  indistinguishable from a server refusal. It's already counted in `metadata_losses` and reported, so this plan leaves
  it (a possible follow-up: a typed `ClassOutcome::Unreachable` the runner waits on).
- **Drop, not drain**: `copy_concurrent.rs` `run` breaks on the first failure and `finish` drops `in_flight`; writers
  never `abort()`, temps stay open (on smb2 0.31.1), `clean_abandoned_staged_writes` gets `STATUS_SHARING_VIOLATION`
  from our own live session, and has no outage wait either.
- **The stale-temp reap can't tell an old temp from committed data**: `cleanup.rs::reap_stale_transfer_temps` (~367)
  goes by name + `modified_at` ≥ 1 h, but every staged temp wears the SOURCE's stamped dates (`stamping.rs`; the
  creation date too), so a temp that holds the only copy (the way was cleared, then the landing stopped) reads as old at
  once and the next transfer into that folder deletes it. Only a ` (recovered)` rescue protects it today.
- **Two ways to record a temp**: the cross-volume engine's `StagedWrite` writes `Record::Legacy` lines via
  `in_flight_temps::register`, the local engine writes kinded `Record::Tracked` ones via `track(ItemKind::Temp)`. Only
  the tracked shape can be held for arrival (`keep_for_arrival` takes a `TrackedRecord`) or change kind.
- **Uncovered steps**: compress and remote zip edits (`archive_edit/fresh_compress.rs`, `remote.rs`), the move source
  sweep (`source_sweep.rs`), folder metadata (`folder_metadata.rs`), delete (`delete/walker.rs`), and the landed-flags
  step (`landed_flags::put_on`; stays uncovered, see above).

## Milestones

Each is independently landable and green. Sizes: XS < 1 h, S ≈ half a day, M ≈ a day of one agent. Order as listed,
except that 3b and 10a–10c may run any time after their prerequisites: 3b needs 3; 10a needs 2 and 3; 10b also needs 7
(its terminal error); 10c needs 7 and 6b (it reuses 6's landing mode, 5b's record, and 6b's settle). 5b must land
before 6.

### 1. SMB transport losses are typed and mark the session (M)

**Scope** (`crates/cmdr-smb`): bump to `smb2 = { version = "0.32.0", features = ["serde"] }` in
`crates/cmdr-smb/Cargo.toml` and `apps/desktop/src-tauri/Cargo.toml`, then `cargo update -p smb2 --precise 0.32.0` (see
"The smb2 dependency"). `SendFailed` has kind `ConnectionLost`, so `map_smb_error`'s existing arm maps it to
`DeviceDisconnected` with no new code; pin that with a test. Keep the errno for every remaining `Io` error
(`raw_os_error` off the `io::Error`, never `None` when one exists: those are local failures now, and their errno is what
the UI explains). Make the post-write `set_times` (compound path) and `stamp_writer` (streaming path) return the loss as
`Err(DeviceDisconnected)` through the error funnel when their failure's `kind()` is a connection loss. A refusal (any
other kind) stays "keeps the server's date".

Then collapse the four loss funnels (see "Diagnosis recap") into ONE predicate and ONE guarded transition:

- `fn ends_the_session(&smb2::Error) -> bool`, by `err.kind()` only: `ConnectionLost` (which now includes `SendFailed`),
  `SessionExpired`, `SignatureInvalid` (today's kinds; the streaming funnel skips `SignatureInvalid` today, and joining
  it is deliberate: a refused signature taints the session whichever call saw it). `handle_smb_result`,
  `update_state_on_smb_error`, `scan.rs`'s mirror, and the watcher-death path all ask it; ❌ no funnel keeps its own
  `matches!`.
- A session generation, bumped in the same critical section that installs a rebuilt `MainSession` (`reconnect.rs` ~235),
  stored on `MainSession`, and packed with the state in one `AtomicU64` (replacing the `Arc<AtomicU8>` the producers
  hold today). The flip to `Disconnected` is a CAS that succeeds only while the generation the failing call ran on is
  still the installed one, so a reconnect that installs between the check and the flip can't be overwritten.
  `transition_to_direct`, `on_unmount`'s store, and `test_support.rs` move onto the same atomic.
- How a call names its generation: `clone_session` returns a small lease (`{ tree, conn, generation }`) instead of the
  bare pair, and the funnel takes the lease's generation (for example `handle_smb_result(op, path, generation, result)`,
  or a method on the lease). The streaming producers capture it with the lease. The watcher records the generation it
  was spawned for; ❗ decide per remaining site (scan-pool sessions, any direct `session.lock()` user) and list the
  decisions in the report.

Milestone 3 exposes the same counter as `Volume::connection_generation`.

**Intentions**: a backend must never report `Ok` for a write whose own last steps saw the connection die, and must never
hide the errno that says "transport". With this, the transfer layer's existing waits see the right error. Returning
`Err` after the bytes landed is safe: a staged temp is cleared and re-run; a single-shot `CreateNew` goes through
`settle_unanswered_create` (byte-compare), already pinned.

**Landmines**:

- The predicate decides `Disconnected` by `kind()` alone; ❌ no `Error::Io` errno arm (on an established connection it's
  a local failure since 0.32.0, and an EACCES or ENOSPC must never end the session).
- ~35 `handle_smb_result` and ~29 `clone_session` call sites change signature: mechanical, but every one must pass the
  generation of the session ITS call ran on, ❌ never a fresh read of the current one (that silently disables the
  guard).
- Errno access: `Error::Io(e)` → `e.raw_os_error()`; ❌ never parse `to_string()`. Don't branch on `SendFailed`'s
  `source` either (smb2 says: branch on `kind()`); it's for the log.
- The compound path's standalone CLOSE already ran when `set_times` goes out; don't add cleanup that deletes the file on
  this new `Err` (a single-shot write at the user's real name). Only the existing `create_succeeded_but_write_failed`
  path deletes, and a refused FLUSH stays on it (a refusal, ❌ not a loss).
- The single-shot `CreateNew` then settles as landed through `settle_unanswered_create`, but the source's dates never
  went on (the stamp is what died). Re-stamp by path once settled (idempotent), or count it in `metadata_losses`; ❌
  never silently.
- ❗ The bump itself WIDENS which errors flip the state: the failing sender's EPIPE/ECONNRESET used to arrive as `Io`
  (no flip) and now arrives as `SendFailed`, kind `ConnectionLost`, which today's funnels already flip on. So the bump
  and the generation guard ship in the same milestone, ❌ never the bump landed ahead of the guard: without the guard, a
  late `SendFailed` from the dead connection takes down the fresh session.
- `smb2` version must be ≥ 3 days old unless it's ours (it is: exempt).

**Test plan**: red first in `mapping_test.rs` (a `SendFailed` maps to `DeviceDisconnected`; a local
`Io(PermissionDenied with EACCES)` keeps `raw_os_error = Some(EACCES)`), `state_test.rs`/`session` tests (a `SendFailed`
and a `Disconnected` both transition to `Disconnected`; an `Io` with any errno doesn't; a `ConnectionLost` from a call
that ran on generation N, handled after generation N+1 was installed, leaves the state `Direct`, through
`handle_smb_result` AND `update_state_on_smb_error`). For the stamp: if no seam exists to fail `set_times`, add the
smallest `#[cfg(test)]` one in `stamping.rs` rather than a timing-based live test; assert the write returns
`Err(DeviceDisconnected)` and the session reads `Disconnected`.

**DONE**: `Cargo.lock` resolves smb2 0.32.0; no path in `cmdr-smb` returns `Ok` from `write_from_stream` after a
connection-loss error in its own steps; errnos survive mapping; one `ends_the_session` and one guarded transition serve
every funnel (grep `transition_to_disconnected`, and `ErrorKind::ConnectionLost` outside `mapping.rs` and the predicate,
to prove it); a loss from an older session never moves the current one's state; `crates/cmdr-smb/DETAILS.md` says so
(with the smb2 version); `transfer/DETAILS.md` § "Known gap, deliberately open: the abandoned SMB handle" says a dropped
writer now closes its handle and names what still leaks (smb2#24's dropped `async fn`, a no-credit drop's late CLOSE).

### 2. One transport-fault classifier on `VolumeError` (S)

**Scope** (`crates/cmdr-fs`, `cmdr-operations`): add `VolumeError::transport_fault(&self) -> Option<TransportFault>`
(exhaustive match, no wildcard; `TransportFault` names the kind: timeout, connection lost, session reset, stale handle,
transport errno). `retry::is_retryable` becomes a thin call to it (or is deleted in its favor); `volume_outage`
(`wait_after_failure` and its module docs), `interrupted_landing::create_went_unanswered`, and `stream_pipe.rs` (~412,
the cancel-race arm) ask it. The transport-errno set lives ONCE, in `cmdr-fs` (for example
`cmdr_fs::volume::is_transport_errno(i32)`), adds EPIPE and ECONNABORTED, and
`error_classification.rs::classify_io_error` asks it too (today its `ConnectionInterrupted` list already disagrees with
`retry.rs`: it lacks ECONNRESET). Classify via `std::io::Error::from_raw_os_error(code).kind()` (`BrokenPipe`,
`ConnectionReset`, `ConnectionAborted`, `NotConnected`, `TimedOut`, `NetworkDown`, `NetworkUnreachable`,
`HostUnreachable`), ❌ not `libc`: `cmdr-fs` declares `libc` per target OS on purpose (`cfg-gate` watches it), and the
`ErrorKind` mapping is portable and needs no new dep.

**Intentions**: one place answers "did the link fail, or did the server say no?", typed, so every later milestone asks
the same question. `transport_fault` is the noun the step runner and the error UX both key off.

**Landmines**: exhaustive match is load-bearing (a new variant must decide); `Cancelled` is ❌ never a fault;
`NotConnected` is not a fault (nothing is dialing it). Keep `retry.rs` tests' coverage (move them, don't drop them).

**Test plan**: red: `an_epipe_is_a_transport_fault`, `an_econnaborted_is_a_transport_fault`; port every `retry::tests`
positive and negative to the classifier.

**DONE**: no other "is this transport" predicate remains in `cmdr-operations` (grep `is_retryable`,
`is_transient_errno`, and the errno list in `error_classification.rs`); docs point at the classifier.

### 3. A connection generation, so a stale error means "back, retry" (S–M)

**Scope**: `Volume::connection_generation(&self) -> Option<u64>` in `cmdr-fs` (default `None`, documented next to
`connection_state`); SMB answers it from milestone 1's counter (bumped on every SUCCESSFUL session rebuild, not on the
already-`Direct` early return in `do_attempt_reconnect`). SFTP, WebDAV, S3, and ADB are milestone 3b. In
`cmdr-operations`, the outage wait takes the generation the failing call STARTED on: `wait_after_failure` gains a
`started_on: Option<u64>` (the runner snapshots it before each attempt), and `wait_while_out` answers `Back` at once
when the volume reads live AND its generation moved past `started_on`. `through_outages` snapshots per call.
`OutageVolume` gains a generation that `become_reachable` + reconnect and `reconnected_elsewhere` bump. While in
`volume_outage.rs`, also key its five `volume outage: {name} …` log lines as `volumeName={name:?}` (asked by
`docs/specs/log-redaction-markers-plan.md` milestone 1, which owns the redaction cell for that keyed shape: the bare
label leaks today).

**Intentions**: in a concurrent window, one task's reconnect must not turn every sibling's in-flight error into a
terminal failure. "The volume is live and on a newer connection than the one my call used" is proof the failure was the
old connection's. A live volume on the SAME generation stays `NotWaitable` (the error is real).

**Landmines**:

- Snapshot BEFORE the call, never after the error (after reads the new generation and hides the race).
- `Back` from a generation bump still counts against `MAX_OUTAGES_PER_ITEM`, so a flapping link stays bounded.
- A backend that answers `None` keeps today's behavior exactly; ❌ don't synthesize generations from state flips.
- SMB's generation already exists (milestone 1, on `MainSession`); expose that counter, ❌ don't add a second one.
  (`SmbVolume` has no `OsMount` state: its state machine is `Direct ⇄ Disconnected`, and an OS-mount fallback is a
  different volume instance, so nothing there needs a generation of its own.)
- A snapshot before the call over-approximates: a call that snapshotted N while the volume was down, then ran on N+1
  after a sibling's reconnect, gets one re-run it didn't need. It's bounded by `MAX_OUTAGES_PER_ITEM` and safe (only
  re-runnable work goes through these waits); document it, don't engineer it away.
- The generation shortcut answers `Back` without joining the lane's outage record: ❌ don't call `Outages::back` from it
  (another waiter may be in a NEWER outage on that lane).
- ❗ **`land`'s first rename must not discard on a transport fault** (`staged_write.rs` ~527). Today a
  non-`AlreadyExists` rename error runs `discard_unplaced_temp` at once, and a discard that answers `Ok` or `NotFound`
  calls `release()`. `settle_interrupted_landing`'s proof ("the temp is still in the in-flight set") holds today only
  because that delete runs on the dead session and fails. With milestone 1's guard the session stays `Direct` for a
  stale error, and this milestone turns that error into `Back`. So the delete runs on the fresh session: if the rename
  DID land, it answers `NotFound`, `release()` runs, the settle says `RunAgain`, and the re-run's landing meets our own
  file under `ExpectedFree`: `DestinationExists` over the file we just wrote. Fix it in this milestone. When the first
  rename's error is a `transport_fault`, return without discarding: the temp stays registered, and the settle decides
  after the wait. On `GaveUp` / `NotWaitable` the temp is still a registered spare copy (nothing was cleared), so the
  operation's abandoned-write cleanup removes it. Red: `copy_landing_race_tests.rs`, a landing rename that performs on
  the server and then answers a transport error while the volume already reads live on a newer generation → the file
  counts as landed once, with no `DestinationExists`.

**Test plan**: red in `volume_outage_tests.rs`: "an error from before someone else's reconnect is waited past at once"
(volume live, generation moved → `Back`, no reconnect attempt dialed) and its negative (same generation →
`NotWaitable`). End to end in `copy_outage_tests.rs`: concurrent driver, task A's write fails after task B reconnected →
every file lands. SMB: a unit test in `crates/cmdr-smb/src/volume/reconnect_test.rs` that a successful reconnect bumps
and an already-live one doesn't.

**DONE**: the incident's "late error after reconnect" shape copies through; `transfer/DETAILS.md` § "Waiting out a
volume" documents the generation guarantee.

### 3b. SFTP, WebDAV, S3, and ADB report a generation too (S–M)

**Scope**: each of `crates/cmdr-sftp`, `cmdr-webdav`, `cmdr-s3`, `cmdr-adb` answers `connection_generation`, bumped on a
successful in-place session rebuild, and guards its own "mark the session dead on a transport error" sites against the
stale flip the way milestone 1 guards SMB's (audit them first; list each site and its decision in the report).

**Intentions**: the same "late error after a sibling reconnected" rescue for every backend that answers
`connection_state`. Independently landable: a backend answering `None` keeps today's behavior exactly.

**Landmines**: ❌ don't synthesize generations from state flips; a backend whose session is stateless per request (S3
over HTTP may be) can answer a generation that only its reconnect bumps, or `None` if it never marks itself dead. Say
which in its `DETAILS.md`.

**Test plan**: per backend, in its `reconnect_test.rs` (SFTP, S3 have one; add for WebDAV, ADB): a successful reconnect
bumps, an already-live one doesn't, and a late loss from the old generation leaves the new session live.

**DONE**: every backend that answers `connection_state` answers `connection_generation`, or its `DETAILS.md` says why
not.

### 4. Characterize safe-replace at all five sites (S)

**Scope**: tests only. Pin what a file→file Overwrite does today at `merge.rs` (deep child), `copy_concurrent_task.rs`,
`copy_serial.rs`, `move_cross.rs`, and `sequential_extract.rs`: success (original replaced, ledger records `orig`, op
not rollbackable, landed flags on `orig`, no temp left); a write failure (original intact, temp gone); a refused delete
of the original with it still there (original intact, temp discarded, typed refusal); delete ok then rename refused
(`(recovered)` file, failure names it via `new_data_at`); merge-invariant cells via `safety_oracle.rs`. Reuse
`finalize_recovery_tests.rs`, `*_preserves_new_data_on_finalize_failure` (`copy_crashsafe_tests.rs`,
`move_failure_tests.rs`), `safety_grid_tests.rs`; add only the missing cells.

**Intentions**: milestones 5 and 6 restructure exactly this path. These pins make "byte-identical behavior outside the
outage" provable.

**Landmines**: pin REALITY, not this plan. If a cell shows a bug, note it in the report; don't fix here. One known bug
cell to pin as it is (milestone 6 flips it): a top-level concurrent safe-replace task dropped between `delete(orig)` and
the rename (hold its rename, fail a sibling) has its temp deleted by the post-loop's `clean_partial_writes`, so the
destination holds neither file (see "Diagnosis recap").

**Test plan**: each new cell asserts `fault_fired(op)` where it injects.

**DONE**: a grid (site × outcome) of green pins, listed in the report.

### 5. `stream_pipe_file` becomes an explicit step runner (S–M)

**Scope**: refactor `stream_pipe_file`'s loop into named checkpoints (open source → write → land) with ONE `on_fault`
decision: classify via `transport_fault`, wait via `volume_outage` with the step's generation snapshot, and return the
checkpoint to resume at (re-open, re-run the write after clearing the attempt, settle the landing). Behavior otherwise
unchanged. `landed_flags::put_on` stays a best-effort tail after the landing, outside the runner: a flag a drop cut off
is already a counted, reported metadata loss (a possible follow-up: a payload-less `ClassOutcome::Unreachable` the
runner could wait on).

**Intentions**: one runner, one fault decision, so the safe-replace landing (next) and any future step inherit outage
handling by construction instead of per call site. The issue calls this `run_step`; the name is the executor's.

**Landmines**:

- Tier 2 still races only the open and the write; the landing stays un-raced.
- The stall watchdog stays unarmed for single-shot writes; re-armed per attempt.
- `attempt = 1` reset after an outage `Back`, `outages` counter per FILE (not per step).
- `unanswered_create` settles BEFORE the next open, exactly as now.
- Every `.await` keeps a `TaskPhase`.
- ❌ Don't wait on the flags call by reading its `ClassOutcome::Refused`: a refusal and a dropped link look the same
  there, and a wait inferred from it would stall on every server that refuses flags.

**Test plan**: a pure refactor: `copy_outage_tests.rs`, `copy_retry_tests.rs`, `strategy_retry_tests.rs`,
`strategy_abort_tests.rs`, `copy_staged_write_tests.rs`, `copy_landing_race_tests.rs` all green unchanged. No new red
test (no behavior changes); add a characterization cell first if any checkpoint's resume isn't pinned by those suites.

**DONE**: `stream_pipe_file` reads as the checkpoint machine; the three ad-hoc `wait_after_failure` sites are one.

### 5b. A temp that may be the only copy stays recorded until it lands (S–M)

**Scope** (`cmdr-operations`: `in_flight_temps.rs`, `in_flight_records.rs`, `in_flight_sweep.rs`, `staged_write.rs`,
`transfer/volume/cleanup.rs`):

1. `StagedWrite` records its temp as a tracked `ItemKind::Temp` (the local engine's shape: `in_flight_temps.rs`'s shared
   `record` body with the destination volume's `RecordHome`, as `track_in` builds it; a temp never waits on the writer,
   so `begin` stays sync) instead of `register`'s `Record::Legacy` line. Old logs stay readable; nothing new writes
   `Legacy`. This is what lets the volume engine's temps be held for arrival (milestone 9) and change kind (below).
2. A new `ItemKind::Landing { destination, expected_size }`: complete bytes on their way to `destination`, from the
   moment the way may be cleared. Where `land` calls `release()` today before clearing a claimed name (and, from
   milestone 6, before the swap's `delete(orig)`), it instead re-records the temp as `Landing` through the non-temp
   barrier (`track_in`'s: on disk before the clearing delete goes out) and drops it from the in-memory `in_flight_temps`
   (so no in-operation sweep may delete it). Retired once the rename lands it, a rescue renames it out of temp space, or
   `land`'s `StillThere` arm discards it (the delete was refused and the destination still holds what was there, so the
   temp was a spare copy after all).
3. Its sweep rule (startup and arrival, `in_flight_sweep.rs::settle_kind`) ❌ never deletes: destination missing and the
   temp at `expected_size` → `restore` (rename without replace: this FINISHES the landing). Check the temp FIRST: temp
   missing → `AlreadyGone`, so the record retires (a crash after the landing rename but before the record was retired).
   Without that check, `recover` on a missing temp answers `Refused` → `Deferred`, so the record never retires and every
   reconnect sweep retries it. Destination occupied → `recover` (` (recovered)` beside it; that's also what a crash
   after recording but before the delete leaves, a spare complete copy, never a loss); size mismatch → `LeftAlone` plus
   `announce_leftovers_kept`; unknown → `Deferred`. `name_matches_kind`: the temp marker.
4. `reap_stale_transfer_temps` skips every path the engine's ledger records for that volume (recorded or pending, any
   kind): a recorded temp is a live write or committed data, and its mtime is the source's (see "Diagnosis recap").

**Intentions**: resolves round 1's open residual for real. A temp that became the destination's only copy (a swap whose
landing a Cancel, a give-up, or a crash ended after `delete(orig)`, or `land`'s cleared claimed name, which has the same
window today) is never age-reaped, survives a crash, and is landed by the next sweep that can reach it. One record whose
kind changes once replaces "deregister, then wait for a barrier".

**Landmines**:

- ❗ The kind change must never leave a window where a sweep reads only `Temp`: write the `Landing` record (barrier)
  BEFORE retiring the `Temp` one, and make the sweep settle a path recorded under both by the non-temp kind. A test
  covers the crash between the two lines.
- The reaper's ledger lookup compares in the record's own path space (`RecordHome::record_path`) and volume id.
- `in_flight_temps` (in memory) keeps its meaning: exactly the removable copies of ours. ❌ A `Landing` is never in it.

**Test plan**: `in_flight_sweep_tests.rs`: a `Landing` with its destination missing is restored, occupied is recovered,
size-mismatched is left alone, one whose temp is gone retires (`AlreadyGone`), and none is ever deleted; a path recorded
as both settles as `Landing`. `staged_write_tests.rs`: a claimed-name landing whose delete is held (`FaultyVolume`,
assert `fault_fired`) has the `Landing` record on disk before the delete is issued. `cleanup_tests.rs`: the reap skips a
recorded temp with a year-old mtime and still reaps an unrecorded one.

**DONE**: no code path drops a temp from the persisted ledger before clearing a name; the in-flight ledger's
`DETAILS.md` section and `transfer/DETAILS.md` § "File writes are staged" describe the kind and the reap's skip.

### 6. Safe-replace is a landing mode of `StagedWrite` (M)

**Scope**: the conflict layer stops minting the temp. A file→file Overwrite resolves to `write_path` = the original's
own name and a payload-less `Replaces::Swap` (renamed from `ViaTemp(orig)`: the original IS `write_path` now).
`staging_for(Swap)` answers a new `WriteStaging::StageOverOriginal`, which `StagedWrite` mints and records like `Stage`
and lands delete-first (`delete(orig)`, then `rename(temp, orig, false)`) through ONE `land` with a
`LandingName::ReplacesOriginal`, sharing its cleared-way tail (5b's `Landing` record before the delete,
`what_a_refused_delete_left`, `rescue_out_of_temp_space`). `resolve_staging` never turns it single-shot (the original's
survival is the stronger guarantee). `failed_write_leaves_ours_at(StageOverOriginal)` is `false`, exactly like `Stage`:
the name holds the user's original. The flags go on after `commit`, as for any staged write.

Deleted by this: `WriteStaging::AlreadyStaged`, `StagedWrite::caller_temp`, `abandon_attempt`'s caller-temp branch,
`finalize.rs` (`finalize_safe_replace` and its private `swap_into_place`), `Replaces::swap_target`,
`PathedVolumeError::naming_the_replaced`, `landed_flags::hold` / `land_held` and the
`WriteOperationState:: flags_after_landing` map they share, the five sites' finalize calls (`merge.rs` ~285,
`copy_concurrent_task.rs` ~274, `copy_serial.rs` ~540, `move_cross.rs` ~604, `sequential_extract.rs` ~212),
`move_cross.rs`'s `landed_dest`, and the "dest_path is a temp sibling" contract in `transfer_driver/mod.rs`
(`TransferContext::replaces` ~220, `ConflictDecision::Proceed` ~490).

Outside an outage, behavior is identical, with one deliberate exception: milestone 4's known-bug cell flips (a task
dropped mid-landing no longer has its temp cleaned as a partial). The outage path is milestone 6b: until then a swap
landing's failure stays terminal, as today (see the landmine on the landing wait).

**Intentions**: the safe-replace temp becomes an ordinary staged write with one owner, so every later fix (the outage
wait, the ledger, the drain) reaches all five sites at once, and `write_path` names the file the user gets on every
path, so no layer above `StagedWrite` ever sees a temp-shaped destination. (Keeping `write_path` as the temp, the
earlier draft, would have moved partial cleanup anyway via `failed_write_leaves_ours_at` while keeping every
temp-to-original translation alive.)

**Landmines**:

- ❗ **One owner for the temp.** The drivers' partial tracking never names it (`failed_write_leaves_ours_at` is
  `false`), and `write_path` is the user's original, so a failed write must leave it alone: a failed write's temp is
  removed by `staged.abandon`, a dropped task's by `clean_abandoned_staged_writes`, as for any staged write. Audit every
  consumer of `Replaces::ViaTemp`, `swap_target`, `ResolvedConflict::write_path`, `CopyTaskSuccess::partial_path`,
  `last_dest_cell`, `in_flight_partials`, `append_own_partials`, `forget_in_flight_partial`, and `take_back_reservation`
  before changing a line, and list them with their decision in the report. ❗ Any site that cleans `write_path` on
  failure for a non-`Nothing` replace is now pointing at the user's original.
- ❗ `stream_pipe.rs` (~468) reuses `WriteStaging::AlreadyStaged` for a SECOND meaning: the `NotSupported` fallback
  ("this destination can't rename, write at the final name"). Give that its own representation (it's an unstaged write
  at a claimed name, close to `SingleShot(ClaimedByTheCaller)`); ❌ never route a swap through it, which would write
  over the original in place.
- ❗ **The landing wait.** `stream_pipe_file`'s commit-failure arm (~480) waits and then runs
  `settle_interrupted_landing`, which knows only rename-first landings: for a swap whose `delete(orig)` went through
  unanswered it would discard the destination's only copy. In this milestone a swap's failure skips that arm (terminal,
  as today); milestone 6b gives it the swap settle.
- The ` (recovered)` name and `new_data_at` reporting stay exactly as `finalize_safe_replace` does them today.
- **One moment for `record_overwrite()`** (found by the characterization grid, `safe_replace_grid_tests.rs`): `merge.rs`
  calls it before the finalize, `sequential_extract.rs` only after the finalize succeeds, so after a failed finalize
  their journals may disagree on rollbackable. With one landing path, pick the one correct moment (the original is gone
  the moment `delete(orig)` succeeds, so that's when the op stops being rollbackable) and pin it at all five sites.
- **Pass the size into the landing** (from 5b as built, `6d903ca03`): `hold_for_landing` takes `expected_size` from one
  stat of the temp, because 5b couldn't change `commit`'s signature without editing `stream_pipe_steps.rs`. With one
  landing path, hand `commit` the byte count the write already confirmed and drop the stat (a temp that can't be
  measured keeps the way uncleared today; keep that rule for any path still without a count).

**Test plan**: the milestone-4 grid stays green unchanged, except its known-bug cell, which flips: a concurrent task
dropped between `delete(orig)` and the rename leaves the temp on disk, recorded as `Landing` in the persisted ledger and
absent from `in_flight_temps` and every partial list (not cleaned; the op's failure still names the sibling, and naming
the kept temp is milestone 9's `staged_leftovers` case), and a startup sweep over that ledger lands it at `orig`.
`staged_write_tests.rs`: `StageOverOriginal` lands delete-first, never single-shot, never `rename(force)`.

**DONE**: `rg 'finalize_safe_replace|AlreadyStaged|swap_target|naming_the_replaced|caller_temp' crates/ apps/` is empty
(test names included); `transfer/DETAILS.md` § "File writes are staged", `volume/DETAILS.md`'s safe-replace text, and
`transfer_driver/` docs describe the landing mode.

### 6b. The swap landing rides out an outage (M)

**Scope**: `settle_interrupted_landing` learns the swap's states, and the swap failure takes the landing wait. Once the
share is back: temp still there and whole → resume the landing without re-streaming; temp gone, nothing of ours
discarded it, and `orig` holds exactly the SOURCE's bytes (compared in full, as `settle_unanswered_create` does) →
landed; anything else → run the file again (`stream_pipe_file`) or fail (`sequential_extract`). `sequential_extract`
gets the landing wait (it can't re-stream, but it can settle and re-land); with no source to compare against, a "temp
gone" swap there fails and reports, ❌ never counts as landed. The "nothing of ours discarded it" proof can't read the
in-flight set (5b took the temp out of it); carry it on the `StagedWrite` (a local "discard asked" flag, set by every
discard of this write's temp).

**Intentions**: the incident's landing escape closes at the root, for all five sites at once. Resuming a whole temp
matters more here than for a plain landing: after `delete(orig)` went through, the temp is the only copy at the
destination, so discarding it and re-streaming would be wasteful at best.

**Landmines**:

- ❗ **Classify before acting inside the swap.** A transport fault on `delete(orig)` or on the post-delete rename goes
  to the wait and the settle FIRST. Today's refused-delete stat (`what_a_refused_delete_left` → `StillThere` → discard)
  and the post-delete rescue (`rescue_out_of_temp_space`) run only for a non-transport refusal, or after the wait gave
  up. Otherwise a stale error whose follow-up stat runs on a sibling's fresh session reads `StillThere` and throws the
  temp away, or parks the new bytes under ` (recovered)`, for a blip. This covers the whole shared cleared-way tail, so
  a `StageOntoClaimedName` landing (a `Rename` placeholder, a cross-type aside) gets it too: today a stale transport
  error on its `delete(final_path)` reads `StillThere` on the fresh session, discards the temp, and returns the first
  rename's `AlreadyExists`, which no wait accepts, so a blip ends the op. Add that cell to the red list.
- ❗ **A resume never deletes what appeared during the outage.** When the settle shows `orig` already gone (the first
  delete went through), resume with the rename alone (`rename(force = false)`); a file someone put at `orig` meanwhile
  makes it refuse, and that's reported, ❌ never deleted. A claimed name's resume clears the name again only while it
  holds the entry the landing noted right before its delete (`#576`), and the drivers' partial cleanup never names a
  claimed name (`#575`).
- ❗ A Cancel, a failure drain (milestone 8), or a drop that ends the landing wait AFTER `delete(orig)` is treated like
  a give-up: rescue when the share answers, else leave the temp, recorded as `Landing` (5b), so the next sweep that
  reaches the share finishes the landing, and name it in the report. ❌ Never cleaned as a partial; the stale-temp reap
  skips it (5b).
- MTP's `rename(force=true)` duplicates: the swap stays delete-then-rename, ❌ never force.
- Whole-publish (`Replaces::InPlace`) is untouched; a cross-type Overwrite (aside) is untouched.
- ❗ **Fail closed when the `Landing` record can't be confirmed on disk** (lead's review of milestone 6,
  `staged_write_landing.rs::land_in_the_cleared_way`): today `hold_for_landing` → `in_flight_temps::track_in` returns
  the record even when the ledger barrier never confirmed the write (the writer is gone), and a `home` of `None`
  persists nothing, yet the clearing `delete` still goes out. Make the hold report whether the record is durably on
  disk, and when it isn't, don't clear the way: fail the landing as a refusal with nothing cleared (the original
  survives, the temp is discarded or kept as a temp). Red cells: a writer gone before the hold, and a `None` home, each
  leave the original intact. Check how the local engine's `track_in` callers rely on the same barrier and say in the
  report whether they need the same treatment (fix only the transfer path here; file an issue for the rest if it
  applies).
- ❗ **The server-side copy's landing gets the same wait** (found while executing milestone 3): `server_side_copy.rs`
  calls `staged.commit(...)?` with no outage wait or settle, so a late error at its landing still fails that file (since
  milestone 3 the temp goes with the abandoned-write cleanup instead of an immediate discard). Route it through the same
  landing wait + settle as `stream_pipe_file`'s (a red cell: a server-side copy whose landing rename performs, then
  answers a transport error → lands once).
- A move's source sweep must still run only after the landing succeeded (`move_cross.rs`).
- Journal and rollback ledgers record `orig`, never the temp.

**Test plan**: the milestone-4 grid stays green. Red (needs `OutageVolume::drop_at_delete(n, side)` beside the existing
`drop_at_rename(n, RenameDrop)`): for both drivers, a deep merge child, a cross-volume move, and a sequential extract:
(a) the drop hits `delete(orig)` before the server sees it → waits, lands, original replaced; (b) after the server
deleted `orig` but the reply was lost → resumes the landing WITHOUT re-streaming (assert the write count didn't grow),
no `(recovered)` file; (c) the drop hits the rename after the delete → resumes and lands; (d) the outage outlasts the
budget after the delete → the new bytes are rescued out of temp space and the failure names them; (e) the temp vanished
while `orig` still holds the OLD bytes at the same size → not landed (the byte compare catches it); (f) a stale
transport error on `delete(orig)` while the volume already reads live on a newer generation → no discard, the swap
re-runs; (g) a Cancel during the wait after the delete → the temp stays, recorded as `Landing`, and a later arrival
sweep lands it. Safety grid: overwrite policy + outage cells through `safety_oracle.rs`. Live: extend
`apps/desktop/src-tauri/src/operations_host/backend_suites/smb_outage_test.rs` with a pre-seeded destination and
Overwrite, cut mid-copy.

**DONE**: the incident's `.DS_Store` overwrite shape copies through an outage; `transfer/DETAILS.md` § "Waiting out a
volume" covers the swap settle.

### 7. "Lost the connection to {server}" instead of a device error (M)

**Scope**: a typed outcome for a transport fault on a session volume that ended the operation: the runner (and
`through_outages`) attach what the wait learned to `PathedVolumeError` (for example
`outage: Option<OutageVerdict { waited_for, gave_up }>`, set only when the failing volume answered `connection_state`
and the error is a `transport_fault`). The mapping boundary, which has the operation's captured sides
(`transfer_sides.rs`), turns it into a new
`WriteOperationError::ServerConnectionLost { path, volume_name, waited_for_secs, gave_up }` (`types/errors.rs`).
Regenerate the TS bindings; add the FE message in `transfer-error-messages.ts` with keys in
`intl/messages/en/errors.json` (`errors.write.serverConnectionLost.*`), the parity and keys tests, and a dialog-gallery
entry. Covers copy and move now; delete and archive edits get it in milestone 10 when their waits exist.

**Intentions**: the user is told the real cause (the connection), with the file as context only. After milestones 1–6b,
they see it only when we give up (15 minutes, or three outages on one file). MTP and local drives keep their own
`DeviceDisconnected` wording: a phone unplugged is a device, not a server.

**Draft copy** (❗ a DRAFT, not final: David reviews every word before it ships wider than v0; English only, no
translations):

- Title: "Lost the connection to {volumeName}"
- Message (gave up): "Cmdr waited {duration} for {volumeName} to come back, then stopped. It was copying {fileName}."
  (the verb follows the operation: "moving", and from milestone 10 "deleting" and "compressing"; give the error a typed
  operation kind so the verb never comes from a string); variant when it didn't wait (a sign-in needed, the flapping
  cap): "The connection to {volumeName} dropped while Cmdr was copying {fileName}."
- Suggestion: "Check that the server is on and your network is up, then try again. Files that finished copying are
  already there."

**Landmines**: ❌ never infer "server" from a path or message; only the verdict says so. `DeviceSessionReset` (MTP) and
`ConnectionTimeout` without a session keep `ConnectionInterrupted`. Error copy never says "error" or "failed". The
`WriteOperationError` is IPC-facing: renaming moves the TS union too.

**Test plan**: red in `transfer_error_tests.rs`: a gave-up SMB-shaped fault → `ServerConnectionLost` with the side's
name and wait; an MTP `DeviceDisconnected` → unchanged; a non-session volume → unchanged. FE (all in
`apps/desktop/src/lib/file-operations/transfer/`): `transfer-error-messages.test.ts`,
`transfer-error-messages.parity.test.ts`, `TransferErrorDialog.keys.test.ts`, `TransferErrorDialog.typed.test.ts`.
Checks: `desktop-bindings-fresh` (the IPC union moved) and `svelte-tests-related`.

**DONE**: a copy that outlasts its outage reports `ServerConnectionLost` end to end (one `copy_outage_tests.rs` cell
asserts the event's error), FE renders it, strings have `@key` descriptions.

### 8. Drain, don't drop, when one task fails (M)

**Scope**: on the first task failure the concurrent driver (`copy_concurrent.rs` ~236) stops preparing sources and
starts a FAILURE DRAIN, then keeps awaiting its window under the existing drain deadline (`copy.rs::drain_deadline`).
The signal is one token on `WriteOperationState`, ❌ not `OperationIntent`: `stop_work`, created as
`backend_cancel.child_token()`, so a user's Cancel fires it too and a failure fires ONLY it. Every place that should
stop for either reason watches `stop_work` (one token, so an async `select!` arm needs no helper); every place that must
tell the two apart (rollback, reversal, which terminal event) keeps reading the intent. Everything that already stops on
a cancel stops on it too: `LeafProgress::on_chunk` (`transfer_driver/progress.rs` ~326) returns `Break` (so the backend
runs its own `abort()` + delete, tier 1's cleanup), merge walkers stop spawning leaves, `wait_for_volume` and the retry
backoff end. Siblings' results are folded in (their landed files and rollback ledgers recorded), their cancel-shaped
errors swallowed; the FIRST failure stays the operation's error. A task still running at the deadline is dropped as
today. Naming: "wind-down" already means the cancel drain and the reversal's `WindDown` type (`state/wind_down.rs`), so
❌ don't call this one a wind-down anywhere (code, logs, docs).

**Intentions**: every writer closes its handle and deletes its own temp through the path that already works for a user's
Cancel, so a failed multi-file copy leaves nothing behind in the common case. Same shape as cancel, different cause, so
it must never read as one.

**Landmines**:

- ❗ The terminal event stays `write-error` with the first failure, never `write-cancelled`; the post-loop keys off the
  error's shape, so a sibling's `Cancelled` must not replace it.
- A sibling inside its landing (no `on_chunk`) finishes the landing: its file is complete and gets recorded.
- Audit every `intent().is_cancelled()` and `backend_cancel` read in the volume transfer path (~26 in `transfer/`) and
  decide per site: stop-work (`stop_work`) or which-reason (intent). List them in the report. At least:
  `retry::should_retry` and `wait_before_retry` (a draining sibling must never START another attempt),
  `volume_outage::wait_after_failure`'s entry gate and `wait_for_volume`'s loop and select arm, and `spawn_ready_tasks`.
  ❌ Don't hand `stop_work` to a backend in place of `backend_cancel` (MTP bridges that token): the backend learns of
  the drain through `on_progress`, like tier 1.
- A sibling in a swap landing's outage wait (milestone 6b) is ended by the drain like any wait, and then follows 6b's
  "ended after `delete(orig)`" rule: rescue or leave as `Landing`, ❌ never clean.
- Pause still parks; a paused operation's failure path must not deadlock on the pause gate.
- The serial path and `move_cross.rs` have no window; don't touch them.

**Test plan**: red in `copy_concurrent_driver_tests.rs` / `copy_concurrent_tests.rs`: one task fails while siblings are
mid-stream → each sibling's backend abort ran (its partial removed by the backend, not the sweep), `in_flight_temps`
empty, `staged_leftovers` empty, the event is the first failure, a sibling that landed is in the rollback ledger; a
wedged sibling is still abandoned at the deadline (existing `copy_cancel_tests.rs` shape). Live: a multi-file copy to
Docker SMB where one source read fails mid-copy leaves no `.cmdr-tmp-*` on the share.

**DONE**: `copy_concurrent.rs`'s "drop remaining in-flight tasks" comment and behavior are gone; `transfer/DETAILS.md` §
"Known gap" rewritten to the current state.

**Status: done.** As built (for the milestones after it):

- The token is `WriteOperationState::stop_work` (sync reading `work_stopped()`, which also reads the intent so a stop is
  seen in the instant before its token fires); the drain starts at `stop_work_after_failure()`.
- **Pause, decided differently from the landmine above**: the drain WAKES a paused, parked task (the pause flag stays
  set), so it stops through `on_progress` and its backend cleans up, instead of sitting on the gate until the deadline
  drops it with its handle open. Nothing resumes; a stopping task never parks (`stop_or_park_*`, `park_while_paused`).
- The drain also drops a sibling's pending Stop-mode prompt (`conflict_slot.abandon()`), as a cancel does.
- A Rollback clicked during the drain ended with `write-cancelled` AND `write-error` (a second terminal event, also
  reachable before through a real error landing after a Rollback click); the post-loop now drops the failure after the
  rollback's `write-cancelled`.
- A failed task's own partial (a claimed name) stays in `in_flight_partials`; `ConcurrentOutcome::last_dest_path` is
  gone, so every drained failure's partial reaches the cleanup, not just the last one's.
- The deadline's report is `OperationErrorReport::FailureDrainAbandoned` beside the cancel's `WindDownAbandoned`.
- Audit, stop-work (`stop_work` / `work_stopped`): `retry::should_retry` and `wait_before_retry`,
  `volume_outage::wait_after_failure`'s gate, `wait_for_volume`'s loop and sleep arm, `attempt_once`'s arm,
  `LeafProgress::on_chunk`, `CheckpointStream` (pause park, both foreground-yield arms, polling tick removed),
  `stop_or_park_*` / `is_stopping_or_paused` (so `UploadPark`, the merge and sequential-extract walkers, the server-side
  copy's park, and the serial async driver's boundary), the concurrent driver's spawn loop and await arm,
  `strategy::copy_single_path`'s entry check, `rename_merge_directory`'s entry check, `conflict.rs`'s dispatch check,
  `server_side_copy`'s cancel arm (a draining copy never falls back to streaming), `stream_pipe::write_failed`'s
  transport-to-cancel arm, `preflight`'s post-scan check, volume `folder_metadata::stamp`, and the watchdog's stand-down
  (`transfer_probe`). Which-reason (intent, unchanged): `copy.rs`'s post-loop, `transfer_probe::render_dump` (shows
  `running(draining-after-failure)`), the serial async driver's final intent check. Untouched (local engines, no
  window): `copy/single_item.rs`, `copy/folder_metadata.rs`, `copy_strategy.rs`, `move_op/`.

### 9. Cleanup waits for the server, and what's left is swept on reconnect (S–M)

**Scope** (per the lead's decision: ❗ a user's Cancel never waits on a dropped share; the same holds for a failed op,
so there's one shape): `clean_abandoned_staged_writes` deletes inline only what a LIVE destination takes (bounded
deletes, no wait). If the destination reads out, or a delete meets a `transport_fault` or a refusal (see the landmine on
our own open handle), the terminal event goes out at once and the remaining temps move to a BACKGROUND pass: one
immediate reconnect attempt plus a short bounded wait (for example 20 s), then the deletes. What that pass still can't
remove goes to the persisted ledger's pending set for that volume (`in_flight_temps::keep_for_arrival`; 5b made the
volume engine's temps tracked records, so it applies as is). Fire the ledger's existing arrival sweep
(`in_flight_sweep::on_volume_arrival`) when a registered volume reconnects in place, not only when it registers. ❌ No
new seam: `VolumesHost::on_volume_arrival`'s contract (`crates/cmdr-operations/src/host/volumes.rs` ~53, "whenever a
volume becomes available") widens to an in-place reconnect, and the app's implementation
(`apps/desktop/src-tauri/src/operations_host/volumes.rs` ~53 over `file_system/volume/manager.rs` ~138) also calls the
listener when a registered volume's connection goes to `Connected` (the `connection_changed` host event). The terminal
event's `staged_leftovers` tells "the server refused these" from "these go once {server} is back" with a typed field,
plus a third case for a `Landing` (6b): "this file gets its name once {server} is back", ❌ never worded as junk to
remove (FE copy: a draft for David).

**Intentions**: the user's Cancel (or the failure) is reported the moment it happens. A temp refused because the session
was down gets its delete once the server is back, and one refused for any other reason gets another chance on the next
reconnect (by then the session that held it is gone), without a launch. The leftover toast stays honest.

**Landmines**:

- ❗ The existing waits can't be reused for this pass: after milestone 8 both `wait_after_failure`'s entry gate and
  `wait_for_volume` end on `stop_work`, which has fired by the time a cancelled OR failed op cleans up. The background
  pass needs its own small wait, ended only by tier 2 (`backend_abort`) and the app quitting. On either, it stops and
  leaves the records registered and persisted for the startup sweep.
- The background pass ❌ never delays the terminal event or `write-settled`, and ❌ holds no lane: the next queued job
  on that share starts as usual (its temps have their own UUIDs).
- Hand-off to `pending` happens ONCE, after the background pass stops, so the pass and the arrival sweep never act on
  one record at the same time.
- The sweep ❌ never dials, mounts, or authenticates: a reconnect someone else made triggers it, nothing more. The
  background pass's single attempt is the only dial, and it's the op's own.
- The cleanup wait's "already gave up" outage (an op that failed on a 15-minute give-up) gets one look, then defers: ❌
  no second 15-minute wait.
- Only `Temp` records still in `in_flight_temps` (removable copies of ours) are deleted from here. A `Landing` (5b) is
  ❌ never deleted: it's handed to the arrival sweep under its own kind, whose rule finishes the landing. Asides and
  staging dirs keep their own rules.
- `on_volume_arrival` runs inside the registration call; the reconnect hook must stay as cheap (it already spawns), and
  a flapping link must not stack sweeps (`claim_pending` already hands each record out once).
- A writer the drain deadline dropped closes through smb2 0.32.0's close-on-drop, queued before the drop returns, so the
  cleanup's later delete normally finds the handle closed. Two cases still meet `STATUS_SHARING_VIOLATION` from our own
  session (the incident's shape, at 07:49:40): a drop with no credit on hand (its CLOSE goes out a moment later), and a
  dropped `async fn` holding a handle only for the call, such as a small staged temp's `write_file_compound` (smb2#24:
  that handle stays open until the session ends). On a healthy session no reconnect comes to retry either. So an inline
  delete refused for any reason also moves to the background pass (the terminal event still goes out at once), which
  retries it once, a few seconds later, before handing it to `pending`; the smb2#24 case then waits there for the next
  reconnect. No error classification is needed for this: any non-transport refusal gets the one retry.
- A Rollback with the destination out stays as it is: the reversal rechecks each entry, can't verify, and leaves and
  reports it. ❌ It doesn't join the background pass or the ledger (its ledgers aren't persisted records).
- **A size-mismatched `Landing` needs its own notice** (from 5b as built): the sweep leaves it alone (`LeftAlone`, a
  warn) but raises no notice, because `announce_leftovers_kept`'s only wording is about an unfinished MOVE's hidden
  folder. Give it a fitting typed notice (FE copy: a draft for David). Also, `LeftAlone` retires the record, so the
  name+age reap could later take that temp: keep such a temp recorded (or rescue it to a ` (recovered)` name) instead.

**Test plan**: red in `cleanup_tests.rs`: a cancel while the destination is out emits `write-cancelled` without waiting
(assert it arrives before any reconnect attempt returns, with a hung reconnect), then the background pass removes the
temp once the share is back; a still-refused temp lands in the ledger's pending set; tier 2 during the background pass
leaves the records registered. In `in_flight_sweep_tests.rs`: a reconnect (through the test host's arrival listener,
`host/testing.rs` ~137) sweeps a pending temp for that volume and leaves others'. Live (Docker SMB, `OutageProxy`): cut
during cleanup → temps gone after restore.

**DONE**: `transfer/DETAILS.md` § "File writes are staged" (Finding the litter, What that sweep can't remove) describes
the wait and the reconnect sweep.

**Status: done.** As built (for the milestones after it):

- The background pass is `transfer/leftover_cleanup.rs` (one per stopped op, on the engine's runtime, tracked in
  `EngineStores::leftover_cleanups` so a test can `Operations::leftover_cleanups_settled()`). Its timings are
  `CleanupPolicy { inline_budget: 5 s, wait: 20 s, retry_after: 3 s }` on the state (`with_cleanup_policy` in tests).
  The inline deletes are capped IN ALL by `inline_budget` too, for a destination that reads live and dies under them.
- "Ended only by tier 2 and the app quitting": tier 2 is the op's own `backend_abort`; the quit is
  `LeftoverCleanups::stop`, fired by `abort_all_write_operations`, because a pass outlives its op's registration and
  can't hear `abort_all`'s per-op token. A stopped pass leaves its records registered (not pending) for the startup
  sweep.
- The "already gave up" look is `Outages::gave_up_on` (the same test `join` makes): one reconnect attempt, no wait.
- The reconnect hook is app-side, ❌ no engine seam: `file_system/volume/manager/reconnects.rs::ArrivalOnReconnect`
  wraps every backend's event sink (`volume_host.rs`, test binaries included) and calls
  `VolumeManager::announce_reconnected` on a registered volume's `Connected` transition. `FakeVolumes::reconnected` is
  the engine fake.
- **Found while building it**: `StagedWrite::abandon` retired its temp BEFORE the delete, so a task that stopped on a
  share that was out forgot its temp there (no ledger, no in-memory entry, only the hour reap). It retires only after
  the delete went through now, and a refused one stays listed for the post-loop cleanup.
- `staged_leftovers` is `Vec<StagedLeftovers { fate, count, example_name, volume_name }>`, `StagedLeftoverFate` =
  `Refused` / `RemovedWhenBack` / `LandsWhenBack`. A landing a stop left recorded is collected through
  `state.kept_landings` (pushed by `Landing::keep_landing`), named by its FINAL name. Only `write-cancelled` carries
  them: a failed op's `write-error` doesn't (its pass still runs), which milestone 10's terminal errors may want.
- The size-mismatched `Landing`: the sweep rescues it to a ` (recovered)` name (record retired) and raises a notice. The
  notice folded the existing `MoveLeftoversKeptEvent` into one `LeftoversKeptEvent { volume_name, kept: KeptLeftover }`
  (`StagingFolder` / `UnfinishedLanding`), wire `leftovers-kept`, rather than add a seam method.
- The reconnect-sweep cell lives in `in_flight_temps_tests.rs` (where the arrival flow's cells are), not
  `in_flight_sweep_tests.rs`. Live:
  `smb_outage_test.rs::smb_integration_a_cancel_while_the_share_is_out_reports_at_once_and_removes_the_temp_once_it_is_back`.
- Copy (DRAFT, English only, for David): `fileOperations.cancelRollback.stagedLeftover.{named,counted}` reworded to
  "Cmdr tries again the next time it connects there", new `.waiting.*` and `.landing.*`, and
  `fileOperations.leftovers.unfinishedLandingKept`.

### 10. The remaining steps wait through outages (three milestones, one agent each)

Shared by 10a–10c. **Intentions**: every place a managed op meets a session volume waits like a copy does; together they
empty `transfer/DETAILS.md` § "Not covered, deliberately" except the pre-flight scan, trash, the local engines, and the
landed-flags step (milestone 5 leaves it: a flag cut by a drop is a reported metadata loss). Each step goes through
`through_outages` only where it's safe to run twice. **Test plan**: per step, red with `OutageVolume` (drop at the step,
assert the wait and the end state) and one `FaultyVolume` negative (a refusal still fails at once, `fault_fired`).
**DONE**: the "Not covered" list is current for that area; the area's `DETAILS.md` names its wait.

#### 10a. Move source sweep and folder metadata (S)

`crates/cmdr-operations/src/transfer/volume/source_sweep.rs` (not `transfer/move_op/source_sweep.rs`, the local
engine's): each delete; a `NotFound` after a wait means done. `transfer/volume/folder_metadata.rs`: each stamp.
**Landmine**: a source sweep must never delete something it can't prove is the ledger's: re-check per entry after a
wait, as `source_sweep.rs` already does.

**Status: done.** As built (for the milestones after it):

- The source sweep's item is the WHOLE top-level source: `sweep_carried_source` (now taking the op's `state`) runs
  inside `through_outages`, and a run after a wait re-lists every level and re-compares every stamp, so the per-entry
  re-check is the sweep's own; ❌ no single delete is ever retried blind. The top-level item's own delete reads a
  `NotFound` after a wait as done (the server performed it); a first-run `NotFound` keeps `SourceNotRemoved`. The
  into-zip move (`archive_edit/copy_into.rs::delete_originals`) shares the function, so it waits too. A sweep that gives
  up still reports `SourceNotRemoved` (the copy landed, and that's what the person needs to read), ❌ not
  `ServerConnectionLost`; its `lost_connection` is dropped at that mapping.
- The folder stamp has no error to classify (`apply_metadata` reports a drop as `Refused`, `folder_metadata` as
  nothing). New `volume_outage::wait_after_shortfall`, `wait_after_failure` minus its transport-fault gate (which now
  calls it): a stamp that came up short, or a source that was asked, asks the volumes whether one is `Disconnected` or
  live on a newer generation, waits, and stamps the folder again. Per folder `MAX_OUTAGES_PER_ITEM`; a give-up ends the
  waiting for the rest of that pass. `FolderMetadata::stamp` takes `&Arc<WriteOperationState>`.
- Test support: `OutageVolume::{drop_at_apply_metadata_of, refuse_metadata_of}`, and `FaultyVolume` forwards
  `connection_state`, `connection_generation`, and `attempt_reconnect`, so a `FaultyVolume<OutageVolume>` is a LIVE
  session that refuses (the negative cells). Cells: `volume/move_outage_tests.rs`, `volume/folder_metadata_tests.rs`.

#### 10b. Delete (S–M)

`crates/cmdr-operations/src/delete/walker.rs` (now `volume_walker.rs`): each per-item delete; a `NotFound` on a re-run
after a wait counts as deleted (ONLY after a wait: a first-try `NotFound` keeps today's meaning). Its terminal transport
fault maps to `ServerConnectionLost` (milestone 7's verdict). **Landmines**: a re-run never escalates to recursive, and
❗ re-checks the entry against what the walk listed (kind, size) before deleting again, since another client may have
put a different file at that path during the outage (mismatch → leave it and report).

**Status: done.** As built (for the milestones after it):

- The volume walker moved to `delete/volume_walker.rs` (the file-length allowlist's own split plan); the wait is
  `delete/outage.rs`. The walk's reads (type probe, listings) go through `through_outages`; each delete, file AND
  folder, runs `outage::delete_entry`'s own loop (`wait_after_failure` + `StartedOn`), ❌ never `through_outages`.
- "Leave it and report" STOPS the delete with a new IPC variant,
  `WriteOperationError::ChangedWhileDisconnected { path, volume_name }` (a delete has no rollback, and a changed entry
  says someone is working in that tree right now). The re-check compares `EntryKind` and, for a file, the size; a size
  the walk never learned fails closed. A top-level file the walk only probed gets one `get_metadata` during the walk, on
  a session volume only.
- A batch (`delete_files`, S3): a file it couldn't delete enters its own loop with the batch's error in hand.
- The folder pass keeps its silence for a non-transport refusal, but a transport fault that ends it is reported now (it
  read as complete before).
- `transfer::volume::map_failure_after_wait` (shared with 10c) maps the stop; FE: `serverConnectionLost.*.delete` keys
  and `changedWhileDisconnected.*` (DRAFT copy, English only, for David).
- Cells: `delete/delete_outage_tests.rs`. No live Docker cell (the plan asked none for this milestone).

#### 10c. Archive edits (M)

`crates/cmdr-operations/src/archive_edit/fresh_compress.rs` and `remote.rs`: the remote pull and upload. The remote swap
(`remote.rs::swap_into_place` ~284) is a third delete-then-rename; move it onto `StagedWrite` with `StageOverOriginal`
(milestone 6) so it inherits 5b's `Landing` record and 6b's settle, ❌ never a third copy of the swap rules. If the
types truly don't allow it, mirror classify-before-acting, the `Landing` record before the delete, and the settle, and
say why in the report. Fresh compression (`fresh_compress.rs`): the spool path re-uploads from the local spool after a
wait. The direct path can't resume its producer mid-stream, so after a wait it abandons the stage and either produces
again from the plan or falls back to the spool, whichever the executor finds cleaner. Its publish has three modes:
`commit` (`land`, covered by milestones 3 and 6b), `commit_atomic_replace`, and `commit_with_displaced_original`. The
last two delete the temp on ANY rename error, so a stale transport error after a rename that went through reports a
failure for a published archive. Give them classify-before-acting too: on a transport fault, wait, then settle (temp
gone and the archive at its expected size → published). Terminal fault → `ServerConnectionLost`. **Landmine**:
`reap_remote_temps` (`remote.rs` ~330) is a second name+age reaper; give it 5b's ledger skip if the temp's name or
recording changes.

**Status: done.** As built (for the milestones after it):

- The remote swap IS a `StageOverOriginal` landing: `StagedWrite::begin_over_original(state, volume, final, volume_id)`,
  committed and, when cut off, waited for and settled with the local working copy as the byte-compare source. A
  `RunAgain` uploads the local copy again. (Its follow-up gave every swap landing SMB's one-step `ReplaceIfExists` back;
  see below.) An archive edit's state names no destination volume (a move out writes two), so a `StagedWrite` now
  carries its ledger home (`home_volume`), and `pull_apply_upload_swap` takes the parent's id; a missing home fails
  closed.
- The swap's `original_replaced` journal note would have worded an archive edit's refusal as an overwrite's; the remote
  edit notes `ZipEditUnsupported` first (first note wins).
- `archive_edit/outage_wait.rs` is the archive side's one wait: `StepError::{Fault { error, otherwise }, Ended}`,
  `wait_out`, `rerun_through_outages`, and the landing/publish failure mapping (through `map_failure_after_wait`).
- The compress plan is replayable (`fresh_plan.rs::PlannedSource`, `FreshPlan::bridged`), each run with its own
  cancellation source (`fresh_pipeline.rs::ProducerRuns`); the direct route makes the archive again after a wait, the
  spool route uploads the spool again. `produce_into` returns `PipelineEnd`, whose `source_fault` keeps a remote
  source's typed error (the producer's own only words it). `validate_stage` hands back the volume's error.
- The two replace publishes moved to `transfer/staged_write_publish.rs`, take the size, and return
  `PublishError::{Failed(PublishFailure), CutOff(CutOffPublish)}`; `CutOffPublish::resume` answers `Republished`.
  `commit_without_a_wait` is test-only now. The publish loop is `archive_edit/fresh_publish.rs`.
- `reap_remote_temps` skips what the ledger records (`is_recorded_on`).
- Not covered, and why: the compress's planning walk (a pre-flight scan's twin).
- Cells: `archive_edit/remote_outage_tests.rs`, `archive_edit/fresh_compress_outage_tests.rs`, the reap and journal
  cells in `remote_tests.rs`. No live Docker cell (the plan asked none for this milestone).

**Follow-ups: done.** As built:

- `displace_destination` (every rename aside: the cross-type aside, the same-volume move's, the displaced publish's)
  classifies before acting: a transport-faulted rename keeps its `VolumeAside` record, waits for the share, and one stat
  of the aside settles it (there: set aside; gone: run again). A wait that ends without the share puts the file back
  when one stat can, else keeps the record for the share's next arrival. `DisplacedDestination::restore` classifies the
  same way before it rescues. The sweep reads every aside kind's aside FIRST, so a record of an aside that never
  happened retires (`AlreadyGone`) instead of deferring forever. Cells:
  `transfer/volume/displaced_destination_tests.rs`,
  `in_flight_sweep_tests.rs::an_aside_that_isnt_there_retires_whatever_its_destination_holds`.
- An archive edit's `write-cancelled` names the landings its stop kept (`archive_edit/engine.rs::EditLeftovers`, the
  copy's `LandsWhenBack` line on the parent's drive); the FE already words it. Cell:
  `engine_tests.rs::a_cancelled_edit_names_the_landing_its_stop_kept`.
- Every swap landing (copies and archive edits alike) takes ONE replacing rename on a backend with
  `supports_atomic_replace_rename` (local disks, SMB): `staged_write_landing.rs::replace_in_one_step`, cut off as
  `CutOffAt::ReplacingRename` and settled like a first rename except that a gone temp proves the landing only on the
  source's bytes. No delete, no landing record, no `(recovered)` path; a give-up leaves the original whole. MTP and
  every other backend land through an aside (next bullet). Cells: `staged_write_tests.rs`, `swap_outage_tests.rs` and
  `remote_outage_tests.rs` (the `InOneStep` cells).
- Every other backend's swap landing is recorded aside-first (`staged_write_landing_aside.rs::swap_through_an_aside`):
  the original renamed aside under an `ItemKind::FileAside` record (`DisplacedDestination::hold_for_swap` →
  `in_flight_temps::hold_aside`, on disk before the move, failing closed), the temp renamed onto its name, the aside
  deleted last. Each step classifies before acting (`CutOffAt::{SettingAside, RenameOntoAside}`); the settle asks the
  aside or the temp; a refused landing puts the original back (`DisplacedDestination::put_back`), and only when it can't
  go back is the temp rescued; anything left goes to the volume's next arrival, whose sweep restores or retires the
  aside per record. The temp stays a `Temp` (never the only copy), and the sweep settles a `Landing` before a
  `FileAside` (`ItemKind`'s variant order). `main`'s separate archive aside code (#513's `swap_into_place`,
  `restore_orphaned_asides`) is gone: the remote zip edit takes this landing, and its crash-after-every-step cell
  (`remote_tests.rs::a_crash_after_any_step_of_the_swap_loses_no_archive`, four cuts) holds main's intent at every cut:
  a whole archive at the name after the sweep, nothing left aside or in temp space. Cells: `staged_write_tests.rs`,
  `swap_landing_tests.rs`, `safe_replace_grid_tests.rs`, `swap_outage_tests.rs`, `remote_tests.rs`,
  `remote_outage_tests.rs`.

### 11. Close-out soak (S)

**Scope**: one live cell against the `flaky` Docker SMB fixture (5 s up / 5 s down): a multi-file copy with Overwrite
onto a pre-seeded subset, through several cycles, ends with every file landed, every overwritten original replaced, and
no `.cmdr-tmp-*` on the share. Then the adversarial conformance review and docs audit per
`docs/guides/multi-agent-refactors.md`.

**DONE**: the cell green twice in a row, run alone by name (`pnpm check desktop-rust-integration-tests` or a named
nextest run against the leased fixture; `e2e-linux.sh` is the Playwright suite's and doesn't apply); review findings
fixed or filed as issues.

**Status: done.** As built:

- The cell is `smb_outage_test.rs::smb_integration_an_overwrite_copy_rides_out_a_share_that_keeps_going_away`: 48 files,
  every other one over an original, through the app's own volume wiring (its SMB window of ten). The copy parks three
  times and the share dies under it each time; a watch connection tracks the fixture's halves so seeding, starting, and
  reading back happen inside an up half. Green on every run since the fix: five alone by name, about 20 more beside the
  other outage cells.
- It found a deadlock (3 of 3 hung at the app's window of ten): a merge walker awaited an outage wait without polling
  its leaves, while a leaf held SMB's single-flight `reconnect_lock` mid-dial. Every walker await now goes through
  `LeafPool::drive` (`f35ce3a42`). The "reconnect attempt didn't return within 15 s" it looked like was that deadlock;
  none appear since.
- Its probe dump also showed a resumed read still labelled `parked(pause)`, a label the stall abort exempts
  (`f82bec139`).
- Conformance review: one fix (`87b8a9e2f`: a transport fault after a stop reads as the stop, so a Cancel no longer ends
  as "Lost the connection"), the rest filed as #572–#577. Docs audit applied (`cd4fada93`).

## Invariants (the conformance review checks these)

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow, under every
  policy, backend, and mid-merge cancel, rollback, retry, OUTAGE, or failure drain. Asserted through `safety_oracle.rs`.
- **Staging**: no byte-incomplete file ever wears a final name; only single-shot or whole-publish skips staging, ❌
  never smallness. A landing clears a name only the caller claimed (`LandingName`); `AlreadyExists` alone earns the
  delete.
- **Committed data**: a temp is in `in_flight_temps` exactly while it's a removable copy of ours. Once a claimed name's
  way may be cleared it's re-recorded as `Landing` (on disk before the clearing delete goes out), named by no
  driver-side partial list (`in_flight_partials`, `last_dest_cell`), ❌ never swept, reaped, or discarded, and a failure
  rescues it out of temp space (` (recovered)`) or leaves it for the sweep that finishes its landing. A rescue or
  discard follows a classified refusal or a given-up wait, ❌ never a raw transport error.
- **Safe-replace order**: the original survives until a complete temp exists, and stays whole under its name or its
  recorded aside until the new bytes are whole at the name; one replacing rename only where
  `supports_atomic_replace_rename` says it's one step, else aside-first (the `FileAside` record on disk before the move,
  the aside deleted last), ❌ never `rename(force)` there, ❌ never a delete of the original. `write_path` always names
  the file the user ends up with; ❌ no layer above `StagedWrite` sees a temp-shaped destination.
- **Outage waits**: bounded (budget, `MAX_OUTAGES_PER_ITEM`, per-attempt timeout), cancel-at-once, pausable, shared per
  lane, ❌ only for re-runnable calls. A generation bump is proof of a stale error only when the volume reads live, and
  a stale error ❌ never moves a newer session's state.
- **Cancel tiers**: tier 1 travels via `on_progress` so the backend cleans up; ❌ writes never raced against it; tier 2
  only from the quit deadline, and nothing goes back through the connection after it. A failure drain (`stop_work`)
  reuses tier 1's path and ❌ never reads as a cancel to the user. A Cancel ❌ never waits on a dropped share: what the
  share can't take now is cleaned in the background, then left to the ledger's reconnect sweep.
- **Retry**: per file, only in `stream_pipe_file`, ❌ never on `Cancelled`, capped.
- **Ledgers**: a file is recorded once, from the caller's `Ok`, at its FINAL path (`orig` after a swap); journal rows
  never name a temp.
- **One of each**: one session-loss predicate and one guarded transition per backend, one transport classifier
  (`transport_fault`, one errno set), one landing function, one way to record a temp, one stop-work token.
- **Errors**: typed end to end; ❌ no string matching; a failure carries its path and side from the frame that knows the
  item.
