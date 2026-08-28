# Quit gate

The backend owns the decision to exit. Quitting with work in flight asks the user, counts down on a Rust timer, then
stops everything and exits inside a hard budget.

## Module map

- `mod.rs`: `QuitGate` (phase machine + deadline thread), `blocks_quit` (the policy), `tear_down_and_exit` (the
  ordering), `TauriQuitHost` (the real outside world), the `quit-requested` and `quit-called-off` events.
- `commands.rs`: `quit_confirm` / `quit_cancel`, both thin, both returning the gate's `QuitAnswer`.
- Frontend counterpart: `apps/desktop/src/lib/quit/CLAUDE.md`. MCP surface (the `quit` tool and the two answers):
  `../mcp/executor/quit.rs`.

## Must-knows

- **The countdown is Rust's; the dialog only displays it.** A frontend `setInterval` never fires in a wedged webview,
  and a wedged UI is a likely reason someone is quitting. ❌ Never move the authority to the frontend. Pinned by
  `tests::the_deadline_fires_when_the_frontend_never_answers`.
- **It runs on a dedicated OS thread, not a tokio task**, so a saturated runtime can't delay the one timer whose job is
  firing when other things are stuck.
- **Every entry point routes here** (`app_lifecycle.rs`): `RunEvent::ExitRequested` (⌘Q, menu, dock, logout, every
  `AppHandle::exit`) and the main window's `CloseRequested`, which must `api.prevent_close()` when the gate holds — a
  closed main window takes the dialog with it. ❌ Don't tear down AI / MCP / mDNS before asking: a "Keep working" leaves
  the app running. **❌ Never `AppHandle::exit` past the gate**: the MCP `quit` tool did, so an agent killed a running
  transfer with no prompt where ⌘Q asks.
- **`confirm` / `cancel` report what they DID** (`QuitAnswer`), because an answer can arrive after the deadline claimed
  the decision and the caller may not be the window watching the dialog. `cancel` also emits `quit-called-off`: nothing
  else takes a prompt down when the answer comes from MCP, and one counting toward a quit that will never come is a lie.
- **`Phase::Quitting` is load-bearing.** The teardown ends in `AppHandle::exit(0)`, which comes straight back as
  `ExitRequested`; without the phase the gate would prompt again over the operations it just aborted, forever.
- **A restart (`RESTART_EXIT_CODE`) never reaches the gate**: Tauri ignores `prevent_exit` there, so asking would show
  a dialog nobody could answer.
- **Classify by TYPED variant, never a string**. Both matches in `blocks_quit`
  are exhaustive so a new operation type or lifecycle status has to declare its side.
- **"Don't quit" deletes the countdown, it doesn't defer it.** A snooze would still kill the transfer seconds later,
  which is worse than not having asked.
- **The teardown's order is the contract**: cooperative cancel (no rollback) → wait up to `DRAIN` → tier-2 abort →
  fence the temp ledger → exit. Total budget from decision to process gone is **2 s**, `COUNTDOWN` is 15 s.

Architecture, the budget's arithmetic, and why 15 s: `DETAILS.md`.
