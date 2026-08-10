# The silent-inertness hunt (2026-08-08)

A sweep for one defect class across Cmdr: **a mechanism that looks present and active but isn't reaching its subject.**
Not broken, _inert_ (a guard that never fires, a test that never touches its code) or _confidently wrong_ (an
unanswerable question turned into a fact). The class was named after a week of finding seven instances in the transfer
subsystem; this note records where else the same shapes live, what got fixed, what's recommended, and — as usefully —
which leads turned up nothing.

Ranked by blast radius. Everything below carries a file:line and a "wrong today vs merely unprotected" verdict, because
the two need completely different responses.

## Wrong today

### 1. `LocalPosixVolume::operations_are_local()` answers `true` for network mounts

`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix.rs:473` returns an unconditional `true`. But
`LocalPosixVolume` is what every mount that isn't a pre-registered `SmbVolume` gets (`file_system/mod.rs:182`,
`volumes/watcher.rs:280` via `register_if_absent`): an OS-mounted SMB or NFS share Cmdr couldn't upgrade to direct smb2,
a WebDAV mount, a Dropbox/Google Drive file-provider mount. On all of those, one operation is a network or XPC round
trip, not a microsecond `stat`.

The trait doc names this exact counterexample and the code contradicts it (`crates/cmdr-fs/src/volume/mod.rs`,
`operations_are_local`): "an OS-mounted SMB share answers `true` there [`supports_local_fs_access`] and would answer
`false` here."

Two things read the answer, both in `write_operations/transfer/volume/copy.rs`:

- `copy.rs:407` (`transfer_concurrency`): a `true` answer means this volume's `max_concurrent_ops` doesn't bound its
  peer. So a copy to an OS-mounted NAS runs at the LOCAL side's cap (`clamp(cores / 2, 4, 16)`), and the user's
  `network.smbConcurrency` setting has nothing to say about it.
- `copy.rs:554` (`dest_index`): a `true` answer skips building the one-listing destination name index, so the concurrent
  driver goes back to one `get_metadata` round trip per file. That index exists because those probes were **74% of a
  500-file NAS copy** (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`) — and the OS-mount path is exactly
  the NAS case it can't help.

**Blast radius: performance, not correctness.** Both consumers are optimizations, and the wrong answer picks the slower,
safer branch in each. Not fixed here: the fix needs `LocalPosixVolume` to learn its mount's kind, and the obvious source
(`filesystem_kind::detect_filesystem_for_path`) is a `statfs` that can block for many seconds on a hung network mount —
which is why volume discovery already runs off the main thread. See "Recommended" below.

### 2. `LocalPosixVolume::listing_is_watched()` can claim a listing is fresh when nothing is watching it

`local_posix.rs:247` answers `true` when a listing for the path is in `LISTING_CACHE` **and** an entry for its
`listing_id` exists in `WATCHER_MANAGER.watches`. That's a claim that a watcher is _registered_, not that it _delivers_.
`supports_watching()` above it (`local_posix.rs:243`) is likewise an unconditional `true`, so
`listing::operations.rs:133` starts an FSEvents watcher on any mount point, network ones included.

The claim feeds `listing::caching::try_get_watched_listing`, which hands the cached entries to write-op pre-flight in
place of a real read: the recursive delete walker (`write_operations/delete/walker.rs:417`, `:745`), the copy scan
(`write_operations/scan.rs:405`), and the scan preview (`write_operations/scan_preview.rs:482`). The trait doc is
explicit that `true` tolerates a _debounce window_; a watcher that never fires at all is a different animal, and nothing
distinguishes the two.

**Blast radius: wrong pre-flight numbers and confusing failures, not data loss** — and the reason it stops at that is
the `Volume::delete` non-recursion contract. A stale listing that misses a new child makes the walker skip it, and the
parent's `delete` then fails `ENOTEMPTY` rather than taking the tree. A stale listing that still calls a name a file
when it's now a directory ends the same way. That's the delete contract doing exactly the job the conformance assertion
exists to keep it doing.

**✅ The FSEvents half is now measured, and it makes finding 2 real rather than contingent.**

Measured 2026-08-08 on macOS 26.5.2 with `notify` 8.2.0 / `notify-debouncer-full` 0.7.0, against the live `naspi` share
(`//david@192.168.1.111/naspi` on `/Volumes/naspi`, `smbfs`), using a standalone probe that arms the same debounced
`RecommendedWatcher` Cmdr arms. The tightest run used ONE watcher over one 30 s deadline and both write paths:

- `t=4 s`, a **remote** write bypassing the mount (`smb2 put`, confirmed landed on the share): **no event, ever.**
- `t=14 s`, a **local** write through the mount (`touch`): `Create(File)` delivered.

Two earlier runs agree: three remote mutations (two creates, one delete, all confirmed on the share) produced **zero**
events across 40 s, while the mount's own `ls` reflected them immediately: smbfs re-reads on demand, but nothing is
pushed. A local-write control on the same directory, same probe, delivered.

**So FSEvents on `smbfs` is a local-VFS notifier, not a share notifier.** It reports what this machine did through the
mount and is structurally blind to what any other client does, which is the case a share watcher exists for.

Two consequences:

1. `SmbVolume::supports_watching()`'s justification was false and is corrected in place (the `false` return was right
   for a different reason: smb2-native watching is genuinely missing, not merely unoptimized).
2. **Finding 2 has a sharp real instance.** An SMB share with an OS mount and no Cmdr smb2 session is served by a
   `LocalPosixVolume` (`volumes/smb.rs:26` says so explicitly; it's what the `OsMount` badge means). That volume's
   `supports_watching()` is an unconditional `true`, so an FSEvents watch IS armed on the mount point and
   `listing_is_watched()` answers `true` for it, while that watcher cannot see a single remote change. The oracle then
   hands those cached entries to the delete walker, the copy scan, and the scan preview instead of re-reading. This is
   not a debounce window; it's a watcher that is right about "registered" and permanently blind about "delivering".

**Recommended next step (not taken here, because it's a tradeoff, not a typo):** make `listing_is_watched()` answer
`false` when the path's filesystem is a network one, so the oracle declines and the pre-flight re-reads honestly. The
cost lands on exactly the volumes where a re-read is most expensive, which is why it wants a deliberate decision rather
than a drive-by fix.

> **Resolved (2026-08-10).** Taken, with a sharper shape than "answer `false`": the boolean became
> `Volume::listing_watch_coverage -> WatchCoverage`, whose `ThisMachineOnly` variant names this exact state. The watch
> still arms (it's what updates the pane after the user's own writes, which answering `false` would have thrown away),
> and only `EveryWriter` lets the oracle skip a read. Current behavior lives in
> `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Trait capability model"; this note stays as the evidence
> that motivated it.

### 3. A delete reports "complete, 0 skipped" when directories it couldn't remove are still standing

Both delete tails discard the directory-removal result and then emit a completion event:
`write_operations/delete/walker.rs:231` (`let _ = fs::remove_dir(dir)`, local) and `:951`
(`let _ = volume.delete_with_cancel(...)`, volume-backed). Each is followed by a `WriteCompleteEvent` carrying
`files_skipped: 0` — hardcoded at `:247` and `:961` — and an `Ok(())`.

The _file_ loop above them is honest: a failed leaf delete propagates (`:880`, `return Err(map_volume_error(...))`), so
by the directory phase every enumerated file is gone. What's left to fail is a directory holding something the walk
never enumerated (a file created concurrently, one the listing filtered) or a permission refusal. In either case the
user asked for a folder to go away, watched a progress bar finish, was told nothing was skipped, and the folder is still
in the pane.

**Blast radius: an honest-progress violation, not data loss** (nothing is destroyed that shouldn't be; the wrong thing
is the report). Not fixed here because turning the discard into a hard failure is the wrong correction — a `.DS_Store`
Finder recreates mid-delete would then fail the whole operation. The honest fix is to COUNT the directories that
wouldn't go and report them, and the channel for that already exists and is wired shut: `files_skipped` on
`WriteCompleteEvent` is a literal `0` on both paths. That's a UX call about what the completion toast should then say,
so it's a recommendation, not a fix.

## Merely unprotected: the capability matrix

The lead question was whether any _other_ backend answers a capability flag wrongly. Systematically: no. Every override
of `lane_key`, `extraction_is_sequential`, `supports_streaming`, `max_concurrent_ops`, `supports_export`,
`supports_local_fs_access`, `create_directory_errors_on_existing_dir`, `write_is_single_shot`, `space_poll_interval`,
`supports_foreground_yield{,_as_destination}`, and `local_path` was read against its backend and matches reality.
`ArchiveVolume` leaves `operations_are_local` at the conservative `false` even though its bytes may be local, which
costs nothing.

Two structural notes that came out of the read:

- **`write_is_single_shot` and `write_from_stream` share their predicate.** The trait warns "answer with the SAME
  condition `write_from_stream` branches on", and SMB does: both call `fits_one_compound_write`
  (`backends/smb/streams.rs:262` and the `'write:` block below it). That's the fence already built. The one asymmetry is
  benign: with no negotiated params the predicate answers `false` (stage) while the write path falls back to
  `ASSUMED_MAX_WRITE`, so the transfer stages a write that didn't need it.
- **SMB's non-force rename existence check is a belief, and the wire is what enforces the contract.**
  `backends/smb/volume_impl.rs:577` reads `tree.stat(...).is_ok()`, so a `stat` that fails for permission or a transient
  reason says "nothing there". It doesn't matter: smb2 sends `ReplaceIfExists = false`
  (`smb2-0.18.1/src/client/tree.rs:3108`), so the server refuses the collision anyway. Worth knowing that the guard
  isn't the guarantee.

## What got fixed

### A shared conformance suite, from one assertion to four

`crates/cmdr-fs/src/volume/conformance.rs` held exactly one assertion (`delete` must not recurse). Three more now live
beside it, each pinning a promise that only a comment held, each chosen because a backend that broke it would destroy
user data with nothing reporting an error:

- `assert_rename_refuses_an_existing_destination` — `force == false` must refuse with `AlreadyExists` and leave both
  nodes untouched. `force` is the only thing between a move and the file it would replace, and every caller that hasn't
  yet asked the user passes `false`. Four backends earn the refusal four different ways (`renamex_np(RENAME_EXCL)`, an
  SMB `stat` plus `ReplaceIfExists`, an MTP `exists` probe, a map lookup), so there was no shared mechanism to trust.
- `assert_create_file_refuses_to_clobber` — an existing path must come back `AlreadyExists`, not be truncated. The
  no-clobber contract was stated in three separate backend comments and in no shared place; the New File command reads
  the refusal as "that name is taken", so a clobbering backend would silently empty a file and report success. The
  contract is now in the trait doc too.
- `assert_create_directory_all_reports_an_existing_dir_honestly` — a pre-existing leaf must be reported
  `AlreadyExisted`. `Created` is a promise the directory was empty, and the transfer driver spends it by skipping the
  per-file destination conflict probe for everything it writes inside. Only the dangerous direction is pinned: the trait
  says "when in doubt, answer `AlreadyExisted`", so the conservative answer stays legal.

Wired into every backend's suite that can run them: `InMemoryVolume` (all three), `LocalPosixVolume` (all three),
`SmbVolume` (all three, Docker-gated), `MtpVolume` (rename and `create_directory_all`; MTP has no `create_file`).
`ArchiveVolume` is read-only and pins the same ground with `every_mutation_is_unsupported`.

**Red-step verified.** Each assertion was run against a deliberately broken `InMemoryVolume` (clobbering `rename`,
clobbering `create_file`, a `create_directory_all` that always answers `Created`) and each failed with the message
naming the violation, before the double was restored.

MTP is the backend the `create_directory_all` assertion matters most for: it answers
`create_directory_errors_on_existing_dir() == false`, so the trait's default walk can't learn "already there" from a
collision error and has to learn it from the `exists` probe. Drop that probe as redundant and MTP makes a second
`Documents` beside the first, reports `Created`, and the driver stops probing for conflicts inside a folder of the
user's files.

### Three E2E tests that could report a pass without asserting anything

`apps/desktop/test/e2e-playwright/indexing.spec.ts` — the three byte-exact directory-size tests each carried a
`console.warn('SKIPPED: …'); return` escape hatch, and the file header advertised that the suite "skips gracefully" when
the index isn't ready. Two facts about that:

- The branches were **unreachable**. `waitForIndexData` / `waitForExactSize` end in `expect.poll(...).toBeTruthy()`,
  which THROWS on timeout, so control never arrives at the `if (!stats)` below it. Their `| null` return types, the
  `?? await getDirStats(...)` fallback, and every downstream `expect(x).not.toBeNull()` were all inert too.
- Had they been reachable they'd have been the bug: a warn-then-return is a green pass, and an index that never
  converges is precisely what these tests exist to catch.

Fixed by making the helpers' types tell the truth (they never return null), deleting the dead branches and the vacuous
null assertions, and replacing the header's false promise with a guardrail against reintroducing the escape hatch. No
assertion was weakened: the real ones (`recursiveSize === expected`) are untouched, and the convergence deadline is now
the only thing deciding pass or fail, as it already was.

### Two stale doc claims

- `Volume::pause_releases_read_stream`'s doc said "no backend currently overrides it"; `MtpVolume` does
  (`backends/mtp.rs:958`, restating the same `false`). Corrected to say why the restatement is there.
- `Volume::create_file` had no contract in its doc at all; the no-clobber promise now lives there with the conformance
  pointer, matching how `delete` states its own.

## Leads that turned up nothing

Negative results, recorded so nobody re-runs them:

- **Module-scope hooks (the Playwright bug's siblings).** Vitest is clean. The shared helpers that own hooks
  (`file-explorer/pane/integration-test-utils.ts`) expose them as a `setupMountingTest()` function each file CALLS, so
  they register on the calling file's suite; the rest document "call this in your `beforeEach`" rather than registering
  anything at import. `vitest.config.ts`'s single `setupFiles` entry applies per test file by design, and Vitest's
  per-file module registry would defeat the Playwright shape anyway.
- **Fault injectors that never fire.** Every `InMemoryVolume` lie in the tree (`set_stat_failing`, `set_reported_size`,
  `set_reported_type`, `with_delete_failing`, `with_read_range_unsupported` — 12 arming sites) is armed by a test whose
  assertions FAIL if the fault didn't fire: an `expect_err` plus a typed match on the failing path, or a
  `report.skipped` count that would be zero. They're self-verifying, so they don't need `FaultyVolume::fault_fired`'s
  guard. The shape that needed it was specific: **a test that arms a fault and then asserts an absence.** That's the one
  to watch for, not fault-arming in general.
- **Silent early-returns in the Playwright specs.** `smb.spec.ts` (3 sites), `app.spec.ts` (2), `viewer.spec.ts`,
  `i18n-capture.spec.ts` all either call `test.skip()` (a visible skip) or `throw` on the not-found path. Only
  `indexing.spec.ts` had the fake-pass shape, and it was already unreachable.
- **`#[should_panic]` without `expected`.** None in the tree.
- **`let _ =` / `.ok()` in the transfer and rollback paths.** Read every site under `write_operations/` and
  `operation_log/`: they're event emits (fire-and-forget by contract, a failed emit must never fail the mutation that
  succeeded), best-effort partial cleanup after an already-failing operation, or display-only metadata whose `None` the
  frontend renders as "(unknown)". `conflict.rs:185`'s `get_metadata(dest).await.ok()` looks like the dangerous shape
  and isn't: it feeds the dialog's "(newer)" annotation, and the size that feeds `reduce_volume_conditional_resolution`
  already carries a documented ❗ against fabricating a `0`. The one real finding in this shape is (3) above.
- **`read_range` returning a short read mid-file.** The contract ("fewer than `len` bytes ONLY at end of file") would
  silently corrupt remote-archive browsing if broken, and none of the three implementers breaks it: `LocalPosixVolume`
  loops over `pread` until the window fills or the file ends (`local_posix.rs:540`), `MtpVolume` loops over
  `read_range_direct` with an empty chunk as the terminator (`mtp.rs:934`), and `SmbVolume` hands the whole window to
  smb2's `read_at`, which itself loops over `max_read`-sized SMB2 READs (`smb2-0.18.1/src/client/stream.rs:416`). Still
  worth a conformance assertion (see below) — three independent loops is three chances to lose one — but nothing is
  wrong today.
- **The MTP suite under plain `cargo test`.** `mtp_test.rs` goes red in subsets (a stale process-global
  `connection_manager()` across fixtures) and green per-test. Not a defect: the checker runs `cargo nextest run`
  (process per test), and `virtual_device.rs` documents that as the supported mode. Worth knowing before you conclude
  the suite is broken.

## Recommended, not done

1. **Give `LocalPosixVolume` a mount-kind answer, resolved once.** `operations_are_local()` should be `false` for a
   network or file-provider mount. The shape that fits the existing constraints: resolve
   `filesystem_kind::detect_filesystem_for_path` ONCE at construction (discovery already runs on the `volume-init`
   helper thread precisely because that syscall can hang) and cache it in a field. `local_path()` and
   `supports_local_fs_access()` stay `true` — those are questions about `std::fs` reachability, and this is a question
   about cost. Not done here because "which kinds count as remote?" and "what happens when construction is on the main
   thread" are design calls, not clear-cut fixes.
2. **Give the delete's directory phase the honest count it already has a field for.** Tally the directories `remove_dir`
   / `delete_with_cancel` refused and put the number in `WriteCompleteEvent.files_skipped` instead of the hardcoded `0`,
   so the completion toast can say "3 folders couldn't be removed" rather than nothing. ❌ Don't fail the operation
   instead: a concurrently recreated `.DS_Store` would take the whole delete down with it.
3. **Measure FSEvents on an `smbfs` mount, then act.** The answer decides whether finding 2 is real. If FSEvents is
   silent there, `listing_is_watched` needs the same mount-kind answer as (1), and `SmbVolume::supports_watching`'s
   comment needs replacing with the measurement. Either way the claim gets an evidence anchor instead of a belief.
4. **The single highest-value fence: keep growing `volume::conformance`, not `checks/`.** A scanner check can only see
   syntax; these are semantic promises, and the assertion-every-backend-runs shape is what caught MTP's recursing
   `delete` and is what would catch the next one. The remaining unpinned trait promises worth a look, in order:
   `read_range` returns short ONLY at EOF (a mid-file short read silently corrupts remote-archive browsing);
   `open_read_stream` / `write_from_stream` "must stream" (peak memory bounded by the chunk buffer regardless of file
   size — assertable with a large sparse fixture and a `process_memory` reading); `list_directory` returns directories
   first, then files, alphabetically.
