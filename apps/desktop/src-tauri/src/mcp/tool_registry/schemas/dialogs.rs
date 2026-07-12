//! Dialog tool schemas.

use serde_json::{Value, json};

pub fn dialog_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["open", "focus", "close", "confirm"],
                "description": "Action to perform. 'confirm' triggers the confirm button on an already-open dialog."
            },
            "type": {
                "type": "string",
                "description": "Dialog type. Openable/focusable: settings, file-viewer, about, onboarding. Closable: any dialog id from cmdr://dialogs/available (also settings, file-viewer). Confirmable: transfer-confirmation (covers copy and move; 'copy-confirmation' is an alias) and delete-confirmation."
            },
            "section": {
                "type": "string",
                "description": "For settings: which section to open (e.g., 'shortcuts')"
            },
            "path": {
                "type": "string",
                "description": "For file-viewer: file path. On open without path, uses cursor file. On close without path, closes all."
            },
            "onConflict": {
                "type": "string",
                "enum": ["skip_all", "overwrite_all", "rename_all"],
                "description": "For confirm action on transfer-confirmation: conflict resolution policy for clashing FILES. Folders always merge (a source folder landing on a same-named dest folder merges into it), and this policy governs the files inside. Default: skip_all"
            }
        },
        "required": ["action", "type"]
    })
}

pub fn open_search_dialog_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Search query to pre-fill in the search bar"
            },
            "mode": {
                "type": "string",
                "enum": ["ai", "filename", "regex"],
                "description": "Search mode. Defaults to 'ai' if AI is enabled, otherwise 'filename'."
            },
            "sizeMin": {
                "type": "integer",
                "description": "Minimum file size in bytes"
            },
            "sizeMax": {
                "type": "integer",
                "description": "Maximum file size in bytes"
            },
            "modifiedAfter": {
                "type": "string",
                "description": "ISO date string (for example, '2025-01-01')"
            },
            "modifiedBefore": {
                "type": "string",
                "description": "ISO date string"
            },
            "isDirectory": {
                "type": "boolean",
                "description": "Type filter: true = folders only, false = files only, omit for both"
            },
            "scope": {
                "type": "string",
                "description": "Scope string, same syntax as the scope chip: comma-separated paths, ! prefix for excludes"
            },
            "caseSensitive": {
                "type": "boolean",
                "description": "Case-sensitive matching"
            },
            "excludeSystemDirs": {
                "type": "boolean",
                "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc.)"
            },
            "autoRun": {
                "type": "boolean",
                "description": "Default true: open and run the search. False: open and prefill without running."
            }
        },
        "required": []
    })
}
