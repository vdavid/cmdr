# What the wake loop deliberately left for later

The proactive agent shipped end to end: it notices what changes on disk, decides whether that is worth saying, proposes,
remembers, and hears what the user did with each suggestion. Every design decision lives beside the code
(`apps/desktop/src-tauri/src/agent/wake/`, `agent/memory/`, `agent/suggested_ops/`, and the rail's docs under
`apps/desktop/src/lib/ask-cmdr/`). What follows is only the work that was named and consciously not done.

Each item says what it costs and what would trigger it. None of them blocks anything shipped.

## Waiting on real use, not on effort

- **The two interest tuning knobs stay guesses.** `UNKNOWN_IMPORTANCE_WEIGHT` at 0.35 and the hot/warm thresholds at 0.7
  and 0.3 (`agent/wake/interest.rs`) were picked before anyone had used the feature. The cadence slider made the DELAYS
  a user choice, which is the part a user can feel; the thresholds decide which folders qualify at all, and moving them
  on intuition would be guessing twice. **What unblocks it**: the per-outcome counted log line and the anonymous
  analytics event that ship with the runner. Read a week of real wakes, then rank. ❌ Don't tune from a single support
  message.
- **Three constants in the same class**, all one-liners to change once there is evidence: the declined-wake backoff (5
  min) and the idle poll (60 s) in `agent/wake/writer.rs`, and the outcome ring's 4 KB / 40 entries in
  `agent/memory/outcomes.rs`.

## Named and not built

- **Reading file contents.** The agent proposes from names, sizes, dates, and folder importance. A PDF reader (and
  friends) would let it answer "what IS this document" rather than "what does this filename suggest". Its cost is not
  the parser: every byte read becomes prompt, and `agent/chat/budget.rs` already prices a small local window tightly.
  Wants a design pass on what a summary costs before any parsing lands.
- **Per-rule approval for a long job's tail** is a policy question, not a task. It lives in `open-decisions.md`.
- **A thread-timeline event when the chat memory size changes.** The thread logs `ModelChanged` honestly through two
  cooperating paths, but a budget change gets no equivalent, so a user who shrinks their window mid-thread sees no note
  explaining why the replies changed. About half a day, unblocked, and independent of everything above. The channel
  enums are hand-mirrored in TypeScript, so both sides need the arm.
- **The rail does not refetch on a decision.** `SuggestionsChanged` fires on every approve and reject, but the rail does
  not subscribe, so an approve/reject line reaches an open thread on next load rather than live. Same documented
  limitation the wake digest has. A naive subscription would refetch for every decision whether or not it concerns the
  open thread; the fix wants the conversation-keyed filter the turn stream already uses.

## One chore that needs a machine with a foreground

- **`pnpm i18n:shots` has never run against the new consent copy.** It refuses when another app holds the front
  position, so it cannot run on the headless agent box. The `askCmdr.consent.*` keys carry
  `@key.screenshot: ask-cmdr-consent.png` by hand, which is correct for the surface they render on and keeps
  `message-screenshots-fresh` green, but they read as uncoupled in the generated `coverage-report.md` until a capture
  runs.
