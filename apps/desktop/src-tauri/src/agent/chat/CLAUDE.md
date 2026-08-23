# Agent chat (`agent/chat/`)

One message in, an answer out, crash-safe and within budget. One call end to end, the constants, the
crash cases, the numbers: `DETAILS.md`.

## Module map

- `context.rs`: the PURE core (values in, prompt out) — the stable prefix, elide-only compaction,
  budget enforcement. `assemble_prompt` is the entry; `context/digest.rs` describes a dropped
  result.
- `budget.rs`: budget resolution (family windows, the user's size, the local window, the batch
  hint, a wake's digest share, memory's slice) + the ONE token estimator. Pure.
- `system_prompt.rs`: the identity + rules string (part of the cached prefix).
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`), `mod.rs` (`ChatRuntime`:
  `send_message` and `wake`, both single-flight), `events.rs` + `dispatch.rs` (the two seams),
  `cost.rs` (metering).
- `session.rs`: what a turn needs from live app state — the LLM slot, the budget, the envelope.
- `cancel.rs`: the one in-flight-turn registry, keyed by conversation. Here, not in `commands/`,
  so a WAKE can register too.
- `stream.rs`: the one transport a turn's progress leaves on (`AskCmdrTurn`), its wire enum, and its
  projection from `AgentChatEvent`. Shared by the rail and by a wake.

## Must-knows

- **The prefix stays byte-identical across a thread's calls** (it buys prompt caching): `system`
  (fenced memory, the prompt, then `CMDR.md`) and the tool declarations never vary.
- **⚠️ Memory LEADS the system string and is fenced; ❌ never append it like `CMDR.md`.** It's the one
  part of the prefix an attacker can reach (the agent's write path sees `image_facts` OCR and file
  names off disk), so it sits before the rules, in a fence its own content can't close, under a line
  saying it is data. `DETAILS.md` § The memory block; `../memory/DETAILS.md`.
- **The envelope rides the latest user turn only, snapshot-at-send** (tests pin both), so ground
  truth can't shift mid-turn.
- **Content is written only on `End`; the user row on the FIRST `End`** (crash cases (a)–(d),
  red-guarded). Don't pre-persist either row.
- **The pure core is genuinely pure — keep it that way.** Offset, envelope, `CMDR.md`, memory, and
  budget arrive as values; every context test runs with no tokio runtime and no filesystem.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`).
  Handed a stub instead of the content it was told to name files by, a model invents — that shipped,
  renaming 12 real files to fiction. It overruns honestly.
- **A dropped result says what it held and how to re-read it**, structurally and within 80 tokens
  (`context/digest.rs`). Keep it shape-agnostic and keep result STRINGS out: a digest describes a
  delivery, never is one (invariant 6).

- **A context drop is never silent, and it revokes the dropped result's evidence.** `assemble_prompt`
  returns `ElisionFacts` as DATA (the core can't log), naming every dropped `call_id`;
  `runtime/turn.rs` warns, emits ONE `ContextTrimmed`, and calls `revoke_evidence`. New compaction
  path ⇒ all three.

- **The prompt budget is the user's setting, then the model's window**, resolved fresh per turn in
  `session.rs` (a stale one has a wake thinking in a different window than the rail) and passed as
  `TurnParams::prompt_budget`; `context.rs` never learns the model. `budget.rs` holds the sources,
  the local floor, memory's share, and why the tool-result ceiling must NOT follow it.
- **A runaway loop is impossible by construction**: `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are checked
  at the TOP of the loop. Don't bump one silently.
- **Never block the main thread.** `run_turn` is async, and the real `ToolDispatcher` routes through
  `agent::tools::view::dispatch`.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**, forwarded onto `stream.rs`'s one
  conversation-keyed event: ❌ never key a turn to the invoke that started it, or a reload loses it.
