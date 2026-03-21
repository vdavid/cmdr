# AI search v2: deterministic pipeline with LLM classification

Replaces the current "LLM generates a full JSON query" approach with a hybrid architecture where Rust handles all
structural/technical work and the LLM only classifies intent into predefined enums + extracts filename keywords.

## Motivation

The current approach asks the LLM to generate regex patterns, compute ISO dates, estimate byte counts, and produce valid
JSON. Cloud models succeed ~85% of the time; a 2B local model succeeds ~40%. The failures are always in deterministic
work the LLM shouldn't be doing: date arithmetic, regex syntax, JSON structure, category-to-extension mapping.

Meanwhile, the one thing an LLM uniquely provides — understanding natural language across languages — is actually easy
for even small models. The fix: stop asking the LLM to be a compiler and let it be a classifier.

## Design decisions

### LLM classifies, Rust computes

**Why**: The LLM understands "förra veckan" (Swedish), "先月" (Japanese), "la semaine dernière" (French) and maps them
all to the English token `last_week`. Rust then computes the actual Monday→Sunday timestamp range — always correctly.
This separation means the LLM never generates regex, dates, byte counts, or JSON. It picks from predefined enums and
extracts keywords. Classification is dramatically easier than generation, even for 2B models.

### Key-value line output, not JSON

**Why**: JSON generation is the #1 failure mode for small models (13% parse failure rate on 2B). Key-value lines
(`keywords: rymd\ntype: documents\ntime: recent`) are trivial to produce and parse. Missing lines = no filter for that
dimension. Malformed lines are individually skippable without losing the whole response.

### Single pass, no refinement

**Why**: The two-pass refinement system causes regressions ~15% of the time (over-narrowing, flag dropping, broadening).
With deterministic structure, there's nothing to refine — the query is structurally correct on the first try. If results
are too broad, the user narrows manually using the filter UI. This also halves LLM latency.

### Unified pipeline for cloud and local

**Why**: Same code path, same output format. Cloud models give better synonym expansion and keyword selection; local
models give acceptable keywords. The difference is quality, not architecture. No feature flags, no model-size branching.

### Graceful degradation without LLM

**Why**: If the LLM is unavailable, slow, or returns garbage, Rust uses the raw query text as keywords. The user gets a
broad search instead of an error. The LLM is an enhancer, not a gatekeeper.

## Architecture

```
User query: "Recent invoices for Rymdskottkärra, I mark them as 'rymd'"
                                │
                 ┌──────────────┴──────────────┐
                 │  LLM: classify + extract     │
                 │                              │
                 │  Input: the raw query        │
                 │  Output (key-value lines):   │
                 │    keywords: rymd            │
                 │    type: documents           │
                 │    time: recent              │
                 └──────────────┬──────────────┘
                                │
                 ┌──────────────┴──────────────┐
                 │  Rust: map enums → values    │
                 │                              │
                 │  "rymd" → namePattern *rymd* │
                 │  documents → extension regex │
                 │  recent → modifiedAfter 3mo  │
                 │                              │
                 │  → SearchQuery               │
                 └──────────────────────────────┘
```

## LLM interface

### System prompt (~400 tokens, fits in 2K context easily)

```
Extract search parameters from the user's file search query.
Return one field per line. Omit fields that don't apply.

keywords:  filename words, space-separated, in the user's language
type:      photos|screenshots|videos|documents|presentations|archives|music|
           code|rust|python|javascript|typescript|go|java|config|logs|fonts|
           databases|xcode|ssh-keys|docker-compose|env-files|none
time:      today|yesterday|this_week|last_week|this_month|last_month|
           this_quarter|last_quarter|this_year|last_year|recent|old|
           YYYY|YYYY..YYYY
size:      empty|tiny|small|large|huge|>NUMBERmb|>NUMBERgb|<NUMBERmb
scope:     downloads|documents|desktop|dotfiles|PATH
exclude:   dirname1 dirname2
folders:   yes|no
note:      brief limitation caveat if query involves unfilterable concepts

Rules:
- "keywords" = words likely in FILENAMES. Not descriptions.
- "I name them X" / "I mark them as X" → keywords: X (not the descriptive words)
- "not in X" / "excluding X" → exclude: X
- "ssh keys"/"env files"/"docker compose" → type handles this, no keywords needed
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
```

### Why this prompt works for any language

The user's query can be in any language. The LLM understands it natively and outputs:
- `keywords` in the **user's language** (matching their filenames)
- Everything else as **fixed English enum tokens** (which Rust maps to values)

No translation tables needed. No per-language phrase matching. The LLM does what LLMs are good at (language
understanding) and Rust does what Rust is good at (precise computation).

## Rust mapping layer (`ai_query_builder.rs`, new module)

### Type → regex pattern

A lookup table in Rust. Each type maps to a regex pattern and optional flags:

```rust
struct TypeFilter {
    pattern: &'static str,       // regex to match against filename
    include_system_dirs: bool,   // set exclude_system_dirs = false
}

fn type_to_filter(t: &str) -> Option<TypeFilter> {
    Some(match t {
        "photos"          => filter(r"\.(jpg|jpeg|png|heic|webp|gif)$"),
        "screenshots"     => filter(r"^Screenshot.*\.(png|jpg|heic)$"),  // macOS naming convention
        "videos"          => filter(r"\.(mp4|mov|avi|mkv|webm)$"),
        "documents"       => filter(r"\.(pdf|doc|docx|txt|odt|xls|xlsx)$"),
        "presentations"   => filter(r"\.(ppt|pptx|odp)$"),
        "archives"        => filter(r"\.(zip|tar|gz|tgz|bz2|xz|7z|rar)$"),
        "music"           => filter(r"\.(mp3|m4a|flac|wav|ogg|aac)$"),
        "code"            => filter(r"\.(rs|py|js|ts|go|java|c|cpp|h|rb|swift|svelte|vue)$"),
        "rust"            => filter(r"\.rs$"),
        "python"          => filter(r"\.py$"),
        "javascript"      => filter(r"\.(js|jsx|mjs|cjs)$"),
        "typescript"      => filter(r"\.(ts|tsx|mts|cts)$"),
        "go"              => filter(r"\.go$"),
        "java"            => filter(r"\.java$"),
        "config"          => filter(r"\.(json|ya?ml|toml|ini|conf|cfg)$"),
        "logs"            => with_system_dirs(r"\.(log|out|err)$"),
        "fonts"           => filter(r"\.(ttf|otf|ttc|woff|woff2)$"),
        "databases"       => filter(r"\.(sqlite|sqlite3|db)$"),
        "xcode"           => filter(r"\.(xcodeproj|xcworkspace|pbxproj)$"),
        "ssh-keys"        => filter(r"^(id_(rsa|dsa|ecdsa|ed25519)|authorized_keys|known_hosts)(\.pub)?$"),
        "docker-compose"  => filter(r"^(docker-compose|compose)\.(yml|yaml)$"),
        "env-files"       => filter(r"^\.env(\..+)?$"),
        _                 => return None,
    })
}
```

### Combining keywords + type into a single pattern

When both `keywords` AND `type` are present, they are merged into one regex at build time — no second field on
`SearchQuery`. The builder generates a single `name_pattern` that requires BOTH conditions:

- `keywords: rymd` + `type: documents` → `(?i)rymd.*\.(pdf|doc|docx|txt|odt|xls|xlsx)$` (regex)
- `keywords: websocket` + `type: rust` → `(?i)websocket.*\.rs$` (regex)
- `type: screenshots` alone (no keywords) → `(?i)^Screenshot.*\.(png|jpg|heic)$` (regex)
- `keywords: node_modules` alone (no type) → `*node_modules*` (glob)

This keeps `SearchQuery` unchanged — one `name_pattern`, one `pattern_type`. The cost is one regex match per entry, same
as today. The combination logic lives entirely in `ai_query_builder.rs`.

For display purposes, `TranslateDisplay` stores the keyword and type separately so the UI can show them in the
appropriate filter fields.

### Time → timestamps

```rust
fn time_to_range(t: &str) -> (Option<u64>, Option<u64>) {
    let now = OffsetDateTime::now_utc();
    match t {
        "today"        => (Some(start_of_today(now)), None),
        "yesterday"    => (Some(start_of_yesterday(now)), Some(start_of_today(now))),
        "this_week"    => (Some(monday_of_this_week(now)), None),
        "last_week"    => (Some(monday_of_last_week(now)), Some(monday_of_this_week(now))),
        "this_month"   => (Some(first_of_this_month(now)), None),
        "last_month"   => (Some(first_of_last_month(now)), Some(first_of_this_month(now))),
        "this_quarter" => (Some(first_of_this_quarter(now)), None),
        "last_quarter" => (Some(first_of_last_quarter(now)), Some(first_of_this_quarter(now))),
        "this_year"    => (Some(jan1_this_year(now)), None),
        "last_year"    => (Some(jan1_last_year(now)), Some(jan1_this_year(now))),
        "recent"       => (Some(three_months_ago(now)), None),
        "old"          => (None, Some(one_year_ago(now))),
        y if is_year(y) => year_range(y),          // "2024" → Jan 1 2024..Jan 1 2025
        r if is_range(r) => parse_date_range(r),   // "2024..2025", "2024-01..2024-06"
        // is_range accepts "..", "-", "to", "–" (en-dash) as range separators
        _              => (None, None),
    }
}
```

Each helper is a simple `time` crate computation. Always correct. No LLM date math.

### Size → bytes

```rust
fn size_to_filter(s: &str) -> (Option<u64>, Option<u64>) {
    match s {
        "empty"  => (None, Some(0)),
        "tiny"   => (None, Some(100 * KB)),
        "small"  => (None, Some(1 * MB)),
        "large"  => (Some(100 * MB), None),
        "huge"   => (Some(1 * GB), None),
        s if s.starts_with('>') => parse_min_size(&s[1..]),  // ">50mb"
        s if s.starts_with('<') => parse_max_size(&s[1..]),  // "<1gb"
        _        => (None, None),
    }
}
```

### Scope → searchPaths (+ optional name prefix filter)

```rust
struct ScopeResult {
    paths: Vec<String>,
    name_prefix: Option<&'static str>,  // e.g., "." for dotfiles
}

fn scope_to_paths(s: &str) -> ScopeResult {
    match s {
        "downloads" => paths(vec![home.join("Downloads")]),
        "documents" => paths(vec![home.join("Documents")]),
        "desktop"   => paths(vec![home.join("Desktop")]),
        "dotfiles"  => ScopeResult {
            paths: vec![home.to_string()],
            name_prefix: Some("."),  // only match entries starting with "."
        },
        path        => paths(vec![expand_tilde(path)]),
    }
}
```

When `name_prefix` is set and no keyword pattern exists, the builder prepends it to the name pattern (e.g., `.*` glob
for dotfiles). When keywords exist, both constraints are combined into the single regex.

### Keywords → (pattern, pattern_type)

`keywords_to_pattern()` returns a `(String, PatternType)` tuple — the builder always knows whether it produced a glob or
regex, so `pattern_type` is never inferred post-hoc.

Rules:
- Single keyword → `*keyword*` (glob)
- Multiple keywords → `(keyword1|keyword2)` (regex, unanchored)
- Exact filename detection: if the part after the **last** `.` is 2–5 alphabetic characters matching a known extension
  list (pdf, rs, json, yml, etc.), treat as exact filename → `^package\.json$` (regex, anchored). Words like `v2.0` or
  `config.2024` don't match this heuristic and stay as glob patterns.

### Assembly

```rust
fn build_search_query(parsed: &ParsedLlmResponse) -> SearchQuery {
    let type_filter = parsed.type_field.as_deref().and_then(type_to_filter);
    let (time_after, time_before) = parsed.time.as_deref().map(time_to_range).unwrap_or_default();
    let (size_min, size_max) = parsed.size.as_deref().map(size_to_filter).unwrap_or_default();
    let scope = parsed.scope.as_deref().map(scope_to_paths);
    let exclude = parsed.exclude.as_deref().map(parse_exclude_list);
    let is_dir = parsed.folders.as_deref().map(|f| f == "yes");

    // Merge keywords + type into a single name_pattern + pattern_type
    let kw = parsed.keywords.as_deref().map(keywords_to_pattern); // → (String, PatternType)
    let (name_pattern, pattern_type) = merge_keyword_and_type(kw, type_filter.as_ref());

    let include_system_dirs = type_filter.as_ref().is_some_and(|f| f.include_system_dirs);

    SearchQuery {
        name_pattern,
        pattern_type,
        min_size: size_min,
        max_size: size_max,
        modified_after: time_after,
        modified_before: time_before,
        is_directory: is_dir,
        include_paths: scope.as_ref().map(|s| s.paths.clone()),
        exclude_dir_names: exclude,
        exclude_system_dirs: if include_system_dirs { Some(false) } else { None },
        ..Default::default()
    }
}
```

No new fields on `SearchQuery` — the merged pattern goes into the existing `name_pattern`.

### Caveat generation (deterministic)

Caveats are generated by Rust based on rules, not LLM output:

```rust
fn generate_caveat(parsed: &ParsedLlmResponse, query: &SearchQuery) -> Option<String> {
    // LLM provided a note → use it (sanitized: max 200 chars, no HTML)
    if let Some(note) = &parsed.note {
        let sanitized = note.chars().take(200).collect::<String>();
        return Some(sanitized);
    }
    // No name pattern → very broad search
    if query.name_pattern.is_none() {
        return Some("No filename filter — results may be very broad. Add a name or file type to narrow.".into());
    }
    None
}
```

The LLM's `note` field is a fallback for caveats Rust can't infer (e.g., "can't determine which photos are of your
cat"). It's truncated to 200 chars to prevent prompt injection or excessively long strings. Most caveats are
template-based and therefore consistent across languages and model sizes.

## LLM response parser (`ai_response_parser.rs`)

Simple line-by-line parser. No JSON.

```rust
struct ParsedLlmResponse {
    keywords: Option<String>,
    type_field: Option<String>,
    time: Option<String>,
    size: Option<String>,
    scope: Option<String>,
    exclude: Option<String>,
    folders: Option<String>,
    note: Option<String>,
}

fn parse_llm_response(response: &str) -> ParsedLlmResponse {
    let mut parsed = ParsedLlmResponse::default();
    for line in response.lines() {
        let line = line.trim();
        // Use split_once on first `:` only. The value part may contain colons (e.g., scope paths).
        // Keys are always single words so the first colon is always the delimiter.
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            if value.is_empty() { continue; }
            match key.as_str() {
                "keywords" => parsed.keywords = Some(value),
                "type"     => parsed.type_field = validate_type(&value),
                "time"     => parsed.time = validate_time(&value),
                "size"     => parsed.size = validate_size(&value),
                "scope"    => parsed.scope = Some(value),
                "exclude"  => parsed.exclude = Some(value),
                "folders"  => parsed.folders = validate_folders(&value),
                "note"     => parsed.note = Some(value),
                _          => {} // unknown field, skip
            }
        }
    }
    parsed
}
```

`split_once(':')` naturally handles values containing colons (e.g., `scope: /Users/foo/path:with:colons`) — only the
first colon is consumed as the delimiter. This is correct because all keys are single words without colons.

On macOS, filenames cannot contain colons (forbidden by APFS/HFS+), so keywords will never contain them. On Linux,
colons in filenames are unusual but legal; a keyword like `file:name` would be split as key `file` / value `name` and
silently dropped as an unknown key. This is an acceptable edge case.

Each `validate_*` function checks that the value is a known enum member. Unknown values are discarded (treated as if the
field was omitted), so a hallucinated `time: next millennium` becomes `time: None` → no date filter → safe fallback.

### Fallback when LLM fails entirely

If `parse_llm_response` produces an empty `ParsedLlmResponse` (LLM returned garbage), Rust falls back:

1. Split the raw query on whitespace
2. Keep words > 2 characters, limit to 3 most distinctive (longest) words
3. Build a keyword-only SearchQuery with no filters

This gives the user a broad but functional search instead of an error message. The heuristic is intentionally simple and
language-agnostic — no stop word lists needed. Keeping the longest words biases toward content words over grammar
particles in most languages.

## SearchQuery changes

**No new fields.** Keywords and type are merged into a single `name_pattern` regex at build time. The existing rayon scan
is unchanged — one regex match per entry, same as today. The merge logic lives entirely in `ai_query_builder.rs`.

## IPC changes

### `translate_search_query` refactored

The existing command signature stays the same, but internals change completely:

The `PreflightContext` parameter is dropped — this is a breaking IPC change:

```
// Before: translate_search_query(natural_query: String, preflight_context: Option<PreflightContext>)
// After:
translate_search_query(natural_query: String) → Result<TranslateResult, String>
```

The frontend and MCP executor must update their call sites to drop the second argument.

Internal flow:
1. Call LLM with the simple classification prompt → get key-value text
2. Parse with `parse_llm_response` → `ParsedLlmResponse`
3. Map to `SearchQuery` via `build_search_query`
4. Generate display values for the frontend filter UI
5. Return `TranslateResult` (same shape as today)

The `PreflightContext` parameter is removed — no more two-pass.

### `TranslateResult` simplified

- `query`: `TranslatedQuery` (unchanged schema — no new fields)
- `display`: `TranslateDisplay` (human-readable values for the UI, with keyword and type shown separately)
- `caveat`: `Option<String>`
- Remove: `preflight_summary` (no more preflight)

#### Display value generation

`TranslateDisplay` shows the user what the AI interpreted. The builder generates display values from the parsed enums:

- `time: last_week` → `display.modified_after: "2026-03-09"` (the actual computed date, so the user sees what "last
  week" resolved to)
- `type: documents` → `display.pattern_type: "regex"`, `display.name_pattern` shows the merged pattern
- `size: large` → `display.min_size: 104857600`

These are computed alongside the `SearchQuery` values in `build_search_query`, not in a separate pass.

### Frontend changes

- Remove: preflight row, refinement logic, pass-1/pass-2 orchestration, `PreflightContext` types
- Keep: all manual filter UI, AI prompt input, "Ask AI" flow
- Change: `executeAiSearch` becomes a single IPC call instead of a multi-step orchestration
- The `TranslatedQuery` is passed directly to `search_files` — no lossy round-trip through UI state rebuilding

### MCP executor changes

The `ai_search` MCP tool handler in `mcp/executor.rs` currently orchestrates a two-step flow: translate → search →
refine → search. This shrinks to: translate → search. Delete the preflight/refinement orchestration and the
`PreflightContext` building code. The rest of the handler (result formatting, error handling) stays the same.

## What gets deleted

| Current code | Why it's removed |
|---|---|
| `SEARCH_PROMPT_TEMPLATE` (100+ lines) | Replaced by ~30 line classification prompt |
| `REFINEMENT_RULES` | No more refinement |
| `build_refinement_system_prompt()` | No more refinement |
| `format_preflight_table()` | No more preflight |
| `summarize_ai_query()` | No more preflight summary |
| `AiSearchQuery` struct (13 fields) | Replaced by `ParsedLlmResponse` (8 fields) |
| `PreflightContext`, `PreflightEntry` | No more preflight |
| `validate_regex_pattern()` | LLM never generates regex |
| `fix_json_backslash_escapes()` | LLM never outputs JSON |
| `iso_date_to_timestamp()` | LLM never outputs ISO dates |
| `parse_ai_response()` (JSON parser) | Replaced by line parser |
| `call_ai_translate()` | Replaced by single LLM call + line parser + builder |
| `build_translate_result()` | Assembly now done by `build_search_query()` |
| Frontend: preflight row, pass-1/pass-2 orchestration | Single-pass flow |

**Note**: `iso_date_to_timestamp()` is also used by the MCP executor's manual `search` tool for parsing date arguments.
Keep it (or a simpler replacement) for that purpose — only remove it from the AI pipeline.

## What gets added

| New code | Purpose |
|---|---|
| `ai_query_builder.rs` (~350 lines) | Type/time/size/scope/keyword mapping, pattern merging, assembly |
| `ai_response_parser.rs` (~100 lines) | Key-value line parser + validation |
| Classification prompt (~400 tokens) | Simple, multilingual LLM prompt |
| Date computation helpers | `monday_of_this_week()`, `first_of_last_quarter()`, etc. |
| Keyword-to-pattern helpers | Single/multi keyword → glob/regex, exact filename detection |
| `merge_keyword_and_type()` | Combine keyword pattern + type extension into single regex |

## Testing strategy

### Rust unit tests (in `ai_query_builder.rs` and `ai_response_parser.rs`)

**Parser tests:**
- Well-formed response with all fields → correct `ParsedLlmResponse`
- Missing fields → `None` for those fields
- Unknown fields → silently ignored
- Garbage input → empty `ParsedLlmResponse` (no panic)
- Malformed lines (no colon, empty value) → skipped
- Extra whitespace, mixed case keys → handled
- Multi-word values (`keywords: contract agreement`) → preserved
- Validation: unknown type/time/size values → `None`

**Type mapping tests:**
- Every type enum value → correct extension regex (compiles, matches expected filenames)
- `logs` type → `include_system_dirs` flag set
- Unknown type → no filter
- `screenshots` type → anchored name pattern + extensions

**Time mapping tests:**
- Every time enum value → correct timestamp range
- `today` at midnight edge case
- `last_quarter` in January (wraps to previous year)
- `2024` → Jan 1 2024 to Jan 1 2025
- `2024..2025` → correct range
- Invalid year/range → no filter (no panic)

**Size mapping tests:**
- Every size enum value → correct byte range
- `>50mb` → 50 * 1024 * 1024
- `<1gb` → 1 * 1024 * 1024 * 1024
- Invalid size string → no filter

**Scope mapping tests:**
- `downloads` → ~/Downloads
- Literal path → expanded
- `dotfiles` → home dir

**Keyword pattern tests:**
- Single keyword → `*keyword*` glob
- Multiple keywords → `(kw1|kw2)` regex
- Exact filename (`package.json`) → `^package\.json$` anchored regex
- Empty keywords → no pattern

**Pattern merge tests:**
- Keywords + type → single merged regex (e.g., `rymd.*\.(pdf|doc)$`)
- Type only → type regex as name_pattern
- Keywords only → keyword glob/regex as name_pattern
- Type `screenshots` + keywords → anchored screenshot pattern with keyword
- Merged pattern compiles and matches expected filenames on a test index

**Assembly tests:**
- Full query (all fields) → correct SearchQuery with all fields populated
- Caveat generation: LLM note (truncated at 200 chars) vs. Rust-inferred (no name pattern)
- `logs` type → `exclude_system_dirs: Some(false)`
- `dotfiles` scope → correct paths + name prefix constraint

**Fallback tests:**
- Empty LLM response → 3 longest words from raw query used as keywords
- Partial LLM response (only some fields) → partial query built
- LLM timeout → fallback triggers

### Integration tests

- End-to-end: raw query string → `parse_llm_response` → `build_search_query` → `search()` on a test index → verify
  results match expectations
- Merged keyword+type pattern: build query with `keywords: rymd` + `type: documents`, run against test index containing
  `rymd.pdf`, `rymd.rs`, `notes.pdf` → only `rymd.pdf` matches

### Existing tests retained

All `search()` tests, `SearchQuery` serde tests, scope parsing, system dir exclusion tests — these are unaffected by the
AI pipeline rewrite.

## Migration

### IPC contract

`TranslateResult` keeps the same shape minus `preflight_summary`. No new fields on `SearchQuery` or `TranslatedQuery`.
The frontend receives the query and passes it directly to `search_files`.

## Milestones

### M1: Response parser + mapping layer (Rust only, no LLM changes yet)

New files: `commands/ai_response_parser.rs`, `commands/ai_query_builder.rs`. Full test coverage for all parsers, mappers,
the pattern merge logic, and assembly. No changes to existing code yet — all new code is exercised by unit tests only.

### M2: New prompt + pipeline integration

Rewire `translate_search_query` to use the new parser + builder. Replace `SEARCH_PROMPT_TEMPLATE` and `REFINEMENT_RULES`
with the classification prompt. Delete: `AiSearchQuery`, `PreflightContext`, `PreflightEntry`, `build_refinement_system_prompt()`,
`format_preflight_table()`, `summarize_ai_query()`, `validate_regex_pattern()`, `fix_json_backslash_escapes()`,
`iso_date_to_timestamp()`, `parse_ai_response()`. Update MCP executor: remove two-pass orchestration, replace with
single translate→search. Frontend: simplify `executeAiSearch()`, remove preflight row, remove `PreflightContext` types,
remove pass-2 logic.

### M3: Eval + polish

Run the full 30-query eval suite via MCP on both cloud and local models. Compare results to R5 (cloud) and R5-local.
Fix any category table gaps or mapping bugs. Update `CLAUDE.md` files, `docs/architecture.md`, and
`docs/notes/ai-search-eval-history.md` with R6 results.
