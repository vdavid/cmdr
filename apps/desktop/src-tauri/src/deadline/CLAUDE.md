# Deadlines and blocking budgets

Timeouts and budgets for blocking work (`mod.rs`), shared by IPC commands and the subsystems under them (network,
reveal, write operations). Tests: `budget_tests.rs`, `stall_tests.rs`.

## Must-knows

- **Every helper detaches on timeout; ❌ never add one that drops the work.** A bare `tokio::time::timeout` around a
  future cancels it wherever it stands, which abandons an in-flight MTP transaction and wedges the phone. The helpers
  race a JOIN HANDLE instead: the caller gets its answer, and the work runs on to its own end.
- **A timeout is a typed answer**: `TimedOut<T>` for a non-`Result` return, or the caller's own error variant minted
  by `on_timeout`. `DeadlineError` is the one shared type, and only for work that can't refuse.
- **Multi-leg work shares ONE `Deadline`** (`timeout_detached_within`), and an optional leg takes
  `Deadline::fraction` so it can't starve the leg that answers.
- **`BlockingBudget` is one `static` per contended resource, shared by every caller that hits it**, and takes its
  permit before spawning. An unbounded command once took all 512 blocking-pool threads and froze the app.

Helper inventory, callers, and the timeout-type decision: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
