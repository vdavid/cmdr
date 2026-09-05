# Agent tools (`agent/tools/`)

Ask Cmdr's in-process tool layer: read families, propose families, and the two memory tools, authored as
`consumers: [Agent]` entries of the shared `mcp_tools!` registry (agent-spec D49, one authored source), with handlers
and typed result shapes colocated here.

## Module map

- `read/`: `state`, `pane_listing`, `listing`, `importance`, `volumes`, and `inspect/` (DTOs + pipeline, `text.rs`,
  `find.rs`, `pdf.rs`, `archive.rs`, `exif.rs`, `runner.rs` for the timeouts). `operations_*`, `search`,
  `search_photos`, and `image_facts` are shared with the ai-client view and live in `mcp/executor/`.
- `propose/`: `propose_rename_plan` and the image-facts evidence ledger. `propose/CLAUDE.md`.
- `suggestions/`: the suggested-ops trio over the proposal spine. `suggestions/CLAUDE.md`.
- `memory.rs`: `memory_write` + `memory_edit`, the only tools that write; every rule lives in `../memory/`.
- `quiet.rs`: `nothing_to_suggest`, a pure SIGNAL: the wake path acts on the call, a rail turn calling it is inert.
- `view.rs`: `dispatch` + `refuse_unavailable`, the no-write choke point. `mod.rs`: `agent_tool_declarations()`.

## Must-knows

- **Reuse the shipped core; never re-derive.** A handler calls a deterministic core (`indexing::read::queries`, the
  importance `snapshot_*` functions, `snapshot_volumes`, the proposal store, `file_viewer`'s seams) and only SHAPES
  the result.
- **A result that carries a list must fit ONE tool result.** Page it with `mcp::fit_to_result_budget` and report
  `total` / `returned` / `truncated`; an oversized result pushes the turn's own evidence out of the prompt.
  ⚠️ `search` is the exception: its `matchCount` counts PAST the cap, so it is never a `total`.
  `DETAILS.md` § The size contract.
- **A schema is PREFIX.** Every declaration rides every turn, so keep descriptions terse. A new tool moves
  `budget::FIXED_PROMPT_OVERHEAD_TOKENS`, pinned by `context/cost_tests.rs`.
- **Every result voices its coverage honestly** (spec §2.4): a lower bound, stale data, an unindexed volume, or a cut
  is a typed field in the result, never a wrong zero. Field by field: `DETAILS.md`.
- **`Unrecognized` is out of the view AND out of dispatch.** `ToolId::from_wire_name` maps any non-view name to
  `Unrecognized`; `refuse_unavailable` refuses it, and anything the registry calls `Write`, BEFORE `execute_tool`.
  Keep `ToolId::KNOWN` 1:1 with `agent_tool_view()` (a test pins it).
- **Two hand-authored allowlists guard the two widenings**, `EXPECTED_PROPOSE_TOOL_NAMES` and
  `EXPECTED_MEMORY_TOOL_NAMES`, because no structural check proves a handler doesn't mutate or stays in the jail.
  ❌ Never tag an entry `Propose` or `Memory` without adding it there; never expose a `Memory` tool to the ai-client
  view.
- **A proposal claiming file CONTENTS must prove it.** `dispatch` feeds every non-elided `image_facts` result into the
  thread's `ImageFactsLedger`; a plan citing undelivered content is refused whole, and whatever elides a result owes
  the ledger a `revoke_call`. `propose/DETAILS.md`.
- **Handlers never touch a live filesystem, except `inspect_file`**: up to 200 files per call on blocking threads
  (5 s per path, 20 s per call); past a deadline the row is `unreachable`, the thread is ABANDONED, and the unanswered
  paths are named. It rides `file_viewer`'s seams and the pane's archive routing, never its own (`DETAILS.md` § Reading
  a file the way the viewer does), and alone egresses contents: text windows, `find` snippets, PDF pages plus title
  and author, archive entry names, EXIF incl. GPS; never bytes. The consent copy names each
  (`askCmdr.consent.item.contents`, `askCmdr.consent.contentsRule`); a new KIND of content is a copy change plus a
  `CONSENT_COPY_VERSION` bump.
- **New agent tool** = registry entry + handler/schema/result here + a `ToolId` variant + `EXPECTED_AGENT_TOOL_NAMES`
  + `ToolId::KNOWN` + a rail label in `ask-cmdr-labels.ts` (a missing label shows "Working"; a test pins it).

The two-view registry model, the tool catalog, the honesty and size contracts, and the dispatch gate: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
