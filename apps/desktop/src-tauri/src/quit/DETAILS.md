# Quit gate: architecture and decisions

Must-knows and the module map: `CLAUDE.md`.

## The problem this closes

Nothing owned an operation's lifetime except whichever webview happened to be alive. The main window's `beforeunload`
handler called `cancelAllWriteOperations()`, which walks the GLOBAL registry — so a dev hot-reload killed a
backgrounded transfer while the queue window still rendered a row for it, and on the production quit path the call was
a `void`-ed fire-and-forget that nothing awaited. Meanwhile `RunEvent::Exit` tore down window geometry, live search
walks, `ptpcamerad`, AI, MCP, and mDNS synchronously in Rust. Write operations were the one app-wide resource delegated
to a webview event. That asymmetry was the bug.

## The phase machine

`Phase::Idle` → `Waiting(sender)` → `Quitting`, under one mutex, with `Idle` reachable again only from `Waiting` (a
cancel).

- **`Idle` → `Waiting`**: `request_quit` found at least one operation matching `blocks_quit`. It emits `quit-requested`
  and spawns the deadline thread, which owns the whole rest of the flow.
- **`Waiting` → `Idle`** (`cancel`): the sender is dropped, so the thread's `recv_timeout` returns `Disconnected` and
  it stands down. A later ⌘Q gets a fresh countdown.
- **`Waiting` → `Quitting`**: either `confirm` (which sends `Decision::Quit`) or the thread's own
  `claim_deadline` after `recv_timeout` times out. Both are one atomic swap, which is what settles the
  cancel-lands-as-the-deadline-fires race: whoever takes `Waiting` out of the mutex wins, and the loser is a no-op.
- **`Quitting` → anywhere**: never. See "Why `Quitting` is terminal".

### Why `Quitting` is terminal

`tear_down_and_exit` ends in `AppHandle::exit(0)`, and Tauri turns that into a fresh `RunEvent::ExitRequested`. Aborted
operations are still registered at that moment (tier 2 stops *waiting*; it doesn't wait for the records to clear), so a
gate that asked again would prompt over the very operations it just abandoned — and would do it every time round.
`Quitting` makes every later request a pass-through. Pinned by
`tests::once_the_decision_is_made_every_later_request_sails_through`.

### Why an OS thread, not a tokio task

The deadline exists precisely for the case where things are stuck. A `tokio::time::sleep` in a spawned task is
schedulable behind whatever the runtime is already doing, and the runtime is exactly what a wedged transfer congests. A
`std::sync::mpsc::Receiver::recv_timeout` on a dedicated thread has no such dependency: the kernel wakes it. It also
keeps the whole flow synchronous and readable — receive, decide, tear down, exit — with no cancellation-safety story to
get wrong.

## The two clocks

- **`COUNTDOWN` = 15 s**, the user-facing one. macOS gives an app a limited window to answer a logout or restart before
  the system complains or cancels the restart. Tauri surfaces no signal separating that case from a plain ⌘Q:
  `RunEvent::ExitRequested` carries only an exit code, `None` for any user-driven quit, so there is nothing to branch
  on and the single countdown has to fit the strictest case. 20 s plus the ~2 s teardown sat too close to that window;
  15 + 2 leaves margin. **If Tauri ever surfaces the logout case distinctly, a shorter countdown there is the upgrade.**
- **`DRAIN` = 1.5 s**, how long the teardown waits for the cooperative cancel before firing tier 2. Polled every 20 ms
  (`DRAIN_POLL`), so an operation that answers promptly isn't waited out — pinned by
  `tests::a_cooperative_cancel_that_lands_skips_the_rest_of_the_drain`.

The 2 s budget is `DRAIN` plus a tier-2 abort (token flips, no I/O), a ledger flush (no syscall today), and the exit.

## The teardown's order, and why each step

1. **Cooperative cancel, `rollback = false`** (`cancel_all_write_operations`). This is `OperationIntent::Stopped`: every
   fully-copied file is kept, only the file in flight loses its partial. ❌ No rollback anywhere on the quit path — a
   teardown must never silently delete files with no visual feedback.
2. **Wait, bounded by `DRAIN`.** The healthy case ends here, with each backend having cleaned up its own partial (tier
   1's whole reason for existing, `transfer/DETAILS.md` § "Two tiers of cancel").
3. **`abort_all_write_operations`** (tier 2) for whatever didn't answer. It stops *waiting* rather than asking again, so
   a dead SMB mount can't hold the exit. The abandoned bytes become the staging layer's problem, which is safe because
   of Q1.
4. **`flush_in_flight_temps`.** Every temp is recorded at registration (before its first byte), on a bare `File` with
   no user-space buffer, so this is a fence rather than a repair — but naming it keeps a future `BufWriter` from
   quietly dropping the last records the next launch's sweep needs.
5. **`AppHandle::exit(0)`**, which runs the existing `RunEvent::Exit` teardown (window geometry, search walks,
   `ptpcamerad`, AI, MCP, mDNS).

**What this deliberately does NOT promise:**

- **That every worker thread observed the cancel.** A thread wedged in `read()` on a dead mount may still be sitting
  there when the process dies. The user-visible contract is "the app quits in 2 s and nothing on disk is corrupt or
  misleading", not "every thread wound down politely".
- **That the 2 s covers a wedged Tauri event loop.** Step 5 posts to it, so a main thread that has stopped turning
  would swallow the exit. A wedged WEBVIEW can't cause that — the event loop is Rust's, and the deadline thread doesn't
  touch either — so the case this feature exists for is covered; a wedged event loop is a different bug, and the
  hammer for it (`std::process::exit`) would skip `RunEvent::Exit` and leave the user's `ptpcamerad` disabled. Not a
  trade worth making blind.
- **That the prompt is always seen.** A quit requested before the main window's `onMount` has wired the listener gets
  no dialog. The countdown still runs and the app still quits correctly; the user just doesn't get asked. The listener
  is registered first thing in `onMount` (ahead of the awaited setup) to keep that window as small as it can be.

## What counts as blocking the quit

`blocks_quit` takes the manager's snapshot and keeps a row when it is BOTH still going (`Queued` / `Running` /
`Paused`) and moves bytes (`Copy` / `Move` / `Delete` / `Trash` / `ArchiveEdit`). Notes:

- **A conflict prompt needs no arm of its own**: an operation waiting on an answer is still `Running`.
- **Instant metadata ops never block** (`Rename` / `CreateFolder` / `CreateFile`, the `run_instant` family). They
  finish faster than a human could read a dialog about them, and a `Running` record for one is often already stale.
- **Retained failures never block**: they're `Failed`, and there's nothing left to lose.

Both matches are written exhaustively so a new variant is a compile error here rather than a silent default into
"killable at quit".

## Testing it

`tests.rs` drives the real gate, the real thread, and the real channel against a `RecordingHost`, with the two
durations shrunk. That's what lets a test exercise the deadline without ending the test process — the trait's `exit` is
a recorded call, not `AppHandle::exit`.

The filtering policy lives in the GATE, not the host, so `RecordingHost` hands over an unfiltered registry and the
instant-op and settled-op cases are ordinary unit tests.
