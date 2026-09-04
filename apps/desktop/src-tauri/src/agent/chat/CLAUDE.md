# Agent chat (`agent/chat/`)

One message in, an answer out, crash-safe and within budget.

## Module map

- `context.rs` + `budget.rs`: the PURE core — the stable prefix, elide-only compaction, budget
  resolution, the ONE token estimator.
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`), `mod.rs` (`ChatRuntime::send_message`
  and `wake`, both single-flight), `events.rs` + `dispatch.rs` (the two seams).
- Leaves: `system_prompt.rs`, `session.rs`, `stream.rs` (`AskCmdrTurn`, a turn's one progress
  transport), and `cancel.rs` (the in-flight-turn registry, keyed by conversation, so a wake can
  register too).

## Must-knows

- **The prefix stays byte-identical across a thread's calls** (it buys prompt caching): `system`
  (fenced memory, the prompt, then `CMDR.md`) and the tool declarations never vary. The envelope
  rides the latest user turn only, snapshot-at-send, so ground truth can't shift mid-turn.
- **⚠️ Memory LEADS the system string and is fenced; ❌ never append it like `CMDR.md`.** It's the one
  part of the prefix an attacker can reach, so it precedes the rules, fenced, under a line calling it
  data. `DETAILS.md` § The memory block; `../memory/DETAILS.md`.
- **Keep the pure core pure.** Offset, envelope, `CMDR.md`, memory, and budget arrive as values, so
  every context test runs with no tokio runtime and no filesystem; `context.rs` never learns the model.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`).
  Handed a stub instead of content it was told to name files by, a model invents: that shipped,
  renaming 12 real files to fiction. Overrun honestly instead.
- **A context drop is never silent, and it revokes the dropped result's evidence.** `assemble_prompt`
  returns `ElisionFacts` as DATA (the core can't log); `runtime/turn.rs` warns, emits ONE
  `ContextTrimmed`, calls `revoke_evidence`. New compaction path ⇒ all three. Its stub stays
  shape-agnostic and under 80 tokens: a digest describes a delivery, never is one (invariant 6).
- **Content is written only on `End`; the user row on the FIRST `End`** (crash cases (a)–(d),
  red-guarded). Don't pre-persist either row.
- **A runaway loop is impossible by construction**: `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are checked at
  the TOP of the loop, and an identical repeat of a FAILED call isn't re-dispatched (`repeats.rs`).
  Don't bump one silently.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**, forwarded onto one
  conversation-keyed event: ❌ never key a turn to the invoke that started it, or a reload loses it.

The whole call, the constants, the crash cases, and the runtime file by file: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
