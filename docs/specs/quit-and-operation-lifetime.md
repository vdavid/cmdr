# Quitting safely, and operations that outlive their window

This is the prerequisite for `docs/specs/operation-session-plan.md` (its M0). That plan reasons from "an operation
outlives the view watching it", which is false today. This spec makes it true, and settles what quitting does.

## The intent

One root cause under two symptoms: **nothing owns an operation's lifetime except whichever webview happens to be
alive.** A window disappearing can end work it never owned, and the process can exit while a write is half-done with no
one deciding whether that was acceptable.

The end state: the backend owns operation lifetime and the quit decision. Windows are viewers. A viewer going away is
not an event an operation needs to know about.

## The defect as it stands

`apps/desktop/src/routes/(main)/+layout.svelte:282-284` registers a `beforeunload` handler calling
`cancelAllWriteOperations()`, which walks the GLOBAL registry (`write_operations/state.rs`, pinned by a test named
`cancel_all_write_operations_walks_the_global_registry`). Backgrounded operations included; operations the main window
has no view on at all, included.

Two things are wrong with it, and the second is the interesting one.

**It fires when it shouldn't.** Start a copy, press Queue, reload the main window, and the transfer dies while the queue
window sits there rendering a row for it. Under `pnpm dev` this happens on every HMR full reload
(`lib/hmr-recovery.ts:32`, the only `location.reload()` in the app and `import.meta.hot`-gated).

**It doesn't work when it should.** In production, reload is unreachable: SvelteKit routes client-side and never fires
`beforeunload`, and closing the main window doesn't unload it, it quits the app outright (`src-tauri/src/lib.rs:940-948`
turns `CloseRequested` on `main` into `app_handle().exit(0)`). So the only production path into this handler is the app
quitting, where it is `void`-ed fire-and-forget, nothing awaits it, and `RunEvent::Exit` (`lib.rs:980-1003`) doesn't
wait for it either. The safety net documented at `write_operations/DETAILS.md:197` is a race nothing arbitrates.

Note what `RunEvent::Exit` _does_ do: window geometry, live search walks, `ptpcamerad`, AI, MCP, mDNS, all torn down
synchronously in Rust. Write operations are the single app-wide resource delegated to a webview event. That asymmetry is
the whole bug.

## What quitting should do

Settled with David:

- If any non-instant operation is active (running, queued, paused, or waiting on a conflict answer), prompt. Nothing
  active, quit immediately, exactly as today.
- The dialog sits **above everything**, conflict dialogs included.
- It counts down from 20 and quits at zero, so an OS restart is never blocked by Cmdr: "Stuff is ongoing, really quit? …
  Cmdr will quit in 20, 19, 18…" with **[Don't quit]** and **[Quit]**.
- **"Don't quit" cancels the countdown entirely**, it is not a snooze. Otherwise a restart still kills the transfer 20
  seconds later, which is worse than not asking.
- Once Quit is pressed or the timer expires: transfers wrapped within ~1-1.5 s, app gone within 2 s.
- **No rollback.** Keep every fully-copied file. Cancel only the file currently in flight and remove its partial from
  the destination.

### The cancel semantics already exist

That last bullet is `OperationIntent::Stopped`, documented at `write_operations/DETAILS.md:146` as "stop immediately,
keep all fully-copied files, delete only the last partial file", `rolled_back: false`. The quit path calls
`cancel_write_operation(id, rollback=false)` and gets David's semantics unchanged. Nothing to design here.

## Can we actually hit two seconds?

Audited per execution path. The short answer is yes, but not by making workers interruptible. By making **abandoning a
worker safe**, and letting the quitter, not the worker, enforce the deadline.

House precedent for that split: `commands/util.rs:51-60`'s `blocking_with_timeout` wraps `spawn_blocking` in
`tokio::time::timeout` — the blocking thread keeps running, the _caller_ stops waiting. Cmdr already answers "this
syscall may never return" with abandonment everywhere else.

### Where each path stands today

- **APFS clone** (`macos_copy.rs:358`): one uninterruptible `copyfile()`, but O(1). Sub-millisecond on any size. Fine.
- **Local chunked copy** (`chunked_copy.rs:122`): cancel checked every 1 MiB. Healthy disk ~10 ms. **Against a hung
  Finder-mounted NAS the `read`/`write_all` at `:135`/`:144` are plain blocking calls with no timeout: unbounded.**
- **Linux `copy_file_range`** (`linux_copy.rs:71`): cancel checked per 4 MiB chunk, `:81` clamps every call. Fine.
- **Linux/other-platform safe-overwrite via `fs::copy`** (`copy_strategy.rs:193`, `:221` → `overwrite.rs:60`): **no
  cancel check at all, whole file uninterruptible.** macOS is exempt (`copy_strategy.rs:146` keeps the cancel context).
  Now that main ships Linux, this is a real path, not a hypothetical.
- **Cross-volume streaming** (`strategy.rs:469`): mid-file per chunk via the backend's `on_progress`. But the cancel is
  only seen after the in-flight chunk read _and_ write return, and SMB's own deadlines are 20 s to send plus 30 s of
  server silence. **Worst case ~30 s.** MTP reads an 8 MiB window, so ~1.6 s on a slow phone.
- **Concurrent driver** (`copy_concurrent.rs:715`): waits `CANCEL_DRAIN_DEADLINE` = 15 s (`copy.rs:61`) before
  abandoning its futures.
- **Renames and same-volume moves**: nothing in flight, instant.

### On adding timeouts to the local read/write

Asked for directly, and worth being precise: **a blocking regular-file read cannot be given a timeout.** `O_NONBLOCK` is
defined not to apply to regular files, and on a hung SMB/NFS mount the client blocks down in the VFS regardless.
`pthread_kill`-to-`EINTR` is defeated by macOS restarting most FS syscalls, and unwinding a thread mid-`write_all` is
not something we can make safe.

So a "timeout" here can only ever mean _stop waiting for that thread_, which is move (a) below. The spec should say so
plainly rather than carry a task that can't be completed as written. What we do get:

- The operation stops waiting on a deadline and reports itself wedged, instead of blocking the quit.
- With staging (Q1), the abandoned thread cannot damage anything: it is writing to a temp nobody will rename.

### The two moves

**(a) Make abandonment safe** — Q1, local temp+rename staging. Once every in-flight write lands on a `.cmdr-` temp,
abandoning a worker (a dropped future, or a thread wedged in a syscall we cannot interrupt) is always safe, just untidy.
This is the load-bearing change; everything else depends on it.

**(b) Make the deadline the quitter's, not the worker's** — Q2. Cooperative cancel first, hard abort on a timer.

### Cross-volume streaming: fixable, without touching the libs

Today `backend_cancel` is deliberately _not_ raced against writes, and the reason is written down
(`transfer/DETAILS.md:392-397`): dropping the future would skip each backend's own partial cleanup on the healthy cancel
path (`local_posix.rs:606-608`, `smb/streams.rs:459-462`). That reasoning is correct and must survive.

The fix respects it by adding a second tier rather than changing the first:

- **Tier 1, cooperative (unchanged).** Cancel travels through `on_progress`; the backend deletes its own partial. Stays
  the default for every user-initiated cancel. The happy path is untouched.
- **Tier 2, hard abort (new).** A separate `backend_abort` token raced in a `select!` around the chunk await. Fired only
  by the quit deadline (and later, if we ever revive the stall watchdog). Cleanup is delegated to the staging layer and
  the orphan sweep instead of the backend.

Cost on the happy path is one arm in a `select!` on an already-live token: no allocation, no per-chunk syscall, no
change to `smb2` or `mtp-rs`.

### Concurrent driver: a parameter, not a redesign

`CANCEL_DRAIN_DEADLINE` is a constant at `copy.rs:61`, and dropping the `FuturesUnordered` is already the implemented
fallback at expiry. Thread the deadline through as a parameter: 15 s for a normal cancel, ~1 s for quit.

### The honest verdict

With Q1 + Q2, **two seconds is a hard guarantee on process exit, and the disk is always left safe.** What we
deliberately do _not_ guarantee is that every worker thread observed the cancel: a thread wedged on a dead mount may
still be sitting in `read()` when the process dies. That is fine, and the spec should say it out loud rather than imply
a cleanliness we can't deliver. The user-visible contract is "the app quits in 2 s and nothing on disk is corrupt or
misleading", not "every thread wound down politely".

## Q1: local copies get temp+rename staging

The enabler, and independently the most valuable fix here.

**The problem.** Local copies write straight to the **final filename**. `chunked_copy.rs`, `macos_copy.rs`, and
`linux_copy.rs` have no staging; only the _overwrite_ case stages (`overwrite.rs:51`). Partial cleanup on cancel is
either a synchronous `remove_file` that needs the loop to get control back, or — in the chunked path — a **detached
thread** (`chunked_copy.rs:129` → `cancellable.rs:23`) that dies with the process.

So: copy a 4 GB file to an external drive, quit, deadline fires. You are left with a truncated file at the user's real
name, looking complete. That is the exact failure the cross-volume staging work was built to eliminate, still live on
the most ordinary path in the app. It is not a quit bug: crash and power loss produce it too.

**The fix.** Route every local write through the same temp + rename the overwrite path already uses, and register each
temp in `state.in_flight_temps` the way `staged_write.rs:96` does for cross-volume. Cost is one extra same-directory
rename per file, which is free on a same-FS destination. The rename must stay same-directory to remain atomic, which is
how safe-overwrite already does it.

**The orphan-sweep gap this exposes.** `reap_stale_transfer_temps` (`cleanup.rs:251`) only runs when something copies
_into_ that directory, and gates on `STALE_TEMP_MIN_AGE` = 1 hour (`cleanup.rs:34`). `in_flight_temps` is an in-memory
`Mutex<Vec<PathBuf>>` (`state.rs:113`) that dies with the process. So a quit-orphaned temp survives until you happen to
copy into the same directory an hour later. Persist the in-flight temp list as part of the quit teardown, and sweep it
at next launch with no age gate — those are provably ours, so the gate that exists to protect against a concurrent
instance doesn't apply.

- **Tests (TDD, real red first):** a cancelled local chunked copy leaves no file at the destination name; an abandoned
  local copy leaves a `.cmdr-` temp and not a real-name file; the persisted orphan list is swept at startup regardless
  of age. The existing `copy_crashsafe_tests.rs` is the natural home.
- **Docs:** `transfer/CLAUDE.md`'s safe-overwrite must-know (it currently scopes staging to overwrites),
  `transfer/DETAILS.md` § "Finding the litter", and the local-engine notes in `write_operations/DETAILS.md`.
- **Checks:** `pnpm check rust -q`, then `pnpm check rust-tests`.

## Q2: the hard-abort tier

`backend_abort` token, raced per chunk; drain deadline parameterized. Tier 1 untouched.

- **Tests:** a cross-volume copy against a deliberately stalled fixture volume aborts within the deadline (the
  `faulty_volume_test_support.rs` / `copy_wedge_test_support.rs` harnesses already build stalled backends); a normal
  cancel still routes through tier 1 and the backend still deletes its own partial (guard against regressing the
  documented reason).
- **Docs:** `transfer/DETAILS.md:392-397` currently states racing writes against cancel is "deliberately not done" —
  that passage must be rewritten to describe the two tiers and why tier 1 stays the default, not deleted.
- **Checks:** `pnpm check rust -q`, `pnpm check rust-tests`.

## Q3: the quit gate, with the countdown owned by Rust

**Why Rust owns the timer:** if the frontend drives a `setInterval` and the webview is wedged, the app never quits. A
wedged UI is a likely reason someone is quitting in the first place. The dialog is a looking glass here too.

Flow:

1. Rust intercepts the quit. Both entry points need it: `RunEvent::ExitRequested` (⌘Q, menu Quit) and the main-window
   `CloseRequested` branch at `lib.rs:940`, which today calls `exit(0)` directly and must instead route through the same
   gate.
2. Ask the manager for active non-instant operations. None → quit immediately, unchanged behavior.
3. Some → `api.prevent_exit()`, emit a `quit-requested` event carrying the operation list and the deadline, start a
   **Rust** timer.
4. The frontend renders the dialog and counts down for display only, then sends `quit_confirm` or `quit_cancel`.
5. Rust quits on whichever lands first: the confirm, or its own timer. `quit_cancel` stops the timer and releases the
   gate. **If the frontend never answers, the timer still fires.** That is the point.
6. On quit: cooperative cancel every operation (`rollback=false`) → wait up to ~1.5 s for terminal events → hard abort
   (Q2) → persist the orphan temp list (Q1) → exit.

The `beforeunload` handler is deleted **in this same milestone**, not before it, so there is never a window where
neither the old net nor the new gate exists.

- **Open, needs checking during build:** whether `DialogManager` can layer this above a modal conflict dialog, or
  whether it needs to be a native dialog / separate always-on-top window. Not yet investigated.
- **Open, flag to David if it bites:** macOS gives an app limited time to answer a logout or restart before the system
  complains. A 20 s countdown fits, but only just. If Tauri surfaces the logout case distinctly from ⌘Q, a shorter
  countdown there would be safer. Don't guess at the API; check and report.
- **Copy:** new user-facing strings, so catalog keys with `@key` descriptions and nine locales, and David reviews the
  wording before it ships.
- **Tests:** a quit with no active operations doesn't prompt; a quit with one does and blocks; `quit_cancel` releases
  the gate and stops the timer; the timer fires with no frontend answer at all (the wedged-UI case, the reason the
  design exists); a reload leaves backgrounded operations running (the original defect, red first).
- **Checks:** `pnpm check rust -q`, `pnpm check svelte -q`, then the full `pnpm check`, plus an E2E for the prompt.

## Sequencing

Q1 → Q2 → Q3, strictly. Q1 is what makes Q2's abandonment safe, and Q2 is what lets Q3 promise a deadline. Q1 is worth
landing on its own merits even if the rest slips: it closes a crash- and power-loss-safety hole that exists today.

## Risks

- **Dropping an SMB write future mid-frame** leaves the session in an indeterminate state. Acceptable at quit, since the
  session dies with the process. Tier 2 must therefore stay quit-only until someone thinks through session reuse.
- **Tier 1 regression risk.** The documented reason tier 1 exists (backends clean their own partials) is easy to erase
  by accident while adding tier 2. The test guarding it is named in Q2 for that reason.
- **Staging changes the local hot path.** An extra rename per file is cheap, but the local engine is the most
  performance-sensitive path in the app. Benchmark before and after with `copy_bench.rs`; if a many-small-files copy
  regresses measurably, say so rather than absorbing it quietly.
