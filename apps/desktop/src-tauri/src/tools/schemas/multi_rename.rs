//! Multi-rename tool schema.

use serde_json::{Value, json};

/// The Multi-Rename Tool's knobs, each optional and named as `MultiRenameSpec` names it (a
/// test pins that every spec field is declared here), plus the batch and paging params.
pub fn multi_rename_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "pane": {
                "type": "string",
                "enum": ["left", "right"],
                "description": "Which pane's files to rename. Defaults to the focused pane."
            },
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "File names in the pane's folder, in rename order (what [C] counts by). Defaults to the selection, else the cursor item."
            },
            "nameMask": {
                "type": "string",
                "description": "Name mask (default [N]). Placeholders: [N] name, [N2-5] / [N-3] ranges, [E] extension, [C] counter ([C10+5:3] = start 10, step 5, 3 digits), [P] parent folder, [G] grandparent, [Y] [M] [D] [h] [m] [s] and [YMD] modified time, [U] / [L] / [F] / [n] upper, lower, words, or original case for what follows. [[ is a literal [. Media fields read each file's EXIF and tags: [=exif.taken:YMD] (date taken; [=exif.taken|modified:YMD] falls back to the modified date), exif.camera / make / model / lens / iso / focal / width / height / orientation, tag.title / artist / albumartist / album / track / disc / year / genre / comment ([=tag.track:2] pads to two digits). A file without the field is a noValue row, one that can't be read unreadable. A call reads at most 2,000 media files; past that it's refused as specError tooManyMediaFiles."
            },
            "extensionMask": {
                "type": "string",
                "description": "Extension mask (default [E]), same placeholders."
            },
            "search": { "type": "string", "description": "Search for this in the name (default nothing). a|b with replace x|y swaps in one pass." },
            "replace": { "type": "string", "description": "Replace with (default nothing)." },
            "caseSensitive": { "type": "boolean", "description": "Search matches case (default false)." },
            "firstOnly": { "type": "boolean", "description": "Replace only the first match (default false)." },
            "includeExtension": { "type": "boolean", "description": "Search the extension too (default false)." },
            "regex": { "type": "boolean", "description": "search is a regular expression (default false)." },
            "substitute": { "type": "boolean", "description": "With regex: $1 in replace inserts a group (default false)." },
            "case": {
                "type": "string",
                "enum": ["unchanged", "lower", "upper", "firstUpper", "words"],
                "description": "Case step after search and replace (default unchanged)."
            },
            "removeDiacritics": { "type": "boolean", "description": "Strip accents from Latin and Greek letters (default false)." },
            "greekToLatin": { "type": "boolean", "description": "Write Greek letters in Latin ones, ELOT 743 (default false)." },
            "normalizeUnicode": { "type": "boolean", "description": "Rename to the composed (NFC) Unicode form (default false)." },
            "dryRun": {
                "type": "boolean",
                "description": "true (default): preview only. false: stage every ready row as ONE rename proposal the user reviews and approves in Cmdr. Nothing is ever renamed by this tool."
            },
            "start": { "type": "integer", "minimum": 0, "description": "Dry run: first row of the page (default 0)." },
            "limit": { "type": "integer", "minimum": 1, "maximum": 200, "description": "Dry run: rows per page (default 50, max 200)." },
            "filter": {
                "type": "string",
                "enum": ["all", "problems"],
                "description": "Dry run: page every row (default) or only the ones that can't be renamed as set."
            },
            "rationale": { "type": "string", "description": "Proposal only: why, shown to the user as your words." }
        },
        "required": [],
        "additionalProperties": false
    })
}
