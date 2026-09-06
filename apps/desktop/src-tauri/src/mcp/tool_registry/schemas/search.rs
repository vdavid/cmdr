//! Search tool schemas.
//!
//! ⚠️ **A schema is PREFIX**: every property description rides every turn of every
//! conversation, whether or not the turn searches. `search_schema` is the biggest
//! declaration either view carries, so each property here gets ONE line and the
//! knowledge that would fill a paragraph (when to turn the system tier off, what a
//! date means, why there is no `offset`) is stated once in the tool's description
//! in `../table.rs`. ❌ Don't say it in both places.

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
                "description": "Skip system, cache, and build folders (node_modules, .git, Caches). Default: true."
            },
            "sortBy": {
                "type": "string",
                "enum": ["relevance", "size", "modified"],
                "description": "relevance (default), size (biggest first, a folder by its recursive total), or modified (newest first)."
            },
            "countOnly": {
                "type": "boolean",
                "description": "Answer with the counts and coverage alone, no entries. Default: false."
            },
            "limit": {
                "type": "integer",
                "description": "Max entries. Default 30, max 200; a page may come back shorter to fit one result."
            },
            "maxWaitSeconds": {
                "type": "integer",
                "description": "Seconds to wait for the walk, 1-120. Default: 20."
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
                "description": "Max results to return. Default 30, max 200."
            },
            "maxWaitSeconds": {
                "type": "integer",
                "description": "How long to wait for the search, 1-120 (on top of the LLM translation, which runs first). Cmdr walks whatever the index hasn't covered yet, so a first search of a folder takes as long as reading it does. When the wait runs out you get what was found so far plus a note. Default: 20"
            }
        },
        "required": ["query"]
    })
}
