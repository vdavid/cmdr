# Deadlines and blocking budgets: details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows.

## Inventory

- **`TimedOut<T>`** (`{ data, timedOut }`) and **`DeadlineError`** (`TimedOut` / `Unexpected`): the two timeout-aware
  answer shapes that cross IPC.
- **`blocking_with_timeout`**, **`blocking_with_timeout_flag`**, **`blocking_typed_result_with_timeout`**: run a closure
  on the blocking pool under a timeout, answering with a fallback, a flagged fallback (`TimedOut<T>`), or the caller's
  own typed error.
- **`blocking_typed_result_with_timeout_on`**: the typed one on a HELD `rt::Runtime` (`runtime.spawn_blocking`), for an
  engine that holds its host's runtime instead of assuming an ambient one. Natively that's the runtime's own blocking
  pool, so hand in the runtime the ambient call would have used; in the browser the closure runs inline and answers
  before any deadline. The one `deadline` helper that runs in the browser build with blocking work.
- **`timeout_detached_typed`**: runs a FUTURE in its own task and races that task's join handle, so expiry detaches the
  work rather than dropping it.
- **`race_detached`** (`Raced::Answered` / `Raced::StillRunning(handle)`): the same race, but expiry hands the caller
  the running task, for a write whose real end must still be reported (a `TimedOut` there is disproven seconds later).
  Its caller is `cmdr_operations::reply_within` (`crates/cmdr-operations/DETAILS.md` § "A slow instant mutation says it
  is still running").
- **`Deadline`** (`elapsed` / `remaining` / `total` / `fraction`) + **`timeout_detached_within`**: one wall-clock budget
  across a command's legs. A leg that starts with nothing left doesn't start.
- **`io_budget`** + **`SESSION_IO_TIMEOUT`** (the app's `file_system::volume::io_budget_for_volume` reads the state):
  stretch a tier to at least 10 s on a volume Cmdr holds a live session to (`ConnectionState::Direct`), and leave every
  other volume on its tier. The session's transport tells slow from dead on its own (smb2 declares a silent server dead
  and the volume goes `Disconnected`), so a short timer there only turns a busy NAS's 0.3–6 s stall into a wrong answer.
  ❌ Not for `OsMount` or MTP: nothing under them can tell, and a wedged kernel mount blocks for minutes. Callers:
  `rename.rs`'s validity check and rename, `path_exists` / `destination_exists`.
- **`BlockingBudget`**: a semaphore capping one command family's share of the blocking pool. Callers past the cap wait
  as futures, not threads.
- **`blocking_typed_result_until_stalled`** + **`StallWatch`**: no total deadline; it gives up once the watch reports
  the work idle for the stall limit, and detaches like the deadline helpers. Its callers are the two viewer operations
  whose honest duration has no ceiling but whose silence does: a pulling open
  (`apps/desktop/src-tauri/src/file_viewer/DETAILS.md` § "Watching a pull") and a save of a selection (same file, §
  "Gotchas").

## Callers

Every filesystem-touching IPC command in the app (`commands/`), plus subsystems that do blocking I/O on a deadline of
their own: the SMB mount reads and one-shot credential writes in `network/`, `reveal/delivery.rs`, and the write
engine's `conflict_preflight.rs` and `mutation_reply.rs`. It lives in `cmdr-fs`, below the IPC layer and every
subsystem, so none of them depends on `commands` for it, and a Tauri-free crate can wait the same way the app does.

## Decision: timeout-aware return types

A plain fallback is indistinguishable from a real empty or none result ("no volumes mounted" vs "timed out before
listing volumes"). `TimedOut<T>` carries the distinction for non-`Result` returns; the bare `blocking_with_timeout`
stays for the rare read where it genuinely doesn't matter. A `Result` return carries it as a VARIANT of the command
family's own error enum (`MutationError::TimedOut`, `EjectError::TimedOut`, `DeadlineError::TimedOut`), which is why
`timeout_detached_typed` takes an `on_timeout` that mints the caller's type. Why no shared error struct:
`docs/guides/error-handling.md`.
