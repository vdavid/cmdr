# Ask Cmdr: the chat slice of the agent (v1 spec)

Status: spec complete, plan pending. 2026-07-12. Owner: David.

This spec defines the **"Ask Cmdr" vertical slice**: a read-only chat surface where the user talks to an LLM that can
see what Cmdr knows (drive index, importance, operation log, live app state) and answer questions about their files.
It is the first LLM-powered piece of the full agent
([`later/agent-spec.md`](later/agent-spec.md)) and deliberately ships ahead of the agent's proactive machinery.

It was distilled from the lead session that shipped the importance subsystem and the operation log; the design
decisions here (surface choice, context anatomy, scope cuts) were made with David and are settled unless listed as
open questions. The implementing session writes its own plan from this spec (see § How to use this spec); repo
research, exact DDL, and module layout belong to that plan, not here.

## 1. Why this, why now

- **The substrate is ready.** Everything a useful chat needs already exists and is deterministic: the drive index
  (sizes, listings, recency), the importance subsystem (`ImportanceIndex`: `top_n`, `explain`, offline NAS weights),
  the operation log (indexed history with provenance and honest coverage flags), and the MCP agent surface (the
  consolidated tool registry, per-volume indexing tool, importance resource, queue visibility, `await` conditions —
  explicitly built as the future agent's substrate).
- **It retires the biggest open risk cheaply.** The agent spec's §18.1 (can the pinned `genai` drive real multi-step
  tool loops with opaque thinking-state round-trip?) is the one question that can reshape the provider layer. This
  slice forces the answer on the smallest possible LLM surface, before the summarizer, wake loop, or proposals depend
  on it.
- **It is the headline.** "Talk to your file manager" is the demo that differentiates Cmdr, and it reaches beta users
  without the proactivity machinery whose etiquette risks burning trust. The wow ships first; the risky part bakes.
- **It forces the interaction-surface design** (the chat rail, session history, focus model) on the cheapest surface;
  the agent's later notifications and proposal reviews inherit it.

## 2. Principles (inherited from the agent spec; they govern anything unanswered here)

1. **Read-only, structurally.** The chat agent has NO write tools, no `read_file`-contents tool, nothing that mutates.
   Propose-never-act arrives with the proposals feature; until then the agent can only look and speak. This is also
   the privacy line: only names, paths, and metadata can ever reach the provider in v1 — never file contents.
2. **Deterministic bottom, LLM top.** Tools return typed, deterministic data from existing subsystems; the LLM adds
   judgment and language. No LLM in any hot path.
3. **Continuity through state, not transcript residency.** Threads live in the DB; every message is a cold,
   self-contained API call assembled from state. Nothing long-lived holds a conversation in memory.
4. **Honesty is load-bearing.** The system prompt requires the agent to respect and voice coverage caveats (a
   `top_level_only` operation-log flag, a stale index, an unmounted volume) rather than answer confidently past them.
5. **Radical transparency.** Every tool call the agent makes is visible in the chat UI (a collapsible "looked at X"
   line per call), and threads are plain rows in a local DB the user can inspect.
6. **Typed everything.** Message roles, tool identifiers, stop reasons, error kinds cross every boundary (IPC, DB) as
   typed enums, never matched strings (`no-string-matching`).

## 3. Scope

### v1 (this effort)

- **Step 0, gating: the `genai` capability spike** (agent-spec §18.1). About a day. Verify against the pinned crate:
  multi-call turns, per-provider tool-schema strictness, opaque thinking/reasoning-state round-tripping in multi-step
  loops, streaming with tool calls, for Anthropic + OpenAI + Gemini. The outcome shapes the `AgentLlm` trait. If genai
  falls short: upstream patch, local patch, or per-provider adapters behind the trait for the gaps only — decided in
  the plan, not assumed.
- **The `AgentLlm` trait** (agent-spec D41) over the existing `src/ai/` genai client: messages carrying an opaque
  per-message provider-state blob, tool declarations, normalized tool calls and stop reasons, streaming deltas,
  cancellation via the existing stream-cancel. Provider types never leak past it. Plus a **deterministic fake
  implementation** (scripted turns and tool calls) — the entire runtime and UI must be testable with zero network.
- **`main.db`** — the agent's durable store (agent-spec D1/D3, as reconciled by the operation-log effort: a peer
  durable DB beside `operation-log.db`, in the app data dir, Time Machine-backed). Reuses the operation log's
  forward-migration-ladder pattern (the template exists; second consumer proves it). v1 tables: `conversations`
  (id, title, created/updated, archived flag, **nullable `origin`** — cheap insurance so a future
  notification-spawned thread is a column value, not a migration), `messages` (thread FK, seq, role, typed content
  blocks incl. tool calls/results as JSON, token counts, timestamp), FTS5 over message text, and a `cost_meter`
  (per-day, per-thread token + cost rows). No custom collation; `sqlite3`-inspectable. No auto-retention in v1
  (transcripts are small; revisit when real sizes exist).
- **The tool layer**: the chat agent consumes the consolidated tool registry **in-process** (agent-spec D49 — the
  registry is agent-first; MCP transport is the second consumer). v1 toolset, all read-only: app-state snapshot
  (panes, cursor, selection, volumes and their connectivity), directory listing/stats from the drive index,
  importance (`top_n` / `weight_for` / `explain`), operation-log search + detail, volume list. Where the shipped MCP
  executors already implement these, the in-process path reuses their cores — one implementation, two transports. If
  hoisting a definition out from under the MCP transport is needed, that refactor is in scope (it was anticipated
  when the registry was consolidated).
- **The chat runtime**: single-flight per thread (a new user message while a loop runs is queued, with a visible
  "working… stop?" affordance); per-message budgets (max tool turns, max wall time) so a runaway loop is impossible
  by construction; cancellation at tool boundaries plus stream-cancel for the in-flight HTTP call; typed error
  surface (no key, provider down, rate-limited, budget exhausted) rendered honestly but never with the words "error"
  or "failed".
- **Context assembly** (the pure, TDD-heavy core — values in, prompt out, no I/O):
  1. Stable prefix, byte-identical across calls for provider prompt caching: system prompt; tool declarations;
     `~/.cmdr/CMDR.md` if present (read-only in v1; the rules/memory machinery stays with the full agent).
  2. Thread history with **elide-only compaction**: assistant text survives verbatim; tool results older than a few
     turns collapse to a typed stub ("[tool result elided: top-20 dir listing, 3.1k tokens]"). Summarize-on-overflow
     is explicitly deferred; v1 adds a thread-length soft cap with honest UI copy ("this chat is getting long — start
     a fresh one?").
  3. A fresh **context envelope** injected into the latest user turn only (never the prefix, so caching survives):
     timestamp, focused pane path, cursor item, selection count, mounted volumes + connectivity. Every historical
     message carries its timestamp so the model can reason about gaps ("this morning").
  4. Attachments by reference: chips in the composer resolve to path + metadata in the envelope — never contents.
  - Budget: roughly 6-10k tokens per call; the budget and elision thresholds are named constants.
- **The UI — a toggleable right sidebar rail** in the main window. Decided over a separate window (divorces chat from
  pane context), a palette overlay (fine for one-shot asks, hostile to conversations), and a bottom drawer (steals
  file-list rows). Rationale: the agent's killer context is what the user is looking at, and the rail is the future
  convergence surface for notifications and proposal reviews. Requirements:
  - Toggle via a configurable shortcut (proposed default ⌥⌘A — the implementer collision-checks first, exactly as
    ⌥⌘O→⌘⌥L played out for the operation log) plus a View-menu item; full four-places menu wiring.
  - A deliberate focus model: the rail is a third focus region; entering/leaving is explicit (the toggle focuses the
    composer; Esc returns focus to the active pane); pane min-widths hold when the rail is open; layout persists.
  - Streaming rendering with a stop button; a collapsible tool-call line per call ("looked at ~/Movies — 210 GB");
    markdown-lite (paragraphs, lists, inline code, bold — no full renderer unless one is already in the tree).
  - Sessions: a thread list (recent first), new-chat, rename, archive (flag + filter), and **search across threads**
    (FTS5) — the operation-log dialog's list/paging patterns are the template.
  - Attach by drag from a pane or an "ask about selection" affordance → reference chips.
  - i18n across all 10 locales, style-guide copy, a11y (tier-3 test, focus trap, AA contrast), ALPHA badge via
    `feature-status.json`.
- **Consent + settings**: opt-in feature, BYO key via the existing `ai/` provider config. One consent screen stating
  exactly what leaves the machine: your messages, file/folder NAMES and paths, sizes/dates, and the app-state
  envelope — never file contents (structurally impossible in v1: no such tool exists). Settings: enable toggle,
  provider/model for the interactive slot (agent-spec D43's naming — the bulk slot arrives with summaries), spend
  display fed by `cost_meter`. Record consent; note the website privacy-copy touchpoint for when this ships.
- **Cost visibility**: per-thread token/cost in the thread footer; per-day rollup in settings.
- **Testing**: unit tests for every pure part (context assembly, elision, envelope, budget enforcement — TDD
  red→green); runtime + UI integration tests against the fake `AgentLlm` (scripted multi-tool turns, cancellation
  mid-loop, queueing, typed errors); one gated live smoke per Tier-1 provider (skipped without keys, never in CI's
  critical path); E2E for open-via-menu, open-via-shortcut, send-and-render against the fake backend.

### Explicitly deferred (named so nobody "helpfully" adds them)

- Folder summaries and any knowledge-layer walk (the agent spec's §5): v1 answers from what is already indexed; "I
  can only see names and metadata so far" is an honest, acceptable limitation.
- The event pipeline, wake loop, digests, notifications, proposals, proactivity dial.
- File-contents reading in any form, and content-attach. The consent, denylist, and read-budget machinery
  (agent-spec §11.3) comes with the feature that needs it.
- Summarize-on-overflow history compaction (elide-only + soft cap in v1).
- External MCP servers as agent tools (Cmdr as MCP client). A real trust break hides here: an external tool can
  write, silently bypassing read-only-by-construction. Design the tool layer so external mounts could slot in later
  behind per-server consent and the same gating; build none of it now.
- The local model in the interactive slot: include it only if the existing `ai/` local path drops in with zero extra
  work; otherwise defer with a note. Cloud BYO-key is the v1 posture.
- Memory writes, `~/.cmdr/rules/`, memory mining. v1 only reads `CMDR.md` if it exists.

## 4. Naming (settled pattern, one open call for David)

Per the agent spec's naming section: the persistent entity is **"the agent"**, internal modules live under
`src-tauri/src/agent/` (this effort creates `agent/llm`, `agent/chat`, `agent/store`, `agent/tools` or similar — the
plan owns the layout, `name-internals-after-the-UI` applies). The user-facing surface name for this slice is
**"Ask Cmdr"** (menu item, rail title, feature-status id `ask-cmdr`) — recommended, but it is a product-identity call:
**confirm with David before the i18n pass**, alongside the shortcut default and the consent-screen copy.

## 5. Anatomy of one API call (normative example)

For a thread with six prior messages spanning 12 hours, the seventh call is, top to bottom: (1) system prompt —
identity, the read-only hard rules, the honesty-about-coverage rule, style; stable, cached. (2) Tool declarations;
stable, cached. (3) `CMDR.md` if present. (4) History: all turns with timestamps; old tool results elided to typed
stubs, assistant prose intact — which is what lets "remind me what the big folders were this morning" answer with no
tool call. Thinking blocks round-trip within a turn's tool loop (the spike's subject) and are not retained across
user turns. (5) The context envelope as a tagged block opening the new user turn: `[Sat 21:30 · focused pane:
~/Documents/taxes · cursor: 2024/ · 0 selected · volumes: Macintosh HD, NAS-home (connected)]`. (6) The user's text.
An answer that needs no tools comes straight back; one that does runs the loop within budgets, each call surfaced in
the UI.

## 6. Execution gotchas (paid for in the last two efforts; carry them)

- The on-open formatter hook reflows `docs/specs/*.md` and has repeatedly mangled `later/agent-spec.md` (underscores
  → asterisks). Revert hook churn you didn't author; never commit it. About 12 docs are oxfmt-unclean and reflow on
  every check run — leave them out of commits too.
- Use `pnpm check`, never bare cargo — the COW-cloned worktree `target/` false-greens; `touch` a source first when
  verifying compilation freshness. `--fast` while iterating, full per milestone, `--include-slow` before the wrap.
- Never bump `file-length` / `claude-md-length` allowlists; surface warns. Known standing warns: `tool_registry.rs`
  (1296 vs 1121, awaiting David's consent) and `operation_log/CLAUDE.md` (628 words).
- New MCP-visible or IPC-visible enums: camelCase over the wire, snake_case tokens in DB, one owning `types.rs`
  mapping (the operation log's `token_enum!` pattern).
- The structural MCP registry tests (`EXPECTED_TOOL_NAMES`, counts, gate table, wire-bytes snapshot) fail on any
  registry change BY DESIGN — update them consciously, never loosen them.
- Run the app via `pnpm dev --worktree <slug>`; an FF-merge leaves the worktree's dev data dir
  (`com.veszelovszki.cmdr-dev-<slug>`) behind to delete by hand.
- Local `main` moves mid-effort (parallel sessions are the norm now). Rebase before the FF-merge and expect union
  conflicts where two features thread parameters through one seam; resolve as the union, compile-gate each replayed
  commit.

## 7. Open questions for the planning session

1. Everything the spike answers (§3 step 0) — run it before writing the milestone plan's provider sections.
2. Which existing MCP executor cores can be consumed in-process as-is vs. need hoisting out from under the transport.
3. Markdown-lite rendering: hand-rolled subset vs. an existing in-tree renderer (check What's-new first).
4. The envelope's exact field set and its consent wording.
5. Thread-length soft cap and elision thresholds (named constants; guesses are fine, tune with use).
6. Sidebar min-width behavior at small window sizes.
7. Which Tier-1 models to certify at ship time (verify current model ids at implementation time; never from training
   data).

## 8. How to use this spec

Write a plan first (`/plan`: worktree off local `main`, plan in `docs/specs/ask-cmdr-plan.md`, adversarial fresh-eyes
review rounds until convergence, max 5). The plan owns milestones, DDL, module layout, and test lists; this spec owns
behavior, scope, and the settled decisions above. Suggested milestone shape (the plan may reshape it with recorded
rationale): spike → `AgentLlm` + fake → `main.db` store → in-process tool layer → runtime + context assembly →
sidebar UI + streaming → sessions/search/attachments → consent/settings/i18n/E2E. TDD is mandatory for context
assembly and budget enforcement (pure functions; real red first). Surface product calls to David before building on
them: the feature name, the shortcut default after the collision check, the consent copy, and the sidebar side
(right is the recommendation, not a decree).
