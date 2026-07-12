# Agent subsystem

The in-app AI agent. Its first user-facing slice is **Ask Cmdr**, a read-only chat rail
([`docs/specs/ask-cmdr-spec.md`](../../../../../docs/specs/ask-cmdr-spec.md); plan:
[`docs/specs/ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md)). Named after the persistent entity, not the
surface, so later proactive slices (proposals, notifications) grow here too. Full map + milestone layout:
[DETAILS.md](DETAILS.md).

## Module map

- `llm/` (M1, here now): the `AgentLlm` seam — the provider-agnostic trait, its genai-backed impl, the deterministic
  fake, and the typed message-part model. See [`llm/CLAUDE.md`](llm/CLAUDE.md).
- `store/` (M2), `tools/` (M4), `chat/` (M5): durable `main.db`, the read-only toolset, the chat runtime + context
  assembly. Not built yet; `start(app)` lands with the store.

## Must-knows

- **Read-only by construction.** The chat agent has NO write tool and no content-read tool — only names, paths, and
  metadata ever reach the provider (spec §2.1). This is a structural privacy line, not a runtime check; don't add a tool
  that breaks it without revisiting the whole consent + gating story.
- **Staged ahead of consumers.** M1's seam has no non-test caller yet, so `mod.rs` carries a justified
  `#![allow(dead_code)]`. Remove it when M5 wires `AgentLlm` into the runtime — don't let it outlive its reason.

Depth (milestone layout, the read-only rationale, how the slice relates to the full agent): [DETAILS.md](DETAILS.md).
