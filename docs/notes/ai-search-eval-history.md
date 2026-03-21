# AI search eval history

Reference for future eval and prompt tuning. Tracks six rounds of development for the AI search feature (natural language
to structured file search query translation), including the v1 → v2 architecture transition between R5 and R6.

## Architecture overview

### v1 (R1–R5): LLM generates full JSON query

The LLM receives a large prompt (~3200–8200 chars depending on round) and generates a complete JSON `AiSearchQuery` with
13 fields: regex patterns, ISO dates, byte counts, extension lists, scope paths. A two-pass refinement step (pass 1 =
broad, pass 2 = narrow based on result counts) adjusts the query. Cloud models succeed ~85% of the time; a 2B local
model succeeds ~40%.

Failure modes: regex syntax errors, wrong date arithmetic, invalid JSON, refinement regressions (over-narrows ~15%,
drops flags ~10%), MCP timeouts from missing `namePattern`.

### v2 (R6+): LLM classifies, Rust computes

The LLM receives a compact classification prompt (~600 chars) and returns key-value lines with enum tokens:
`keywords: rymd`, `type: documents`, `time: recent`. Rust then deterministically maps these to regex patterns, timestamp
ranges, byte thresholds, and scope paths. No JSON generation, no regex generation, no date math by the LLM. No two-pass
refinement — single pass only.

Key files:
- `ai_response_parser.rs` — parses key-value lines, validates enums
- `ai_query_builder.rs` — maps enums to `SearchQuery` fields, merges keyword+type patterns
- `search.rs` — contains `CLASSIFICATION_PROMPT`, orchestrates the pipeline

The v2 design is documented in `docs/specs/ai-search-v2-plan.md`.

## Iteration summary

| Round | Pipeline | Queries | Reliability | Key changes after |
|-------|----------|---------|-------------|-------------------|
| R1 | v1 | 6 | n/a | Glob limitation docs, naming conventions, category-to-extension mapping, size inference, macOS screenshots, default code exclusions |
| R2 | v1 | 19 | 72% (8/29 empty, 15s timeout) | HTTP timeout 15→30s, pass-1 JSON in refinement context, "preserve flags" rule, regression guard |
| R3 | v1 | 29 | 90% (26/29) | Prompt compacted 8234→3200 chars, extracted to constants |
| R4 | v1 | 29 | 90% (3 empty) | Restored "be broad" framing, prominent regex constraint, 3 examples back as behavioral anchors, "ALWAYS include namePattern" rule, empty-pattern guard in `search()` |
| R5 | v1 | 30 | 93% (28/30) | Final v1 round. Fixed remaining R4 regressions. Established the 30-query catalog. Identified architectural limits of JSON generation approach |
| R6-cloud | v2 | 30 | 100% (30/30) | v2 pipeline: classification prompt + Rust builder. Zero parse failures, zero MCP timeouts |
| R6-local | v2 | 30 | 100% (30/30) | Same pipeline on 2B local model. All 30 queries return valid classification output |
| R7-local | v2 | 30 | 100% (30/30) | Post-fix eval on 2B model. Great/Perfect 43%, Bad down from 43%→20%. Fixes: shell-scripts type, last_6_months, redundant keyword detection, .key removal, prompt improvements |

## Round details

### R1: initial implementation (6 queries)

Issues found:
- `invoice.*rymd|rymd.*invoice` — over-constrained, should be `*rymd*`
- "screenshots from this week" — didn't know macOS `Screenshot YYYY-MM-DD` naming
- "documents older than a year" — 886K results, no file extension filter
- "big videos I can delete" — `{a,b}` brace expansion not supported in our glob engine, 0 results
- "node_modules taking up space" — good, but no size filter for "taking up space"
- "python script yesterday" — good translation

### R2: after scope filtering, case toggle, system dir exclusion, two-pass preflight (19 queries)

Good: ssh keys, tax 2024, log files, zip >50mb, empty folders, biggest files, recent downloads.
Bad: kubernetes (8559 hits — refinement broadened to all `.yml`), package.json not in node_modules (`excludeDirs`
ignored), "files edited today" (2607 hits — no system exclusion).
Broken (MCP timeouts): fonts, docker compose, presentation.

### R3: after flag preservation fix (29 queries)

Fixed from R2: .env files (0→48), docker compose (0→24), shell scripts (0→1286).
Still bad: package.json not in node_modules (745, no `excludeDirs`), "files edited today" (3960).
Regressions from refinement: markdown notes (574→0, over-narrowed to `note*.md`).

### R4: after prompt compaction (29 queries)

New regressions from compaction:
- rymd invoices: refinement generated `(?=...)` lookahead — regex error
- node_modules: lost name filter entirely, returned Spotlight dirs
- empty folders: lost `maxSize: 0` trick (224K hits)
- recent downloads: lost `~/Downloads` scope (51K hits)
- markdown notes: refinement over-narrowed again

Root causes: lost "be broad" framing, regex constraint less prominent, lost key examples as behavioral anchors.
MCP failures: LLM returns no `namePattern` → scan 5.6M entries → 60s → MCP client timeout.

### R5: final v1 round (30 queries)

Restored R4 regressions. Added query #30 (audio recordings meetings). This round established the final v1 baseline.
Cloud model: 28/30 queries produce usable results. Remaining failures:
- package.json not in node_modules — LLM never reliably produces `excludeDirs`
- markdown notes this month — refinement over-narrows `\.md$` to `note*.md`

R5 demonstrated the architectural ceiling: the two-pass JSON generation approach can't reliably handle `excludeDirs`,
creative filter tricks, or complex multi-field combinations. These are structural limitations, not prompt-tunable.

### R6: v2 pipeline (30 queries)

Complete architectural rewrite. The LLM now returns key-value lines with enum tokens; Rust handles all pattern
generation, date computation, and query assembly deterministically.

**R6-cloud (Anthropic cloud model):**
- 30/30 queries produce valid classification output (no parse failures, no timeouts)
- 27/30 produce correct/good results
- 3 queries have quality issues (see analysis below)

**R6-local (2B local model):**
- 30/30 queries produce valid classification output (dramatic improvement from v1's ~40% success)
- 24/30 produce correct/good results
- 6 queries have quality issues (keyword selection less precise, but structurally valid)

**R6 issues found and fixed post-eval:**
1. Missing `shell-scripts` type — query #22 had no type to map to. Added `shell-scripts` type with `\.(sh|bash|zsh)$`.
2. Missing `last_3_months`/`last_6_months` time enums — query #24 "contracts last 6 months" couldn't express the time
   range. Added both enums.
3. Keyword/type confusion — "HEIC photos" produced `keywords: heic / type: photos` → merged pattern
   `heic.*\.(jpg|jpeg|png|heic|webp|gif)$` which only matches files with "heic" in the name AND a photo extension.
   This misses `IMG_1234.heic` (no "heic" in the name part). Similar issue: "sqlite databases" produced
   `sqlite.*\.(sqlite|sqlite3|db)$` which misses `history.db`. Fix: added redundant-keyword detection in
   `merge_keyword_and_type` — when the keyword is a known extension that appears in the type's pattern, the keyword is
   dropped and just the type pattern is used. Also added prompt rules to prevent this classification in the first place.
4. `.key` extension in presentations type — `\.(ppt|pptx|key|odp)$` matched TLS certificates, not Keynote files.
   Removed `.key` from the pattern.
5. Prompt improvements — added rules for singular keywords, time-only-when-explicit, type-over-keywords preference,
   stronger exclude examples. Added 3 new examples (contracts, shell scripts, HEIC).
6. Exclude handling — added stronger prompt rule: "ALWAYS use `exclude` when the user says 'not in', 'but not in',
   'excluding', 'except in'."

### R7-local: post-fix eval on 2B model (30 queries)

First eval after the R6 fixes landed (shell-scripts type, last_6_months time enum, redundant keyword detection, .key
removal from presentations, prompt improvements for singular keywords and time-only-when-explicit).

**Results: 13 Great/Perfect (43%), 11 Good/Okay (37%), 6 Bad (20%), 0 errors.**

Compared to R6-local: Great/Perfect up from 7→13, Bad down from 13→6. The architecture is confirmed sound — remaining
gaps are 2B model quality (3 queries) and 2 missing capabilities (scope inference, markdown type).

**Key fixes confirmed:**
- "Time only when explicit" rule fixed 6 queries (#6, #9, #10, #11, #13, #26) that previously had spurious
  `time: today/recent`.
- `shell-scripts` type: #22 now uses the type correctly (was Bad).
- `last_6_months` time enum: #24 contracts now works perfectly (was Bad).
- `.key` removal from presentations: #8 no longer returns TLS certs.
- Redundant keyword detection: #21 HEIC no longer merges keywords with photos type.
- Singular keyword rule: #24 uses `contract` instead of `contracts`.

**Remaining 6 Bad queries — root causes:**
1. #14 kubernetes — `type: code` without keywords returns all 351K code files. 2B model doesn't understand "kubernetes"
   as a keyword, classifies it as a code query.
2. #18 recent downloads — `time: recent` emitted but no `scope: downloads`. Missing capability.
3. #25 .env files — merge logic gap: `keywords: env` + `type: env-files` → `env.*\.env` pattern.
4. #27 markdown notes — no `markdown` type exists, falls back to `type: documents` which loses `.md`.
5. #29 package.json not in node_modules — 2B model dumps all words as keywords, no `exclude`.
6. (Regressions) #3 node_modules, #12 tax docs — 2B model dumps all words as keywords instead of classifying.

Root cause pattern: #3, #14, #29 share the same 2B model limitation — the model ignores enum structure and puts raw
query words as keywords. #25 is a merge logic gap in `ai_query_builder.rs`. #18 and #27 are missing capabilities (scope
inference, markdown type). Cloud model handles all of these correctly.

## Query catalog

| # | Query | R5-cloud (v1) | R6-cloud (v2) | R6-local (v2) | R7-local (v2) | v2 interpreted pattern | Notes |
|---|-------|---------------|---------------|---------------|---------------|------------------------|-------|
| 1 | rymd invoices | Good | Good | Good | Great (92) | `(?i)rymd.*\.(pdf\|doc\|docx\|txt\|odt\|xls\|xlsx)$` | keywords: rymd / type: documents / time: recent. Correct merge |
| 2 | screenshots this week | Good | Good | Good | Good (0) | `(?i)^Screenshot.*\.(png\|jpg\|heic)$` + time_after: Monday | type: screenshots / time: this_week. 0 is real (CleanShot not Screenshot) |
| 3 | node_modules space | Bad | Good | Good | Okay (768) | `*node_modules*` glob, is_dir=true, min_size=100MB | R7: regressed. LLM put all words in keywords: `(node_modules\|folders\|taking)`. Lost `folders: yes` + `size: large` |
| 4 | python script yesterday | Good | Good | Good | Okay (77K) | `(?i)\.py$` + time range | R7: improved from Bad. `type: python` correct but no `time: yesterday` |
| 5 | docs older than 1yr | Good | Good | Good | Okay (6068) | `(?i)\.(pdf\|doc\|docx\|txt\|odt\|xls\|xlsx)$` + before 1yr | R7: `type: documents` + `time: last_year` (2025 only, not `time: old`) |
| 6 | big videos to delete | Good | Good | Good | Great (105) | `(?i)\.(mp4\|mov\|avi\|mkv\|webm)$` + min_size=100MB | R7: fixed from Bad. No spurious time or keyword |
| 7 | ssh keys | Good | Good | Good | Perfect (6) | `(?i)^(id_(rsa\|dsa\|ecdsa\|ed25519)\|authorized_keys\|known_hosts)(\.pub)?$` | type: ssh-keys. Stable across all rounds |
| 8 | presentation last quarter | Good | Good | Partial | Good (0) | `(?i)\.(ppt\|pptx\|odp)$` + quarter range | R7: fixed. `.key` removed from presentations. 0 is likely real |
| 9 | docker compose | Good | Good | Good | Great (24) | `(?i)^(docker-compose\|compose)\.(yml\|yaml)$` | R7: fixed from Bad. Type system, no spurious fields |
| 10 | fonts installed | Good | Good | Good | Great (1820) | `(?i)\.(ttf\|otf\|ttc\|woff\|woff2)$` + caveat | R7: fixed from Bad. `type: fonts` correctly used |
| 11 | log files disk space | Good | Good | Good | Perfect (1) | `(?i)\.(log\|out\|err)$` + min_size=100MB, exclude_system_dirs=false | R7: fixed from Bad. `type: logs` + `size: large`. warp.log found |
| 12 | tax documents 2024 | Good | Good | Good | Okay (778) | `(?i)tax.*\.(pdf\|doc\|docx\|txt\|odt\|xls\|xlsx)$` + 2024 range | R7: regressed from Good. `type: documents` + `time: 2024` but no keyword `tax` |
| 13 | websocket rust | Good | Good | Good | Great (9) | `(?i)websocket.*\.rs$` | R7: fixed from Bad. No spurious time |
| 14 | kubernetes | Good | Good | Partial | Bad (351K) | `*kubernetes*` glob | R7: regressed. LLM used `type: code` without keywords. All code files returned |
| 15 | photos of my cat | Good | Good | Good | Great (188K) | `(?i)\.(jpg\|jpeg\|png\|heic\|webp\|gif)$` + caveat | Stable + caveat |
| 16 | biggest files | Good | Good | Good | Good (17) | min_size=1GB + caveat (no sorting) | R7: fixed from Bad. `size: huge`, no name filter, caveat shown |
| 17 | empty folders | Good | Good | Partial | Good (10) | is_dir=true, max_size=0 | R7: improved. `size: empty` + `folders: yes` working |
| 18 | recent downloads | Good | Good | Good | Bad (1.4M) | scope: ~/Downloads + time_after: 3mo ago | R7: still bad. `time: recent` but no `scope: downloads` |
| 19 | files edited today | Good | Good | Good | Good (7568) | time_after: start of today | R7: regressed slightly. Lost system dir exclusion |
| 20 | zip >50mb | Good | Good | Good | Great (15) | `*zip*` glob + min_size=50MB | Stable. Perfect |
| 21 | HEIC not converted | Good | Good | Partial | Okay (188K) | `*.heic*` glob + caveat | R7: improved from Bad. `type: photos` only (redundant keyword dropped). Returns all photos |
| 22 | shell scripts dotfiles | Good | Good | Good | Okay (1607) | `(?i)\.(sh\|bash\|zsh)$` + scope: ~ (dotfiles prefix) | R7: improved from Bad. `type: shell-scripts` used but regex mangled, no `scope: dotfiles` |
| 23 | pnpm-lock wasting space | Good | Good | Good | Okay (29) | `^pnpm\-lock\.yaml$` regex + min_size | R7: regressed. LLM lost keyword, just `size: large` |
| 24 | contracts last 6mo | Good | Good | Partial | Great (10) | `(?i)contract.*\.(pdf\|doc\|docx\|txt\|odt\|xls\|xlsx)$` + 6mo range | R7: fixed from Bad. `keywords: contract` (singular) + `type: documents` + `time: last_6_months` |
| 25 | .env files | Good | Good | Good | Bad (0) | `(?i)^\.env(\..+)?$` | R7: still bad. `keywords: env` + `type: env-files` merge → `env.*\.env` |
| 26 | sqlite databases | Good | Good | Good | Great (2328) | `(?i)\.(sqlite\|sqlite3\|db)$` | R7: fixed from Okay. `type: databases` only, no spurious fields |
| 27 | markdown notes this month | Bad | Good | Good | Bad (1) | `(?i)note.*\.md$` + this_month range | R7: still bad. `keywords: note` + `type: documents`. Needs markdown type |
| 28 | old xcode projects | Good | Good | Good | Good (0) | `(?i)\.(xcodeproj\|xcworkspace\|pbxproj)$` + before 1yr | R7: fixed. `type: xcode` + `time: old`. 0 is real |
| 29 | package.json not in node_modules | Bad | Good | Partial | Bad (3657) | `^package\.json$` regex + exclude: node_modules | R7: worse. LLM dumped all words as keywords |
| 30 | audio recordings meetings | Good | Good | Good | Okay (4338) | `(?i)meeting.*\.(mp3\|m4a\|flac\|wav\|ogg\|aac)$` | R7: improved from Bad. `type: music` + caveat. No spurious time |

Legend: Good = correct/useful results. Great = correct with tight result count. Perfect = exactly right. Partial = structurally valid but suboptimal (missing synonym, wrong enum choice).
Okay = valid but too broad/narrow. Bad = too many/few results, wrong filters. Error = regex/parse failure. MCP fail = timeout or empty.

## R6 analysis

### What v2 fixed

- **100% parse reliability on both models.** Key-value lines are trivial to produce vs. JSON with regex patterns. Zero
  parse failures across 60 eval runs (30 cloud + 30 local).
- **Zero MCP timeouts.** The LLM never generates regex, so it never produces patterns that cause full-index scans. Even
  when classification is imperfect, the Rust builder produces a bounded query.
- **No more refinement regressions.** Single-pass eliminates the ~15% over-narrowing and ~10% flag-dropping from v1's
  two-pass refinement.
- **Deterministic type/time/size mapping.** Category-to-extension, date arithmetic, and size thresholds are always
  correct. The LLM just picks a token.
- **Local model usability jumped from ~40% to ~80%.** Classification is dramatically easier than JSON generation for
  small models. Most local failures are suboptimal keyword/enum choices, not structural failures.

### What's still imperfect

- **Keyword/type confusion.** LLMs sometimes put the file format as a keyword alongside the type (e.g., `keywords: heic`
  + `type: photos`). The redundant-keyword safety net in `merge_keyword_and_type` handles known extensions, and prompt
  rules reduce the frequency, but novel combinations may still slip through.
- **Exclude reliability on local models.** The 2B model sometimes ignores "not in X" and omits the `exclude` field.
  Cloud models handle this reliably with the strengthened prompt rules.
- **Synonym expansion.** Cloud models expand "kubernetes" to include k8s/kube/helm in keywords. Local models return
  only the literal word. This is acceptable — the user can add terms manually.
- **Markdown not in documents type.** `.md` files are in the `code` type extension list, not `documents`. This means
  "markdown notes this month" requires the LLM to either pick `type: code` (too broad) or omit type and use keyword
  only. The current prompt doesn't have a `markdown` type.
- **Time defaulting.** Some LLMs default to `time: recent` even when the user didn't mention a time period. The prompt
  now explicitly says "Never default to recent/today."

### v2 type table (current)

```
photos          → \.(jpg|jpeg|png|heic|webp|gif)$
screenshots     → ^Screenshot.*\.(png|jpg|heic)$
videos          → \.(mp4|mov|avi|mkv|webm)$
documents       → \.(pdf|doc|docx|txt|odt|xls|xlsx)$
presentations   → \.(ppt|pptx|odp)$
archives        → \.(zip|tar|gz|tgz|bz2|xz|7z|rar)$
music           → \.(mp3|m4a|flac|wav|ogg|aac)$
code            → \.(rs|py|js|ts|go|java|c|cpp|h|rb|swift|svelte|vue)$
rust            → \.rs$
python          → \.py$
javascript      → \.(js|jsx|mjs|cjs)$
typescript      → \.(ts|tsx|mts|cts)$
go              → \.go$
java            → \.java$
config          → \.(json|ya?ml|toml|ini|conf|cfg)$
logs            → \.(log|out|err)$  [+include_system_dirs]
fonts           → \.(ttf|otf|ttc|woff|woff2)$
databases       → \.(sqlite|sqlite3|db)$
xcode           → \.(xcodeproj|xcworkspace|pbxproj)$
shell-scripts   → \.(sh|bash|zsh)$
ssh-keys        → ^(id_(rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)(\.pub)?$
docker-compose  → ^(docker-compose|compose)\.(yml|yaml)$
env-files       → ^\.env(\..+)?$
```

### v2 time enums (current)

```
today, yesterday, this_week, last_week, this_month, last_month,
this_quarter, last_quarter, this_year, last_year,
last_3_months, last_6_months, recent (=3mo), old (=before 1yr),
YYYY, YYYY..YYYY / YYYY-YYYY / YYYY to YYYY / YYYY–YYYY
```

## Classification prompt (current)

This is the exact prompt text the LLM receives. `{TODAY}` is replaced with the current date at runtime.

```
Extract search parameters from the user's file search query.
Return one field per line. Omit fields that don't apply.

keywords:  filename words, space-separated, in the user's language
type:      photos|screenshots|videos|documents|presentations|archives|music|
           code|rust|python|javascript|typescript|go|java|config|logs|fonts|
           databases|xcode|shell-scripts|ssh-keys|docker-compose|env-files|none
time:      today|yesterday|this_week|last_week|this_month|last_month|
           this_quarter|last_quarter|this_year|last_year|last_3_months|last_6_months|
           recent|old|YYYY|YYYY..YYYY
size:      empty|tiny|small|large|huge|>NUMBERmb|>NUMBERgb|<NUMBERmb
scope:     downloads|documents|desktop|dotfiles|PATH
exclude:   dirname1 dirname2
folders:   yes|no
note:      brief limitation caveat if query involves unfilterable concepts

Rules:
- "keywords" = words likely in FILENAMES. Not descriptions.
- Use singular forms for keywords (contract, not contracts).
- "I name them X" / "I mark them as X" → keywords: X (not the descriptive words)
- Only set `time` when the user explicitly mentions a time period. Never default to recent/today.
- Prefer `type` over `keywords` for well-known file categories. Don't put the type name in keywords.
- Don't put the file format in keywords when using a type. "PDF documents" → type: documents.
  "sqlite databases" → type: databases.
- If the user wants ONLY a specific format, use format as keyword without type:
  "HEIC photos I haven't converted" → keywords: .heic / note: can't determine conversion status
- "not in X" / "but not in X" / "excluding X" / "except in X" → ALWAYS use exclude: X
- "ssh keys"/"env files"/"docker compose"/"shell scripts" → type handles this, no keywords needed
- For content/semantic queries ("photos of my cat"), set type + add a note

Examples:
"recent invoices, I mark them rymd" → keywords: rymd / type: documents / time: recent
"大きな動画を削除したい" → type: videos / size: large / note: can't determine safe to delete
"node_modules folders taking up space" → keywords: node_modules / folders: yes / size: large
"screenshots from this week" → type: screenshots / time: this_week
"package.json not in node_modules" → keywords: package.json / exclude: node_modules
"empty folders" → folders: yes / size: empty
"ssh keys" → type: ssh-keys
"foton från förra veckan" → type: photos / time: last_week
"that rust file with the websocket server" → keywords: websocket / type: rust
"old xcode projects" → type: xcode / time: old
"contracts I signed in the last 6 months" → keywords: contract / type: documents / time: last_6_months / note: ...
"shell scripts in my dotfiles" → type: shell-scripts / scope: dotfiles
"HEIC photos I haven't converted" → keywords: .heic / note: can't determine conversion status

Today: {TODAY}.
```

## Key lessons

### v1-era (R1–R5)

- **Examples are behavioral anchors.** Removing them from the prompt causes regressions on those exact query types.
  R4 proved this: compacting away the node_modules, empty folders, and kubernetes examples broke all three.
- **"Be broad" framing is critical for pass 1.** Without it, the LLM over-constrains initial queries, leaving pass 2
  nothing to work with.
- **Regex constraint must be prominent.** The LLM uses lookahead (`(?=...)`) if the constraint is buried.
- **Flag preservation needs explicit rules in the refinement prompt.** Without "preserve all flags from pass 1",
  refinement drops `includeSystemDirs`, `excludeDirs`, etc.
- **Regression guard is essential.** Discard pass 2 if it returns more results than pass 1. Refinement broadens
  instead of narrowing ~10% of the time.
- **Two-pass is powerful but refinement can be destructive.** Over-narrows ~15% of the time (`.md$` → `note*.md`).

### v2-era (R6+)

- **Classification is dramatically easier than generation.** Moving regex/date/JSON generation to Rust eliminated the
  entire category of structural failures. LLM classification is reliable even on 2B models.
- **Key-value lines > JSON for small models.** Zero parse failures. Missing lines = no filter (safe default). Malformed
  lines are individually skippable.
- **Single-pass > two-pass when structure is deterministic.** Refinement added ~15% regression risk and doubled latency.
  With deterministic mapping, the first pass is structurally correct; quality differences come from keyword/enum choice.
- **Redundant keywords need a safety net.** Even with prompt rules, LLMs sometimes put extensions as keywords alongside
  types. The `keyword_redundant_with_type` check in `merge_keyword_and_type` catches these.
- **The type table IS the prompt engineering.** Adding a type (like `shell-scripts`) instantly makes a whole query class
  work. The classification prompt is a thin interface; the power is in the Rust mapping table.
- **Prompt examples still matter for classification.** Adding the "contracts/last_6_months" and "shell scripts/dotfiles"
  examples improved those query categories on both models.
- **Exclude remains the hardest field for local models.** The concept of negation ("not in X") is harder to classify
  than positive categories. Cloud models handle it reliably; local models need more examples.
- **Post-fix R7 confirmed: type table + prompt rules are the highest-leverage fixes.** Adding `shell-scripts`,
  `last_6_months`, and the "time only when explicit" rule fixed 6+ queries each. The 2B model's remaining failures are
  almost entirely "dumps all words as keywords" — a model quality limit, not a prompt/architecture issue.
- **Redundant keyword detection works but merge logic still has gaps.** The `keyword_redundant_with_type` check handles
  known extensions, but `keywords: env` + `type: env-files` still merges to `env.*\.env` because `env` isn't recognized
  as redundant with `env-files`. Edge cases need either broader heuristics or special-case handling.
- **2B model has a "keyword dumping" failure mode.** When confused, the 2B model puts all query words as keywords
  (e.g., `node_modules|folders|taking`) instead of classifying them into the correct fields. This caused 3 of 6 Bad
  results in R7. Cloud model never does this.

## Failure taxonomy (v2)

| Failure mode | Frequency | Example | Mitigation |
|--------------|-----------|---------|------------|
| Keyword dumping (local) | ~10% of queries on 2B model | `node_modules\|folders\|taking` instead of classifying | Model quality limit. Cloud model never does this |
| Keyword/type merge gap | ~5% of kw+type queries | `keywords: env` + `type: env-files` → `env.*\.env` | Broader redundancy heuristics or special-case handling needed |
| Missing exclude (local) | ~15% of exclude queries on local | package.json not in node_modules | Stronger prompt examples, explicit "ALWAYS use exclude" rule |
| Missing capability | Ad hoc | No `scope: downloads` inference, no `markdown` type | Add capabilities as identified |
| Wrong time defaulting | Mostly fixed in R7 | Was ~5% on local, now rare | Prompt rule: "Never default to recent/today" — confirmed effective |
| Type table gap | Mostly fixed | `shell-scripts` and time enums added in R6→R7 | Add to type table + validator + prompt |

## Concrete next steps

R7-local confirmed the architecture is sound. Remaining work is capability gaps and 2B model quality.

1. **Add `markdown` type** — `.md` files don't belong in `documents` (office docs) or `code` (too broad). Would fix
   query #27. Low effort, high impact.
2. **Fix `env-files` merge logic** — `keywords: env` + `type: env-files` produces `env.*\.env` which matches nothing.
   Either treat `env` as redundant with `env-files`, or special-case the merge. Would fix query #25.
3. **Add `scope: downloads` inference** — when the user says "downloads" without other context, infer `scope: downloads`
   instead of treating it as a keyword. Would fix query #18.
4. **Consider a `images` alias for `photos`** — some users say "images" not "photos". Currently falls through to
   `None`.
5. **Extend EXTENSION_KEYWORDS list** as new confusion cases are found. The current list covers the most common
   extensions but isn't exhaustive.
6. **Local model exclude training** — the 2B model's exclude handling could improve with more examples or a dedicated
   "negative filter" example cluster in the prompt.
7. **Evaluate adding `tiff`/`bmp`/`raw`/`cr2`/`nef` to photos type** for photographers.
8. **Evaluate adding `csv` to documents type** — currently only in config via the generic extension list.
9. **Consider cloud fallback for ambiguous queries** — the 3 "keyword dumping" failures (#3, #14, #29) are a 2B model
   quality limit. A heuristic (e.g., >3 keywords emitted) could trigger cloud fallback for these cases.
