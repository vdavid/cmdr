# Agent tools (`agent/tools/`)

Ask Cmdr's in-process tool layer: read families, propose families, and the two memory tools, authored as
`consumers: [Agent]` entries of the shared `mcp_tools!` registry (agent decision D49, one authored source), with handlers
and typed result shapes colocated here.

## Module map

- `read/`: `state`, `pane_listing`, `listing`, `importance`, `volumes`, and `inspect/` (DTOs + pipeline, `text.rs`,
  `find.rs`, `pdf.rs`, `archive.rs`, `exif.rs`, `runner.rs` for the timeouts). `operations_*`, `search`,
  `search_photos`, and `image_facts` are shared with the ai-client view and live in `mcp/executor/`.
- `propose/`: `propose_rename_plan` and the image-facts evidence ledger. `propose/CLAUDE.md`.
- `suggestions/`: the suggested-ops tools over the proposal spine, `propose_server` included. `suggestions/CLAUDE.md`.
- `memory.rs`: `memory_write` + `memory_edit`, the only tools that write; every rule lives in `../memory/`.
- `quiet.rs`: `nothing_to_suggest`, a pure SIGNAL: the wake path acts on the call, a rail turn calling it is inert.
- `connect.rs`: `connect_volume`, the one `Session` tool.
- `view.rs`: `dispatch` + `refuse_unavailable`, the no-write choke point. `mod.rs`: `agent_tool_declarations()`.
- `line/`: what the rail's tool line says about a call (subject, outcome, preview, repeat mark). `line/CLAUDE.md`.

## Must-knows

- **Reuse the shipped core; never re-derive.** A handler calls a deterministic core (`indexing::read::queries`,
  `snapshot_*`, the proposal store, `file_viewer`'s seams) and only SHAPES the result.
- **A place is `path` + optional `volumeId` in, `location` out, via `crate::location`'s one resolver**; a miss is a
  typed `Ok`, ❌ never an empty listing. `DETAILS.md` § The location contract.
- **A result that carries a list must fit ONE tool result.** Page it with `mcp::fit_to_result_budget` and report
  `total` / `returned` / `truncated`; an oversized result pushes the turn's own evidence out of the prompt.
  ⚠️ `search` is the exception: its `matchCount` counts PAST the cap, so it is never a `total`.
  `DETAILS.md` § The size contract.
- **A schema is PREFIX.** Every declaration rides every turn, so keep descriptions terse. A new tool moves
  `budget::FIXED_PROMPT_OVERHEAD_TOKENS`, pinned by `context/cost_tests.rs`.
- **Every result voices its coverage honestly** (spec §2.4): a lower bound, stale data, an unindexed volume, or a cut
  is a typed field in the result, never a wrong zero. Field by field: `DETAILS.md`.
- **`Unrecognized` is out of the view AND out of dispatch.** `ToolId::from_wire_name` maps any non-view name to
  `Unrecognized`; `refuse_unavailable` refuses it, any `Write`, and `Session` off a user-started turn, BEFORE
  `execute_tool`. Keep `ToolId::KNOWN` 1:1 with `agent_tool_view()`.
- **`Propose`, `Memory`, and `Session` are hand-allowlisted** (`EXPECTED_*_TOOL_NAMES`): nothing proves a handler
  doesn't mutate, stays jailed, or dials only a saved place. ❌ Never tag one unlisted, or give AI clients `Memory`.
- **A proposal claiming file CONTENTS must prove it.** `dispatch` feeds every non-elided `image_facts` result into the
  thread's `ImageFactsLedger`; a plan citing undelivered content is refused whole, and whatever elides a result owes
  the ledger a `revoke_call`. `propose/DETAILS.md`.
- **Handlers never touch a live filesystem, except `inspect_file` and `list_dir`'s live read**: bounded, the thread
  ABANDONED past a deadline (`unreachable`). Stop abandons any call but a `Propose` / `Memory` one
  (`../chat/DETAILS.md`). `list_dir`'s live read is FDA-gated and index-shaped (`DETAILS.md` § The live source).
  `inspect_file` egresses contents, never bytes, each kind named in the cloud AI disclosure; a new KIND is a copy
  change plus a `CLOUD_AI_CONSENT_VERSION` bump. § Reading a file the way the viewer does.
- **Anything yielding file contents passes `read/sensitive.rs`.** § The sensitive-content gate.
- **New agent tool** = registry entry + handler/schema/result here + a `ToolId` variant + `EXPECTED_AGENT_TOOL_NAMES`
  + `ToolId::KNOWN` + a `line/` subject, outcome, and preview arm + a rail label in `ask-cmdr-labels.ts`.

The two-view registry model, the tool catalog, the honesty and size contracts, and the dispatch gate: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
