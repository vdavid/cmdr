# Suggested-ops tools (`agent/tools/suggestions/`)

The agent's three tools over the proposal spine: `list_suggestions` and `get_suggestion_group` (`Access::Read`), and
`propose_suggestions` (`Access::Propose`). They stage and read; the user approves, and no tool does. Depth:
`DETAILS.md`.

## Module map

- `input.rs`: what `propose_suggestions` accepts, validated into planned shapes that pair each verb with the target its
  executor binds. All the refusal variants.
- `propose.rs`: the schema, the handler, and the write path (resolve → check → write).
- `list.rs` / `group.rs`: the two reads and their pure shapers.
- `tests/`: one file per concern (input contract, write path, read shapers), fixtures in `mod.rs`.

## Must-knows

- **Validation runs to completion before the first write.** One bad group stages none of them, because a half-applied
  sweep leaves the user reading a mix of what the agent meant and what it managed.
- **A selector is resolved once, here, at creation.** Nothing re-resolves it later; the frozen rows are what the user
  reviews and what runs. An empty match is a refusal carrying the pattern, and it stays distinct from
  `SelectorRefusal::NotIndexed` ("I can't see that drive" is not "nothing matched").
- **There is no last-opened predicate, and adding one needs an access-time source first.** The drive index has size,
  mtime, and inode; `importance.db`'s visits are per-FOLDER. A predicate that silently matched nothing would be worse
  than its absence. The prompt tells the model to say when a file last CHANGED instead.
- **The planned shapes are the validation.** `SourceShape` carries each verb's target and `PlannedSources` carries the
  naming a hand-listed group needs (a selector supplies its own), so a trash group with a destination, or a rename built
  from a pattern, can't reach the store. Add a verb by adding a variant.
- **Amend lives inside `propose_suggestions`** and only touches `pending`. A separate mutating tool would be
  `Access::Write` under the registry's tiebreaker and would fail `test_agent_tool_view_never_writes`.
- **A schema here is prefix**: it rides in every cached turn, so keep it terse and say the rest once, in the registry
  line or the system prompt (`agent/chat/DETAILS.md` § What the budgets buy).

The store beneath: `../../store/proposals/CLAUDE.md`. The service between: `../../suggested_ops/CLAUDE.md`.
