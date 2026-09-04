//! Search tool schemas.

use serde_json::{Value, json};

pub fn search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pattern": {
                "type": "string",
                "description": "Glob or regex filename pattern (for example, \"*.pdf\", \"report*\")"
            },
            "patternType": {
                "type": "string",
                "enum": ["glob", "regex"],
                "description": "Pattern type. Default: glob"
            },
            "sizeMin": {
                "type": "string",
                "description": "Minimum file size, human-readable (for example, \"1 MB\", \"500 KB\")"
            },
            "sizeMax": {
                "type": "string",
                "description": "Maximum file size, human-readable"
            },
            "modifiedAfter": {
                "type": "string",
                "description": "ISO date, for example \"2025-01-01\""
            },
            "modifiedBefore": {
                "type": "string",
                "description": "ISO date"
            },
            "type": {
                "type": "string",
                "enum": ["file", "dir"],
                "description": "Filter by type. Omit for both."
            },
            "scope": {
                "type": "string",
                "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\"). A search covers ONE volume, so every include path must be on the same drive; omit to search the boot volume."
            },
            "caseSensitive": {
                "type": "boolean",
                "description": "Case-sensitive matching. Default: false on macOS, true on Linux"
            },
            "excludeSystemDirs": {
                "type": "boolean",
                "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc). Default: true. Turn it OFF for disk-space questions: those folders are usually where the space went. The result says how many matches this hid."
            },
            "sortBy": {
                "type": "string",
                "enum": ["relevance", "size", "modified"],
                "description": "Result order. Default 'relevance' (best name match, then recency). 'size' returns the biggest matches that exist anywhere in scope, files and folders on one scale (a folder by its recursive total) — use it with excludeSystemDirs: false to find where disk space went. 'modified' returns the newest."
            },
            "countOnly": {
                "type": "boolean",
                "description": "Set true when you only need the total, not the results. Returns just the match count (for example, \"1,234 files match\") and skips the file list. Faster than a full search. Default: false"
            },
            "limit": {
                "type": "integer",
                "description": "Max results to return. Default: 30"
            },
            "maxWaitSeconds": {
                "type": "integer",
                "description": "How long to wait for the answer, 1-120. Cmdr walks whatever the index hasn't covered yet, so a first search of a folder takes as long as reading it does. When the wait runs out you get what was found so far plus a note; the walk keeps going, so running the same search again picks up the rest. Default: 20"
            }
        },
        "required": [],
        // Closed because `search` is in the agent view, where nobody reads a call before it
        // runs: an undeclared property (a guessed `name`, a guessed `contains`) is refused by
        // `validate_params` instead of being swallowed into a confident empty answer.
        "additionalProperties": false
    })
}

pub fn ai_search_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Natural language search query (for example, \"recent invoices marked rymd\")"
            },
            "scope": {
                "type": "string",
                "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\"). Merged with AI-inferred scope. A search covers ONE volume, so every include path must be on the same drive; omit to search the boot volume."
            },
            "limit": {
                "type": "integer",
                "description": "Max results to return. Default: 30"
            },
            "maxWaitSeconds": {
                "type": "integer",
                "description": "How long to wait for the search, 1-120 (on top of the LLM translation, which runs first). Cmdr walks whatever the index hasn't covered yet, so a first search of a folder takes as long as reading it does. When the wait runs out you get what was found so far plus a note. Default: 20"
            }
        },
        "required": ["query"]
    })
}
