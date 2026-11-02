//! Queue tool schema.

use serde_json::{Value, json};

pub fn queue_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["pause", "resume", "cancel", "continue_without_gone", "skip_locked", "pause_all", "resume_all"],
                "description": "pause | resume | cancel | continue_without_gone | skip_locked | pause_all | resume_all. pause acts on a running operation; a queued one isn't touching a device yet, so pausing it is refused rather than silently ignored. cancel keeps what's already written. continue_without_gone answers an operation's pendingSourcesGone question by carrying on with the sources still there (cancel is the other answer). skip_locked answers a move's pendingLockedSources question by leaving the Locked items where they are and moving the rest (cancel is another answer)."
            },
            "operationId": {
                "type": "string",
                "description": "Operation to act on (required for pause / resume / cancel / continue_without_gone / skip_locked unless operationIds is given). See cmdr://state operations."
            },
            "operationIds": {
                "type": "array",
                "items": { "type": "string" },
                "description": "For cancel only: several operations to cancel at once."
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

pub fn queue_move_locked_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operationId": {
                "type": "string",
                "description": "The move parked on a pendingLockedSources question. See cmdr://state operations."
            }
        },
        "required": ["operationId"],
        "additionalProperties": false
    })
}

pub fn resolve_conflict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operationId": {
                "type": "string",
                "description": "The operation parked on the clash. From the pendingConflict block in cmdr://state operations."
            },
            "conflictId": {
                "type": "integer",
                "description": "Which clash of that operation you're answering, from the same pendingConflict block. Required: an operation raises its clashes one at a time, and naming the one you saw is what stops your answer from landing on the next one."
            },
            "resolution": {
                "type": "string",
                "enum": ["skip", "overwrite", "rename", "overwrite_smaller", "overwrite_older"],
                "description": "skip leaves the destination alone | overwrite replaces it | rename keeps both (the copy lands as 'name (1).ext') | overwrite_smaller and overwrite_older replace only when the destination is strictly smaller / strictly older, and skip otherwise."
            },
            "applyToAll": {
                "type": "boolean",
                "description": "Apply this answer to every later clash in the same operation, so it stops asking. Default false: this answers one file."
            }
        },
        "required": ["operationId", "conflictId", "resolution"]
    })
}
