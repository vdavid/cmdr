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
                "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\")"
            },
            "caseSensitive": {
                "type": "boolean",
                "description": "Case-sensitive matching. Default: false on macOS, true on Linux"
            },
            "excludeSystemDirs": {
                "type": "boolean",
                "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc). Default: true"
            },
            "limit": {
                "type": "integer",
                "description": "Max results to return. Default: 30"
            }
        },
        "required": []
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
                "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\"). Merged with AI-inferred scope."
            },
            "limit": {
                "type": "integer",
                "description": "Max results to return. Default: 30"
            }
        },
        "required": ["query"]
    })
}
