# Transfer wedge: observability, recovery, and size-formatting correctness

**Status**: specced, not started. **Owner**: David. **Date**: 2026-07-31.

On 2026-07-31 a 764-file copy from a Dropbox File Provider folder to an SMB NAS share stopped dead after 12 files,
ignored Rollback, could not be dismissed, and had to be force-quit, leaving two byte-incomplete files at their final
names on the NAS. **The root cause is still unknown.** Two `sample` runs, the full debug log, and a live filesystem
inspection could not name the await that never resolved.

Evidence, timeline, and the four open questions: `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`. Read it
before starting M1; it is the acceptance target.

Read before starting: `apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md` and its
`DETAILS.md` (the driver, `CheckpointStream`, the foreground auto-yield contract), plus
`apps/desktop/src-tauri/src/file_system/CLAUDE.md` (the "never rayon for macOS frameworks" rule that shapes M4).

## Settled decisions

These are decided. Don't relitigate them while implementing.

1. **Observability ships first, before any fix.** Three of the fixes below treat symptoms of a cause we cannot name. A
   stall detector that reports "this stalled" tells the user what a frozen progress bar already told them. If the next
   occurrence yields the same zero evidence, we have not moved.
2. **The acceptance test for M1 is the incident's four open questions.** Not "we added logging". If a replay of the
   incident would not answer all four, M1 is not done.
3. **M2 and M3 do not wait for a root cause.** Both are correct regardless of why a transfer wedges, and both address
   what actually hurt: unrecognizable corrupt files, and no way out but force-quit.
4. **A byte-incomplete destination file must never sit at its final name.** This is the data-safety invariant the
   incident violated. Every cross-volume file write lands on a `.cmdr-tmp-*` and is renamed only after the last byte,
   whether or not a conflict made it a safe-replace.
5. **Cancel and Rollback must work while the driver is parked.** An escape hatch that only works when the thing it is
   escaping is healthy is not an escape hatch.
6. **One canonical size formatter, one canonical speed formatter, one canonical ETA.** Duplicated implementations are
   how the two windows came to disagree. Enforce with types and a lint, not with review discipline.
7. **Don't fix the sync-status leak by widening the timeout or the thread budget.** The defect is that cancelled work
   keeps running and keeps its threads. Fix the lifetime, then the fan-out.

## M1: make a wedge diagnosable (do this first)

The whole of what the log said about a 20-minute production wedge: a task's spawn, its stream open, then silence.

- **M1.1 Per-task lifecycle logging in the transfer driver.** Every copy task logs spawned, stream opened, chunk N
  written (throttled), each checkpoint park with its reason (user pause / source yield / destination yield) and its
  resume, finalize, and completion. Today a task that stopped mid-stream is indistinguishable from a slow one.
- **M1.2 A driver-side stall watchdog that dumps state.** After N seconds with no byte movement, log one structured
  record: every in-flight task with what it is awaiting and its bytes done, the `in_flight_partials` set, the
  `OperationIntent`, the pause gate, and the free concurrency slots. This single record answers open questions 1 and 2.
  It is also the substrate M5 renders in the UI, so build it here and let M5 be presentation only.
- **M1.3 Outstanding-request accounting in the `smb2` client.** `smb2::client::connection` logs a `dispatch:` line only
  on the `ChangeNotify` path, so Create and Write are invisible; open question 3 is unanswerable today. Track dispatched
  versus answered per `msg_id`, and warn periodically on any request outstanding beyond a threshold, with `cmd`,
  `msg_id`, and age. Lives in the `smb2` crate (`~/projects-git/vdavid/smb2`).
- **M1.4 Give `file_system::sync_status` logging at all.** It has none, which is why 23 wedged threads left no trace.
  Batch entered and exited with duration and path count, and a warning when a batch outlives the IPC timeout that
  abandoned it.
- **M1.5 Log the cancel and rollback path.** No `rolling back op=` line was ever written, leaving open question 4
  ambiguous between "the intent never arrived" and "the driver never observed it". Log intent transitions at the IPC
  edge, at the manager, and at the driver's observation point, so the gap is visible.

## M2: no byte-incomplete file at its final name

A new-file copy has no conflict, so it takes no safe-replace temp and streams straight to the destination path. The
incident left `sms-20260726002817.xml` at zero bytes and `sms-20260725002819.xml` truncated at exactly 4 MiB, both at
their final names, indistinguishable from complete files after the force-quit. These were phone backups.

- **M2.1** Route every cross-volume file write through a `.cmdr-tmp-*` and a post-write rename, not just the
  conflict-driven safe-replace path.
- **M2.2** Confirm the existing `.cmdr-` crash-recovery sweep collects these on next launch, and extend it if the new
  temps land outside its scope.
- **M2.3** Tests: kill a transfer mid-stream and assert the destination directory contains no partial at a final name,
  for both the fresh-copy and the overwrite path.

Note the interaction with M3: rollback must clean these temps, and M2 makes that job easier by giving every partial a
recognizable name.

## M3: cancel and rollback that survive a parked driver

David clicked Rollback, nothing happened, the window would not close, and the app had to be force-quit. The stall was
recoverable in principle; this is what turned it into data loss.

- **M3.1** Make the driver observe `OperationIntent` on the await path, not only in the spawn loop. Today
  `is_cancelled` is checked while pushing new tasks, which a parked driver never reaches.
- **M3.2** Make in-flight copy tasks abortable while parked or awaiting a backend, so cancel does not depend on them
  completing first.
- **M3.3** Bound the wait: if tasks do not wind down within a deadline, abandon them, mark the operation failed, and
  leave the partials for M2's cleanup rather than hanging forever.
- **M3.4** The transfer dialog and the Transfers window must always be dismissable, whatever the backend is doing.
- **M3.5** Tests covering cancel and rollback against a deliberately wedged backend task.

## M4: the sync-status thread leak

Confirmed independently in both samples: 21 to 23 OS threads permanently blocked in
`sync_status::get_ubiquitous_bool` -> `NSURL getResourceValue` -> synchronous XPC to `fileproviderd`, still blocked
four minutes later. `commands/sync_status.rs` wraps the call in a 2 s timeout, but `spawn_blocking` work cannot be
cancelled: the timeout returns an empty map while the `std::thread::scope` holds a Tokio blocking thread plus ~11
spawned 8 MB-stack OS threads until the provider answers, and the frontend retries into another batch. Two rounds were
in flight when sampled.

Unknown whether this contributed to the transfer wedge. It is worth fixing on its own: this is the
resource-efficiency win.

- **M4.1 Make the work cancellable.** A cancellation token the chunk loop checks between paths, so an abandoned batch
  stops instead of running to completion for a caller that timed out 2 s ago.
- **M4.2 Cap concurrent in-flight batches.** A second request while one is in flight should join or supersede it, not
  start another fan-out. This alone would have halved the observed thread count.
- **M4.3 A long-lived, bounded worker pool instead of per-call `std::thread::scope`.** Same 8 MB stacks (the rayon
  prohibition in `file_system/CLAUDE.md` stands), but spawned once. Per-call thread creation at 8 MB a piece, several
  times a second during scrolling, is the resource cost David wants back.
- **M4.4 Cache per path with invalidation.** Sync status changes rarely; the current code re-queries every path on
  every listing render.
- **M4.5 Cheaper negative path.** This folder is a worst case: with no dataless files, all 764 paths miss the `stat`
  shortcut and take the XPC path. Skip the NSURL query where the answer cannot be interesting, for example outside a
  known File Provider domain root. **Needs a design decision first**: `file_system/file_provider.rs` (which held the
  `domain_id_for_dir` hint) has moved to `indexing/scanner/file_provider.rs` under
  `index-crate-extraction-plan.md`, so the hint now sits inside the tree becoming the `cmdr-index` crate and is not
  ours to reach into. Either the probe gets duplicated app-side or it belongs in `cmdr-fs` as shared vocabulary; that
  is the extraction effort's call, so ask before designing around it.
- **M4.6** Measure before and after: thread count, wall time, and CPU for a 764-file Dropbox folder.

## M5: stall detection in the UI

Falls mostly out of M1.2; this milestone is presentation.

- **M5.1** When no bytes move for N seconds, say so instead of showing a confident ETA. The dialog claimed "~8m 12s
  remaining" throughout a total stall.
- **M5.2** Offer the way out (M3) from that state, and point at the log.
- **M5.3** Copy follows `docs/style-guide.md`: conversational, actionable, never the words "error" or "failed".

## M6: one way to display a size, a speed, and an ETA

### The root cause, confirmed

Both windows render sizes with the same correct `$lib/ui/Size.svelte` component. They disagree anyway, because
**`initReactiveSettings()` is called only in `routes/(main)/+layout.svelte`**. The Transfers window is
`routes/queue/+page.svelte`, outside that group, so it never loads settings and falls back to the module-level default
`fileSizeFormat = 'binary'` (`reactive-settings.svelte.ts:31`) forever. David's setting is SI, hence 83.65 MB in the
copy dialog and 79.78 "MB" (actually MiB) in the Transfers window for the same byte count.

This is not a size bug. **Every reactive setting silently falls back to its default in every secondary window**:
`queue`, `viewer`, `settings`, `shortcuts`, `debug`, and `dev` all skip the initializer, so `uiDensity`,
`dateTimeFormat`, `appColor`, `stripedRows`, and the rest are wrong there too, in ways nobody has noticed yet.

- **M6.1 Initialize reactive settings in every window, once, from a shared place**, so no route can forget. Make
  forgetting impossible rather than documented.
- **M6.2 Audit the other secondary windows** for settings-dependent rendering that has been silently wrong.
- **M6.3 A regression test that fails if a window route renders settings-dependent UI without initialization.**

### The rogue formatters

`<Size>` is the house primitive and most call sites use it. These reimplement byte formatting privately and are all
hardcoded to base 1024 while labelling the result "KB" / "MB" / "GB":

- `lib/tauri-commands/write-operations.ts:256` `formatBytes()`, re-exported from `tauri-commands/index.ts` and used by
  `settings/sections/AiLocalSection.svelte`.
- `lib/query-ui/query-filter-state.svelte.ts:103-134` (parse and format).
- `routes/debug/DebugDriveIndexPanel.svelte:119`.
- `lib/file-operations/transfer/TransferConflictDialog.svelte:36-39` (threshold-based CSS class, not a label, but the
  same duplicated tier logic).

- **M6.4** Delete each and route through the canonical formatter, keeping `<Size>` as the component form.
- **M6.5 Speed and ETA get the same treatment.** The dialog showed 58.41 MB/s (instantaneous) while the backend
  reported a decaying cumulative average (928 KB/s falling to 749 KB/s as the stall dragged on), and the two windows
  showed ETAs of 8m 12s and 5m 46s for the same operation. Decide one speed definition and one ETA definition, compute
  them in one place, and have both windows render that.
- **M6.6 Fix the file counter.** It read 5 of 764 while 10 files were fully written to the NAS. Establish where the
  count is incremented relative to the write completing, and make it honest.

### Type safety

David's ask: make a future divergence impossible rather than merely reviewed.

- **M6.7** A byte-count type that cannot be rendered without going through the formatter, so a bare `number` can't
  reach the DOM as a size. Rust side: a `ByteSize` newtype crossing IPC. TypeScript side: a branded type whose only
  consumer is the formatter.
- **M6.8** A lint (the repo already has custom `cmdr/*` ESLint rules and Rust checks) that rejects new private byte,
  speed, or duration formatting, on the model of `cmdr/no-error-string-match`.
- **M6.9** Document the primitives where they will be found: the units contract in the colocated `CLAUDE.md`, and a
  pointer from `docs/guides/building-ui.md` next to the other house primitives. David's requirement is that these be
  "easy-to-find, documented, standard utility functions", so discoverability is part of done, not a follow-up.

## Sequencing

M1 first and alone; land it and get a wedge reproduced or caught in the wild if we can. M2 and M3 next, in parallel if
convenient, since neither depends on a root cause. M4 independently. M5 after M1.2 exists. M6 last: it is the largest
surface and the lowest severity, and M6.1 is a one-line-shaped fix with a wide blast radius that deserves its own
careful pass.

### Running alongside the index crate extraction

`index-crate-extraction-plan.md` is in flight on `worktree-david-index-crate-extraction`. The code surfaces do not
overlap: of its 334 changed files, exactly one (`volume/backends/smb_watcher/archive_refresh_test.rs`) is in this
spec's working set, and none are in M6's frontend set. `VolumeError`, `ignore_poison`, `pluralize`, and `thread_qos`
have already moved to `cmdr-fs` without needing changes in any transfer file, so the app-side re-exports hold.

- **M4.5 is blocked on their decision**, as noted above.
- **Hold M6.7 and M6.8 until the extraction lands.** `FileEntry` in `cmdr-fs` already carries `display_size` and
  `display_size_tooltip`, so a byte-count newtype would be redesigning a type that effort owns while it owns it.
  M6.8's lint registers in `scripts/check/checks/registry.go`, which they are reworking.
- **Expect mechanical conflicts only in shared infrastructure**: `registry.go` and its neighbours, the generated
  `apps/desktop/src/lib/ipc/bindings.ts` (regenerate, never hand-merge), `docs/specs/index.md`, and `Cargo.toml` if
  M1.3 bumps `smb2`.
- **If both are ready together, let the extraction land first.** Rebasing localized additions onto a 334-file move is
  much cheaper than the reverse.

## Open questions for David

1. **M1 verbosity.** Per-chunk logging on a 764-file transfer is a lot of lines. Debug-level and off by default, or
   always on for the file target given it is the only target that mattered here?
2. **M3.3's deadline.** Abandoning wedged tasks means abandoning open SMB handles. Acceptable, or prefer to hold and
   report rather than risk server-side handle churn?
3. **M6.7 scope.** The byte-count newtype touches a lot of signatures. Whole app in one pass, or start at the transfer
   surfaces and widen?
