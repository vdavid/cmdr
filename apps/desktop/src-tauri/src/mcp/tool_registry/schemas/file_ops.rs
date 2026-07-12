//! File-operation and tag tool schemas.

use serde_json::{Value, json};

pub fn copy_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "autoConfirm": {
                "type": "boolean",
                "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
            },
            "onConflict": {
                "type": "string",
                "enum": ["skip_all", "overwrite_all", "rename_all"],
                "description": "Conflict resolution policy for clashing FILES (only when autoConfirm is true). Folders always merge: a source folder landing on a same-named dest folder merges into it, and this policy governs the files inside. Default: skip_all"
            }
        },
        "required": []
    })
}

pub fn move_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "autoConfirm": {
                "type": "boolean",
                "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
            },
            "onConflict": {
                "type": "string",
                "enum": ["skip_all", "overwrite_all", "rename_all"],
                "description": "Conflict resolution policy for clashing FILES (only when autoConfirm is true). Folders always merge: a source folder landing on a same-named dest folder merges into it, and this policy governs the files inside. Default: skip_all"
            }
        },
        "required": []
    })
}

pub fn compress_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "autoConfirm": {
                "type": "boolean",
                "description": "When true, the dialog opens and immediately confirms without waiting for user interaction, returning once the compress starts. Exception: if the target archive already exists, the dialog stays open for the user to confirm the overwrite rather than replacing it silently."
            }
        },
        "required": []
    })
}

pub fn delete_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "autoConfirm": {
                "type": "boolean",
                "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
            },
            "mode": {
                "type": "string",
                "enum": ["trash", "delete"],
                "description": "trash = move to Trash; delete = permanent. Omit for the pane's default (a volume without a trash forces permanent)."
            }
        },
        "required": []
    })
}

pub fn rename_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to rename in. Defaults to the focused pane."
            },
            "name": {
                "type": "string",
                "description": "Current filename to rename. Defaults to the cursor item. Errors if it isn't in the listing."
            },
            "newName": {
                "type": "string",
                "description": "The proposed new name (a name, not a path)."
            },
            "autoConfirm": {
                "type": "boolean",
                "description": "When true, renames directly without the review editor. Returns once the rename lands."
            }
        },
        "required": ["newName"]
    })
}

pub fn mkdir_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Folder name. Omit to open the dialog with the default prefill."
            },
            "autoConfirm": {
                "type": "boolean",
                "description": "With a name, create directly without the dialog. Returns once created."
            },
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Target pane. Defaults to the focused pane."
            }
        },
        "required": []
    })
}

pub fn mkfile_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "File name. Omit to open the dialog with the default prefill."
            },
            "autoConfirm": {
                "type": "boolean",
                "description": "With a name, create directly without the dialog. Returns once created."
            },
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Target pane. Defaults to the focused pane."
            }
        },
        "required": []
    })
}

pub fn tag_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane to act in. Defaults to the focused pane."
            },
            "action": {
                "type": "string",
                "enum": ["set", "toggle", "clear"],
                "description": "set | toggle | clear"
            },
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filenames to tag. Defaults to the selection, else the cursor item. Errors if a name isn't in the listing."
            },
            "colors": {
                "type": "array",
                "items": {
                    "type": "string",
                    "enum": ["red", "orange", "yellow", "green", "blue", "purple", "gray"]
                },
                "description": "Finder color names. Required for set and toggle; ignored for clear."
            }
        },
        "required": ["action"]
    })
}
