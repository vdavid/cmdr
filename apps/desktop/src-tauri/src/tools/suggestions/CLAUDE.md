# Suggested-ops tools (`tools/suggestions/`)

The agent's four tools over the proposal spine: `list_suggestions` and `get_suggestion_group` (`Access::Read`), and
`propose_suggestions` and `propose_server` (`Access::Propose`). They stage and read; the user approves, and no tool
does. Depth: `DETAILS.md`.

## Module map

- `input.rs`: what `propose_suggestions` accepts, validated into planned shapes that pair each verb with the target its
  executor binds.
- `problem.rs`: `GroupProblem`, the one refusal vocabulary every check below refuses in. It depends on none of them, so
  the checks don't depend on each other.
- `place.rs`: every place a group names, through the location resolver (and onto a proven live sibling).
- `names.rs`: the names a group gives its files (a rename's, a move's or copy's), checked by the engine's own rules on
  the placed group. ❌ Never a second copy of those rules.
- `propose.rs`: the schema, the handler, and the write path (resolve → check → write). `snapshot.rs`: one bounded
  stat per hand-listed source, frozen as its creation snapshot before the write (also
  `propose_rename_plan`'s).
- `server.rs` (+ `server_plan.rs`): `propose_server` and its checks. The saved servers they read, and the checks the
  approval re-runs, are `../../agent/suggested_ops/server/`'s.
- `list.rs` / `group.rs`: the two reads and their pure shapers.
- `tests/`: one file per concern (input contract, write path, read shapers), fixtures in `mod.rs`.

## Must-knows

- **Validation runs to completion before the first write.** One bad group stages none of them, because a half-applied
  sweep leaves the user reading a mix of what the agent meant and what it managed.
- **A proposal names only places that can run now.** Every place resolves first (`plan_sweep`); a miss refuses the
  whole call in the read tools' miss shape, and a dead volume's signed-in live sibling takes the group, stored there
  with one `via`. ❌ Never store the id asked when a sibling took it. `DETAILS.md` § Where a proposal may point.
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
- **`propose_server` refuses on a turn the user didn't start, in its own handler** (❌ not in `admit`), and an edit
  can't change where a server is or which account it signs in as: the app's edit would MOVE the stored password there.
  `DETAILS.md` § Proposing a server.
- **A schema here is prefix**: it rides in every cached turn, so keep it terse and say the rest once, in the registry
  line or the system prompt (`agent/chat/DETAILS.md` § What the budgets buy).

The store beneath: `../../agent/store/proposals/CLAUDE.md`. The service between: `../../agent/suggested_ops/CLAUDE.md`.
