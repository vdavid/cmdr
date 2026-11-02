//! File-operation and tag tool schemas.

use serde_json::{Value, json};

/// The pane an Initiate or LightWrite tool acts on, which every one of them requires: focus moves
/// silently (`nav_to_path` and `select` move it), so an unnamed pane can aim at the wrong one.
fn pane_prop(description: &str) -> Value {
    json!({ "type": "string", "enum": ["left", "right"], "description": description })
}

/// `copy` and `move`'s `onConflict`: a preset the dialog opens on, ❌ never a decision.
fn on_conflict_prop() -> Value {
    json!({
        "type": "string",
        "enum": ["stop", "skip_all", "overwrite_all", "rename_all", "overwrite_smaller_all", "overwrite_older_all"],
        "description": "What the dialog's 'files already exist' choice opens on, for clashing FILES. A person still confirms, and can change it. Omit for their saved choice. Folders always merge. 'stop' asks per file. overwrite_smaller_all / overwrite_older_all overwrite only when the destination is strictly smaller / older, and skip otherwise."
    })
}

pub fn copy_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("The pane to copy from; the other pane's folder is the destination."),
            "onConflict": on_conflict_prop()
        },
        "required": ["pane"]
    })
}

pub fn move_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("The pane to move from; the other pane's folder is the destination."),
            "onConflict": on_conflict_prop()
        },
        "required": ["pane"]
    })
}

pub fn compress_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("The pane to compress from; the archive lands in the other pane's folder.")
        },
        "required": ["pane"]
    })
}

pub fn delete_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("The pane to delete from."),
            "mode": {
                "type": "string",
                "enum": ["trash", "delete"],
                "description": "What the dialog opens on: trash = move to Trash; delete = permanent. Omit for the pane's default (a volume without a trash forces permanent)."
            }
        },
        "required": ["pane"]
    })
}

pub fn rename_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("Which pane to rename in."),
            "name": {
                "type": "string",
                "description": "Current filename to rename. Defaults to the cursor item. Errors if it isn't in the listing."
            },
            "newName": {
                "type": "string",
                "description": "The proposed new name (a name, not a path)."
            }
        },
        "required": ["pane", "newName"]
    })
}

pub fn mkdir_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Folder name: creates it directly. Omit to open the naming dialog for a person."
            },
            "pane": pane_prop("Which pane to create in.")
        },
        "required": ["pane"]
    })
}

pub fn mkfile_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "File name: creates it directly. Omit to open the naming dialog for a person."
            },
            "pane": pane_prop("Which pane to create in.")
        },
        "required": ["pane"]
    })
}

pub fn tag_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": pane_prop("Which pane to act in."),
            "action": {
                "type": "string",
                "enum": ["set", "toggle", "clear"],
                "description": "set | toggle | clear"
            },
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Filenames to tag. Defaults to the pane's selection, and refuses with neither. Errors if a name isn't in the listing."
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
        "required": ["pane", "action"]
    })
}
