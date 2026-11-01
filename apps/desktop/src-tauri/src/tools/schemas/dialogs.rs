//! Dialog tool schemas.

use serde_json::{Value, json};

pub fn dialog_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["open", "focus", "close"],
                "description": "Action to perform."
            },
            "type": {
                "type": "string",
                "description": "Dialog type. Openable/focusable: settings, file-viewer, about, onboarding. Closable: any dialog id from cmdr://dialogs/available (also settings, file-viewer). 'close' on quit-confirmation calls the quit off and keeps what's running; the quit tool raises it, never 'open'. 'close' on rename ends the inline rename editor without saving."
            },
            "section": {
                "type": "string",
                "description": "For settings: which section to open (e.g., 'shortcuts')"
            },
            "path": {
                "type": "string",
                "description": "For file-viewer: file path. On open without path, uses cursor file. On close without path, closes all."
            },
        },
        "required": ["action", "type"]
    })
}

pub fn dialog_confirm_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "type": {
                "type": "string",
                "description": "What to confirm: transfer-confirmation (covers copy and move; 'copy-confirmation' is an alias), delete-confirmation, rename (the inline editor), mkdir-confirmation, new-file-confirmation, or quit-confirmation (quits now and stops what's running). A confirm on transfer-confirmation or delete-confirmation returns the started operation's id ('operationId: <id>' in the OK text) for await operation_complete or queue. A delete-confirmation trash of a cloud-drive folder answers once the folder's scan rules out online-only files, which can take up to about 90 seconds. A press the dialog itself stopped (a refused source, a press already in flight) refuses with data.refusal = 'notStarted' and leaves the dialog open."
            },
            "name": {
                "type": "string",
                "description": "For rename, mkdir-confirmation, and new-file-confirmation: the name to commit. Omit to commit what the field holds. A name that doesn't stick refuses with data.refusal = 'nameRefused' and leaves it open. A rename that copies too much to start unasked refuses with data.refusal = 'needsMoveDialog', having opened the Move dialog."
            },
            "background": {
                "type": "boolean",
                "description": "For transfer-confirmation or delete-confirmation: press the dialog's Background button (F2) instead of Confirm, so the operation starts with no progress dialog. A delete only runs in the background as a trash: a dialog set to delete permanently refuses with data.refusal = 'permanentDelete' and stays open. Default: false"
            },
            "onConflict": {
                "type": "string",
                "enum": ["stop", "skip_all", "overwrite_all", "rename_all", "overwrite_smaller_all", "overwrite_older_all"],
                "description": "For transfer-confirmation: conflict resolution policy for clashing FILES. Folders always merge (a source folder landing on a same-named dest folder merges into it), and this policy governs the files inside. 'stop' decides nothing upfront: the operation asks per file and parks until you answer each one with resolve_conflict (watch cmdr://state operations for the pendingConflict block). The *_all policies decide every clash upfront. overwrite_smaller_all / overwrite_older_all overwrite only when the destination is strictly smaller / strictly older, and skip otherwise. Default: the policy the dialog shows."
            }
        },
        "required": ["type"]
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
                "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc.). Defaults on; turn it off for disk-space questions, where those folders are usually the answer."
            },
            "autoRun": {
                "type": "boolean",
                "description": "Default true: open and run the search. False: open and prefill without running."
            }
        },
        "required": []
    })
}
