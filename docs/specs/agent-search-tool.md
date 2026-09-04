# Ask Cmdr has 18 tools and none of them searches

**Problem**: asked "can you find some penguin pics on my drive?", Ask Cmdr has no tool that finds a file by name. It
reached for `list_dir` instead, invented `name` and `nameMatch` arguments the schema does not have, watched the
deserializer drop them silently, got a plain folder listing back, and reported "I searched, but nothing matched" four
times. Four confident fabricated negatives on a question the app answers instantly from its own index.

Two separate defects sit under that. The dropped arguments are a bug and are being fixed alongside this. This spec is
about the missing capability: the drive index, the search engine, the live walk, and a fully authored `search` tool all
exist and ship, and the agent view is the one consumer that cannot reach them.

**Read first**: `apps/desktop/src-tauri/src/agent/tools/CLAUDE.md` and its `DETAILS.md` (the contracts a new agent tool
has to satisfy: the size contract, the honesty contract, the dispatch gate, and the six wiring points), then
`apps/desktop/src-tauri/src/search/CLAUDE.md` (one volume per search is a ceiling enforced at the API, not a UI choice),
and `apps/desktop/src-tauri/src/mcp/executor/search.rs`'s module header.

## What already exists, so nobody rebuilds it

Almost everything. This is a wiring and shaping job, not a search project.

- **`SearchQuery`** (`search/types.rs`) already carries every filter an agent would ask for: `name_pattern` +
  `pattern_type` (glob or regex), a size range, a modified range, `is_directory`, `include_paths`, `exclude_dir_names`,
  `limit`, `case_sensitive`, `sort_by` (relevance, size, or modified), `exclude_system_dirs`, and `count_only`.
- **The honesty contract is already typed.** `SearchRunCoverage` (`search/live/events.rs`) reports `walk`,
  `permission_denied`, `declined`, `still_covering`, `unresolved_scopes`, `abandoned_ground`, `abandoned_locations`,
  `capped`, `target_volume_id`, and `hidden_by_excludes`, each documented as a field a caller branches on rather than a
  message it string-matches. That is exactly the shape the agent honesty contract demands, and it predates it.
- **A `search` tool is already authored** in the shared registry (`mcp/tool_registry/table.rs`), with
  `consumers: &[Consumer::AiClient]`, `access: Access::Read`, `schemas::search_schema()`, and
  `run: params_only search::execute_search`. It runs the same live path a person's search takes: it reads the index
  where the index covers the scope, and walks the folders it does not.
- **`search_photos` is already in the agent view**, and it is the tool that answers the content half of the penguin
  question (CLIP description match plus OCR). The model reached for it first and correctly, eight calls in parallel
  across volumes and modes, and every one came back "image indexing is off". So the content half was unanswerable on
  that machine before tool selection ever entered into it. See decision 4.

What is missing: the agent view, a result shape that fits one tool result, and a mount path so the model can name a
drive other than the boot one.

## Decisions

### 1. One `search` entry, both views, one typed JSON result

The registry is keyed by name, so there cannot be two `search` entries, and a second entry under a second name
(`find_files`) would fork the description and the schema, which is the drift the single authored table exists to
prevent. So the existing entry grows `Consumer::Agent`, and its result changes from a formatted text table to typed JSON
that serves both consumers.

The text table has to go regardless of who reads it. It reports coverage as English sentences the caller has to parse,
which is what the project's `error-string-match` rule forbids; it carries no `total` / `returned` / `truncated`; and
`limit` has no ceiling, so `limit: 5000` returns well over 100,000 estimated tokens of table (a padded row runs 120 to
150 characters) and pushes the rest of the turn out of the prompt. Claude Code, the other consumer, reads typed coverage
at least as well.

Blast radius is small: `format_search_results` has three unit tests in `mcp/executor/tests.rs` and one E2E helper
(`test/e2e-playwright/search-walk-ground.ts:143`, a substring check that survives the change). The `tools/list` snapshot
pins declarations, not results, so it only moves for the schema trim in decision 7.

### 2. The result shape, and how it fits

```
{ targetVolumeId, matchCount, matchCountHuman, returned, truncated,
  entries: [{ name, path, parentPath, isDirectory, sizeBytes, sizeHuman, modified, modifiedHuman }],
  coverage: { complete, stillWalking, foldersFound, capped, hiddenByExcludes,
              permissionDenied[], declined[], stillCovering[], unresolvedScopes[],
              abandonedGround, abandonedLocations },
  notes: [ "…" ] }
```

- **`entries` goes through `mcp::executor::fit_to_result_budget`**, on top of a `limit` clamped to `MAX_LIMIT = 200`
  (the house cap `inspect_file`, `image_facts`, and `list_pane_files` all use). `returned` / `truncated` ride out with
  it. `matchCount` counts every match including the ones past the cap, so it is not the same number as `total` on the
  other paged tools, and its doc comment has to say so.
- **Numbers arrive spoken.** `sizeHuman` and `modifiedHuman` come from `search::format_size` and `format_timestamp`, the
  one formatter pair, never a second one. `iconId` is dropped: a model cannot render an icon.
- **Uncertainty rides inside the string**, per the `list_dir` doctrine: `matchCountHuman` reads `≥ 1,240 matches` while
  the walk is still going or the cap is hit, and `1,240 matches` when the number is exact. A flag a sibling field
  carries is a flag the model sheds when it restates the number.
- **`coverage.complete` is derived, and the other flags stay.** It is true only when the run settled, the walk completed
  or had nothing to walk, and `permissionDenied`, `declined`, `stillCovering`, `unresolvedScopes`, and `abandonedGround`
  are all clear. It exists so "may I say that is all of them" is one field rather than a seven-way conjunction the model
  gets wrong once in ten. The seven stay, because each one is a different sentence.
- **`notes` keeps the authored prose.** `coverage_note` already writes the sentences, and one of them is genuinely
  actionable copy no flag can replace ("granting Cmdr Full Disk Access in System Settings opens them"). This is the
  `SearchPhotosResult::ImageIndexingOff { note }` pattern: typed variants for branching, an authored note beside them.
  ❌ Not the `summary` field `list_dir` rejected: that one would have restated data the fields already carry.

### 3. `ai_search` stays out of the agent view

It spends an LLM call turning prose into a structured query. Ask Cmdr is already an LLM holding the user's prose, so
exposing it nests a second model call to do work the agent should do in the same breath it decides to search: two
providers billed, two failure modes, and a translation the agent cannot see or correct. It also costs a second schema in
the prefix every turn. The agent writes the structured query itself.

### 4. Content search: search names first, then look inside the hits

The engine matches names and metadata, never file contents. That is what David's penguin question actually needed, so
here is the honest answer rather than a hand-wave.

Three paths reach contents today, and together they cover most of what gets asked:

- **`search_photos`** answers "penguin pics" directly, by CLIP description and by OCR, across every media-indexed
  volume. It is already in the agent view.
- **`search` then `inspect_file` with `find`** answers "the invoice that mentions Rymd": narrow by name, size, or date
  to at most 200 paths, then search inside them. `inspect_file` needs paths up front, and this is where it gets them.
- **`image_facts`** answers "what does this screenshot say" for paths the model already holds.

**A drive-wide content index is out of scope, and stays out until something measures the gap.** It is a second index
with its own enrichment pass, storage budget, and incremental reconcile, which is a subsystem on the scale of
`media_index`, and the two approximations above cover the common questions. What this spec owes it is a description that
steers the model down the chain instead of letting it conclude a name search speaks for contents: the tool description
says the search is over names and metadata, and points at `inspect_file` for what is inside.

**Watch item, not a milestone**: on the transcript that prompted this spec, `search_photos` answered "image indexing is
off" eight times, and the model then fell back to inventing name-search arguments because no name search existed. Once
`search` lands, that fallback is a real tool, so the remaining question is whether the model offers to turn image
indexing on when the content half is the half that was asked for. If it does not, the fix is the two descriptions and a
prompt line, not a third tool.

### 5. Coverage honesty, field by field

What each field obliges the model to say. Everything below already exists; the work is the mapping and one prompt
paragraph.

- `stillWalking` (from `AnswerEnding::StillWalking`): the list and the count are a lower bound and running the same
  search again picks up from here. ❗ Not "no matches".
- `walk: Interrupted` / `Cancelled`, and `abandonedGround`: a lower bound for a different reason, so the sentence
  differs. `abandonedLocations` counts places, not folders, so a wedged mount reads as "one place" rather than "1,497
  folders".
- `permissionDenied`: name the folders, and offer Full Disk Access only where it would actually help (the note already
  gates on that).
- `declined`: Cmdr chose not to read snapshot trees. Nothing for the user to fix, so explain rather than offer.
- `stillCovering`: another walk holds that ground, so those results arrive later rather than being lost.
- `unresolvedScopes`: ❌ never "that folder does not exist". Cmdr cannot tell a typo from a folder nothing has walked.
- `hiddenByExcludes`: the count is filtered. The default system, cache, and build tier is right for "find my invoice"
  and exactly wrong for "where is my disk space going", where the hidden folders are the answer.
- `truncated`: already covered by the existing prompt rule ("say you looked at only `returned` of `total`").

The system prompt's § Coverage covers stale, scanning, lower-bound, and truncated. It does not cover a search that is
still walking, a folder the OS refused, or a filtered count, so it gains one short paragraph. That paragraph is prefix,
so it moves `SYSTEM_PROMPT_TOKENS` too.

### 6. One drive per call. The model loops, and `list_volumes` grows a mount path

`search/CLAUDE.md` is unambiguous: one volume per search is the ceiling, enforced in `resolve_target`, because fan-out
is the only way a search can silently omit a drive. So the tool does not take a volume list, and it does not fan out.

The model loops, deliberately and rarely:

- **The default stays the boot volume** when no scope is given, which is what "on my drive" usually means.
- **`list_volumes` gains `mountPath`.** Today it returns a name, an id, and index freshness, and no path, so a model
  that wants to search the NAS has nothing to put in `scope`. `VolumeSummary` is built from `loc.path`, so this is one
  field on two structs. Without it the loop is not expressible at all.
- **The description tells the model to cover the drive the question is about and say which one it covered**, then offer
  the others. A blind 10-volume fan-out costs 10 results in the prompt plus up to 20 seconds of walking each on the
  unindexed ones, and a question about the boot volume pays all of it.
- `ScopeError::SpansMultipleVolumes` stays a `ToolError`. It is a caller mistake, and its message already names the next
  move ("narrow the scope to a single volume, or search them one by one"), which is what a refusal owes.

### 7. The schema is the expensive part, so trim it

Measured against the shipped source: `search_schema()` serializes to 2,382 characters, roughly 595 estimated tokens,
plus 61 for the description. That is about 656 tokens on a 5,492-token fixed prefix, paid on every turn whether or not
the turn searches, and more than three times what the average tool declaration costs (3,683 tokens across 18).

Four properties carry most of it: `sortBy` (96), `maxWaitSeconds` (89), `excludeSystemDirs` (67), and `countOnly` (59).
Each is currently a paragraph. Trim each to one line, and move the "turn `excludeSystemDirs` off for disk-space
questions" hint into the tool description so it is stated once rather than twice. Target: about 430 tokens of schema
plus a 45-token description, roughly 475 in total.

That makes `FIXED_PROMPT_OVERHEAD_TOKENS` about 6,000, up from 5,492. Three pins move together: `budget.rs`'s constant,
and `SYSTEM_PROMPT_TOKENS` / `TOOL_DECLARATION_TOKENS` in `context/cost_tests.rs`. The `tools_list_snapshot.json`
fixture moves with the trimmed schema.

### 8. No consent copy change

The consent screen already lists "the names and paths of files and folders Cmdr looks at to answer you" and "their sizes
and dates". A name-and-metadata search egresses nothing outside that, so there is no new KIND of content and no
`CONSENT_COPY_VERSION` bump. Checked rather than assumed, because a reviewer will ask.

## Would it actually answer? Three questions, worked

Judged as the tool's consumer: could a model answer in a small number of calls without guessing?

**"Can you find some penguin pics on my drive?"** Two calls, in parallel. `search_photos({ query: "penguin" })` and
`search({ pattern: "*penguin*", type: "file" })`. The honest answer: "Three files have penguin in the name, in
~/Pictures/Antarctica, and the photo index matched 12 more by what is in them. That covers Macintosh HD. Your NAS is not
indexed for photos, so I cannot speak for it yet." ✅ Answerable. The asymmetry is real and worth naming in the reply:
`search_photos` covers every media-indexed volume, `search` covers one.

**"What's eating my disk space?"** One call.
`search({ sortBy: "size", excludeSystemDirs: false, type: "file", limit: 20 })` returns the biggest files anywhere on
the drive, which is what finds a 900 GB VM image eight levels down that no folder-by-folder walk would surface.
`hiddenByExcludes` is zero because the caller turned the tier off, so the count is not quietly filtered. ✅ Answerable,
and it complements `list_dir({ sortBy: "size" })` rather than duplicating it: that one ranks the children of a folder
you name, this one ranks a whole drive. The two descriptions have to draw that line, or the model guesses between them.

**"Find the invoices I saved last March."** One call, sometimes two.
`search({ pattern: "*invoice*", modifiedAfter: "2026-03-01", modifiedBefore: "2026-04-01" })`. When the invoices are
named `2026-03-14 Rymd.pdf` with no "invoice" in them, the second call is
`search({ pattern: "*.pdf", modifiedAfter: …, modifiedBefore: … })` and then `inspect_file` with
`find: { query: "invoice" }` over the hits. ✅ Answerable while the date range keeps the hit list under 200 paths, and
honestly refusable past that ("120 PDFs changed that month, which is more than I can read through; narrow it to a
folder"). One caveat the model must voice and the description must state: Cmdr records when a file last CHANGED, never
when it was saved or opened, which is the same limit the suggestion selector already carries.

**One shape gap found, and accepted.** There is no `offset`, so a result cannot be paged: ranking is top-k, and an
offset over a re-ranked run would skip and double-count rows. The model narrows instead (a tighter pattern, a smaller
scope, a date range, or `sortBy` plus a small `limit`), and the description says so. ❌ Do not add `offset` to the
engine for this.

**A property worth stating**: `search` is the preview the suggestion selector never had. `SelectorInput` (root, name
glob, size range, age) is a near-twin of `SearchQuery`, resolved once and frozen at proposal time, and it refuses
outright on an unindexed drive (`SelectorRefusal::NotIndexed`). `search` walks, so it can show the user what a sweep
would match, on ground the selector cannot even see.

## Milestones

1. **The typed result** (`mcp/executor/search.rs`). The DTO, `fit_to_result_budget` over `entries`, the clamped `limit`,
   `coverage.complete` derived from the six flags, `matchCountHuman` with its `≥`, and `coverage_note` refactored from a
   joined string into the `notes` vector. Delete `format_search_results` and its three tests; fix the E2E helper if the
   substring check does not survive. Pure shaper tests over fixture `LiveAnswer`s, so none of it needs a Tauri harness.
   This is where the real work is.
2. **The agent view.** `consumers: &[Consumer::AiClient, Consumer::Agent]`, a `ToolId::Search` variant, `ToolId::KNOWN`
   from 18 to 19 plus both name maps, `EXPECTED_AGENT_TOOL_NAMES`, and a rail label in `ask-cmdr-labels.ts` with its two
   catalog keys (`askCmdr.tool.search.doing` / `.done`) across 13 locales. A missing label silently shows "Working", and
   a test pins it.
3. **The schema trim and the pins.** Per decision 7: trim, regenerate `tools_list_snapshot.json`, move
   `FIXED_PROMPT_OVERHEAD_TOKENS` and the two `cost_tests.rs` constants.
4. **Coverage in the prompt.** One paragraph in § Coverage for still-walking, refused folders, and a filtered count,
   plus the test that pins it, plus the prefix re-pin from milestone 3.
5. **`list_volumes` gains `mountPath`** (decision 6), on `VolumeSummary` and `VolumeSnapshot`, from `loc.path`.
6. **Docs.** The catalog entry in `agent/tools/DETAILS.md`, the module map line in its `CLAUDE.md`, the size-contract
   list (which enumerates every tool that pages), and `mcp/executor/DETAILS.md` for the result shape.

## Tests

- **`a_search_result_is_cut_to_the_budget_and_reports_the_counts`**: the size contract, with `limit: 5000` against
  fabricated rows. **Test-first**: this is the defect the current shape has today.
- **`coverage_complete_is_false_when_any_gap_is_set`**, driven over every one of the six flags in turn. A derived
  boolean that is right five times out of six is worse than no boolean.
- **`a_still_walking_answer_never_reads_as_no_matches`**: `matchCountHuman` wears `≥`, `stillWalking` is true, and
  `complete` is false.
- **`the_agent_view_is_exactly_the_expected_set`** and the `ToolId::KNOWN` 1:1 test, both already written; they fail
  until every wiring point in milestone 2 lands, which is the point of them.
- **`the_system_prompt_names_the_search_coverage_flags`**, alongside the existing prompt assertions.
- **`hidden_by_excludes_survives_into_the_result`** with a fixture where the default tier hid matches, since a filtered
  count read as complete is the wrong-conclusion case decision 5 exists for.

## Size

Three to four days, plus two short reviews from David.

- Milestone 1 is a day and a half: a new DTO, the notes refactor, the pure tests, and three deletions.
- Milestone 2 is a day, most of it the 13-locale label pair.
- Milestones 3 to 6 are half a day each at most; 5 is two hours.

**David's calls, neither blocking the build**: the two English rail-label strings are new user-facing copy, and the
existing `coverage_note` sentences become copy a model will relay nearly verbatim in the rail, which is a confirm rather
than a write.

## What this deliberately leaves

- **A content index.** Decision 4, gated on evidence that the `search` then `inspect_file` chain is not enough.
- **`offset` on the engine.** Ranking is top-k; the model narrows instead.
- **Fan-out across volumes.** The ceiling in `search/CLAUDE.md` holds; the model loops and says which drive it covered.
- **`ai_search` in the agent view.** Decision 3, and it should stay a decision rather than an omission.
