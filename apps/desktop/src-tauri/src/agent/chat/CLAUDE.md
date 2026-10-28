# Agent chat (`agent/chat/`)

One message in, an answer out, crash-safe and within budget.

## Module map

- `context.rs` + `budget.rs` + `effort.rs`: the PURE core — the stable prefix, elide-only compaction, budget
  resolution, reasoning effort and the reply's room, the ONE token estimator.
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`), `mod.rs` (`ChatRuntime::send_message`
  and `wake`, both single-flight), `events.rs` + `dispatch.rs` (the two seams).
- Leaves: `system_prompt.rs`, `session.rs`, `slot_status.rs` (what the rail knows before a send:
  provider readiness and the gauge's baseline), `stream.rs` (`AskCmdrTurn`, a turn's one progress
  transport), `message_view.rs` (one stored row's display projection, shared by history and the
  stream; `message_view/thread.rs` projects a history page with its turns in view), and
  `cancel.rs` (the in-flight-turn registry, keyed by conversation, so a wake can register too).

## Must-knows

- **The prefix stays byte-identical across a thread's calls** (it buys prompt caching): `system`
  (fenced memory, the prompt, then `CMDR.md`) and the tool declarations never vary. Each user turn
  carries its envelope as a persisted `AgentPart::Envelope`, rendered once at send and replayed
  verbatim: ❌ never re-render a stored one (it would bust the cache and move ground truth).
- **⚠️ Memory LEADS the system string and is fenced; ❌ never append it like `CMDR.md`.** It's the one
  part of the prefix an attacker can reach, so it precedes the rules, fenced, under a line calling it
  data. `DETAILS.md` § The memory block; `../memory/DETAILS.md`.
- **Keep the pure core pure.** Offset, envelope, `CMDR.md`, memory, and budget arrive as values, so
  every context test runs with no tokio runtime and no filesystem; `context.rs` never learns the model.
- **Automatic spends the whole window** (less the reply and a 15% margin), from the provider's report, else the
  family table. ❌ Don't re-add a cap for cost: that's compaction's job (planned).
  `DETAILS.md` § Resolving one budget.
- **Only budget pressure elides, oldest turn first; ❌ never by age.** While the prompt fits, history goes out
  byte-identical to the last call, which is what keeps the provider's prompt cache valid. `DETAILS.md` § Budget
  enforcement lists what still rewrites earlier bytes.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`).
  Handed a stub instead of content it was told to name files by, a model invents: that shipped,
  renaming 12 real files to fiction. Overrun honestly instead.
- **A context drop is never silent, and it revokes the dropped result's evidence.** `assemble_prompt`
  returns `ElisionFacts` as DATA (the core can't log); `runtime/turn.rs` warns, emits ONE
  `ContextTrimmed`, calls `revoke_evidence`. New compaction path ⇒ all three. Its stub stays
  shape-agnostic and under 80 tokens: a digest describes a delivery, never is one (invariant 6).
- **Content is written only on `End`; the user row on the FIRST `End`** (crash cases (a)–(d),
  red-guarded). Don't pre-persist either row.
- **Runaway loops are impossible**: both caps are checked at the loop's TOP (the round cap buys ONE
  tool-less wrap-up call); a FAILED call's identical repeat isn't re-dispatched (`repeats.rs`). ❌ No
  token total gates a turn.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**, forwarded onto one
  conversation-keyed event: ❌ never key a turn to the invoke that started it, or a reload loses it.
- **Terminal events come only from `run_turn`, once per turn** (a panicking rail turn's guard is the
  backstop), carrying its stats; `drive`
  returns how it ended. A tool result row persists BEFORE its `ToolCallFinished`, and a line is
  `(message_id, call_id)`: ids collide across rounds. `DETAILS.md` § What the stream says.

The whole call, the constants, the crash cases, and the runtime file by file: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
