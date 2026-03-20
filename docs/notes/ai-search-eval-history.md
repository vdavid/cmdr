# AI search prompt eval history

Reference for future eval work. Tracks four rounds of prompt tuning for the AI search feature (natural language to
structured file search query translation).

## Iteration summary

| Round | Queries | MCP reliability | Key changes after                                                                                        |
| ----- | ------- | --------------- | -------------------------------------------------------------------------------------------------------- |
| R1    | 6       | n/a             | Glob limitation docs, naming conventions, category-to-extension mapping, size inference, macOS screenshots, default code exclusions |
| R2    | 19      | 72% (8/29 empty, 15s LLM timeout) | HTTP timeout 15s to 30s, pass-1 query JSON in refinement context, "preserve flags" rule, regression guard (discard pass 2 if it broadens) |
| R3    | 29      | 90% (26/29)     | Prompt compacted 8234 to 3200 chars, extracted to constants                                              |
| R4    | 29      | 90% (3 empty)   | Restored "pass 1 = broad discovery", prominent regex constraint, 3 examples back as behavioral anchors, "ALWAYS include namePattern" rule, empty-pattern guard in `search()` |

## Round details

### R1: initial implementation (6 queries)

Issues found:
- `invoice.*rymd|rymd.*invoice` -- over-constrained, should be `*rymd*`
- "screenshots from this week" -- didn't know macOS `Screenshot YYYY-MM-DD` naming
- "documents older than a year" -- 886K results, no file extension filter
- "big videos I can delete" -- `{a,b}` brace expansion not supported in our glob engine, 0 results
- "node_modules taking up space" -- good, but no size filter for "taking up space"
- "python script yesterday" -- good translation

### R2: after scope filtering, case toggle, system dir exclusion, two-pass preflight (19 queries)

Good: ssh keys, tax 2024, log files, zip >50mb, empty folders, biggest files, recent downloads.
Bad: kubernetes (8559 hits -- refinement broadened to all `.yml`), package.json not in node_modules (`excludeDirs`
ignored), "files edited today" (2607 hits -- no system exclusion).
Broken (MCP timeouts): fonts, docker compose, presentation.

### R3: after flag preservation fix (29 queries)

Fixed from R2: .env files (0 to 48), docker compose (0 to 24), shell scripts (0 to 1286).
Still bad: package.json not in node_modules (745, no `excludeDirs`), "files edited today" (3960).
Regressions from refinement: markdown notes (574 to 0, over-narrowed to `note*.md`).

### R4: after prompt compaction (29 queries)

New regressions from compaction:
- rymd invoices: refinement generated `(?=...)` lookahead -- regex error
- node_modules: lost name filter entirely, returned Spotlight dirs
- empty folders: lost `maxSize: 0` trick (224K hits)
- recent downloads: lost `~/Downloads` scope (51K hits)
- markdown notes: refinement over-narrowed again

Root causes: lost "be broad" framing, regex constraint less prominent, lost key examples as behavioral anchors.
MCP failures: LLM returns no `namePattern` -- scan 5.6M entries -- 60s -- MCP client timeout.

## Query catalog

| # | Query | R1 | R2 | R3 | R4 | Best known result | Notes |
|---|-------|----|----|----|----|-------------------|-------|
| 1 | rymd invoices | Bad | Good | Good | Error | `*rymd*` glob (R3) | Naming convention, not AND logic |
| 2 | screenshots this week | Bad | Good | Good | Good | `^Screenshot.*\.(png\|jpg\|heic)$` after Monday | macOS naming convention |
| 3 | node_modules space | Bad | Bad | Good | Bad | `^node_modules$` >=50MB dirs (R3) | Needs name+dir+size+includeSystemDirs |
| 4 | python script yesterday | Good | Good | Good | Good | `*.py` date range + caveat (R2+) | Straightforward |
| 5 | docs older than 1yr | Bad | Good | Good | Good | Doc extensions, before 1yr ago (R2+) | Category-to-extension mapping |
| 6 | big videos to delete | Bad | Good | Good | Good | Video regex >=100MB + caveat (R3+) | Category+size+caveat |
| 7 | ssh keys | -- | Good | Good | Good | SSH key filenames regex (R3) | Concept-to-filenames |
| 8 | presentation last quarter | -- | MCP fail | Good | Good | pptx/key/pdf exts, Q4 date range (R4) | Still imprecise (pdf too broad) |
| 9 | docker compose | -- | MCP fail | Good | Good | `^(docker-compose\|compose)\.(ya?ml)$` (R4) | Exact filename match |
| 10 | fonts installed | -- | MCP fail | Good | MCP fail | Font extensions + caveat (R3) | LLM timeout prone |
| 11 | log files disk space | -- | Good | Good | Good | `\.(log\|out\|err)$` >=50MB + includeSystemDirs (R2+) | Needs includeSystemDirs |
| 12 | tax documents 2024 | -- | Good | Good | Good | Tax keywords + doc exts, 2024 range (R2+) | Keywords+extensions+date |
| 13 | websocket rust | -- | Good | Good | Good | `websocket.*\.rs$` (R3) | Concept+extension |
| 14 | kubernetes | -- | Bad | Good | MCP fail | `(k8s\|kube\|kubectl\|helm\|kubernetes)` (R2/R3) | Keyword expansion, refinement broadens |
| 15 | photos of my cat | -- | Good | Good | MCP fail | Image extensions + caveat (R2+) | Semantic, unfilterable |
| 16 | biggest files | -- | Good | Good | Good | >=500MB + size sorting caveat (R3+) | Size only |
| 17 | empty folders | -- | Good | Good | Bad | `isDirectory: true, maxSize: 0` (R2/R3) | Creative filter trick |
| 18 | recent downloads | -- | Good | Good | Bad | Scope ~/Downloads, after this week (R3) | Needs scope, not just name |
| 19 | files edited today | -- | Bad | Bad | Good | After today, system dirs excluded (R3, 153 hits) | Needs system exclusions |
| 20 | zip >50mb | -- | Good | Good | Good | `*.zip` >=50MB (all rounds) | Straightforward |
| 21 | HEIC not converted | -- | Good | Good | Good | `\.heic$` + caveat (R3+) | Category+caveat |
| 22 | shell scripts dotfiles | -- | Good | Good | Good | `\.(sh\|bash\|zsh)$` (R4, 1286 hits) | Scope+includeSystemDirs |
| 23 | pnpm-lock wasting space | -- | Good | Good | Good | `^pnpm-lock\.yaml$` (threshold debate: 50MB too high) | Exact name+size |
| 24 | contracts last 6mo | -- | Good | Good | Good | Contract keywords + doc exts (R4) | Keywords+extensions+date+caveat |
| 25 | .env files | -- | Bad | Good | Good | `^\.env(\..+)?$` (R4, 48 hits) | Dotfile pattern |
| 26 | sqlite databases | -- | Good | Good | Good | `\.(sqlite\|sqlite3\|db)$` (all rounds) | Extension search |
| 27 | markdown notes this month | -- | Good | Bad | Bad | `\.md$` after month start (R3 pre-refinement) | Refinement over-narrows |
| 28 | old xcode projects | -- | Good | Good | Good | `\.(xcodeproj\|xcworkspace)$` before 1yr (R3+) | Extension+date+caveat |
| 29 | package.json not in node_modules | -- | Bad | Bad | Bad | `^package\.json$` + excludeDirs:["node_modules"] (never achieved) | excludeDirs not supported by LLM |
| 30 | audio recordings meetings | -- | Good | Good | Good | Meeting keywords + audio exts (R4) | Keywords+extension+caveat |

Legend: Good = reasonable results, Bad = too many/few results or wrong filters, Error = regex/parse failure,
MCP fail = timeout or empty response, -- = not tested in that round.

## Key lessons

- **Examples are behavioral anchors.** Removing them from the prompt causes regressions on those exact query types.
  R4 proved this: compacting away the node_modules, empty folders, and kubernetes examples broke all three.
- **"Be broad" framing is critical for pass 1.** Without it, the LLM over-constrains initial queries, leaving pass 2
  nothing to work with.
- **Regex constraint must be prominent.** The LLM uses lookahead (`(?=...)`) if the constraint is buried. Must be
  near the top, bold, repeated.
- **Flag preservation needs explicit rules in the refinement prompt.** Without "preserve all flags from pass 1",
  refinement drops `includeSystemDirs`, `excludeDirs`, etc.
- **Regression guard is essential.** Discard pass 2 if it returns more results than pass 1. Refinement broadens
  instead of narrowing ~10% of the time.
- **Empty-pattern queries on 5M+ indexes cause 60s scans and MCP timeouts.** The `search()` function must guard
  against missing `namePattern`.
- **MCP reliability correlates directly with LLM HTTP timeout.** 15s to 30s fixed most R2 failures.
- **Two-pass is powerful but refinement can be destructive.** Over-narrows ~15% of the time (for example, `\.md$` to
  `note*.md`).
- **The LLM consistently struggles with:** "not in X" (needs `excludeDirs`), creative filter tricks (`maxSize: 0`
  for empty folders), combining name + directory + size filters simultaneously.

## Failure taxonomy

| Failure mode | Frequency | Example | Mitigation |
| ------------ | --------- | ------- | ---------- |
| MCP timeout (no namePattern) | ~10% of queries | fonts, kubernetes, photos | Empty-pattern guard in `search()` |
| Refinement over-narrows | ~15% of pass 2 | markdown notes, .env | Regression guard (discard if broadens) |
| Refinement drops flags | ~10% of pass 2 | node_modules (loses excludeDirs) | "Preserve flags" rule in prompt |
| Unsupported regex syntax | Rare | `(?=...)` lookahead | Prominent regex constraint |
| LLM can't express filter | ~5% of queries | package.json not in node_modules | Limitation of current prompt design |
| Category mapping miss | ~5% of queries | fonts, presentations | More examples in category map |
