//! `about_item`: what one file or folder is, where it came from, what owns it,
//! and what removing it would mean. The facts "Tell me about this" shows, read by
//! the card's own host (`crate::about::lookup`), through the one model projection.
//!
//! ❌ It never looks inside the item: contents stay `inspect_file`'s, behind its
//! sensitive gate. It reads xattrs, an `Info.plist`, a signature, and Spotlight
//! with Cmdr's Full Disk Access, which is why it's `ReadContents`: an outside
//! client off direct mode is refused (`tools/DETAILS.md` § `about_item`).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use cmdr_about::fact::FactKind;
use cmdr_about::model::ModelCard;

use crate::about::Started;
use crate::about::lookup::look_up;
use crate::location::Location;
use crate::location::resolve::LocationInput;
use crate::tools::actions::{ToolError, ToolResult};
use crate::tools::policy::{Caller, ToolCtx};

pub fn about_item_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Absolute or ~-relative file or folder." },
            "volumeId": { "type": "string", "description": "From list_volumes or a result's location; omit for a path on a connected volume." }
        },
        "required": ["path"],
        "additionalProperties": false
    })
}

/// What the tool answers.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AboutItemResult {
    location: Location,
    /// Each row as `{ state, value }`: `known`, `unknown` with its reason,
    /// `notApplicable`, or `pending`.
    about: ModelCard,
    /// Where each row's answer comes from, so a model repeats an item's own claim
    /// as a claim.
    how_cmdr_knows: BTreeMap<FactKind, &'static str>,
    /// Rows that hadn't answered when Cmdr stopped waiting.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    still_gathering: Vec<FactKind>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<&'static str>,
}

const STILL_GATHERING: &str = "The rows in stillGathering hadn't answered when Cmdr stopped waiting: say Cmdr couldn't tell those yet, never that there's nothing.";
const REFUSED_IN_BACKGROUND: &str = "Cmdr started this turn itself, so it didn't ask the drive this item is on (refusedInBackground). The user can ask, and Cmdr will look then.";

/// Where a row's answer comes from, in a model's words. Exhaustive, so a new row
/// says where it comes from before it can ship.
fn how_cmdr_knows(kind: FactKind) -> &'static str {
    match kind {
        FactKind::Identity => "The file system's own record.",
        FactKind::RealType => "Cmdr matched the first bytes against known formats.",
        FactKind::Bundle => "The app's Info.plist: what the app says about itself.",
        FactKind::Signature => {
            "The signature the item carries. Only checked: executableMatches means Cmdr compared the program to it; otherwise say it carries a signature from that signer."
        }
        FactKind::Download => {
            "What the downloading app wrote on the file. It travels with the file and anyone can write it: say it was recorded as downloaded from that site, never that it came from there."
        }
        FactKind::CmdrOperation => "Cmdr's own operation log, matched to this exact file on disk.",
        FactKind::LastOpened => "Spotlight's record. No record isn't never opened.",
        FactKind::DefaultApp => "The app the OS opens it with.",
        FactKind::Importance | FactKind::RecursiveSize | FactKind::FolderDigest | FactKind::AppData => {
            "Cmdr's drive index, as of its last scan."
        }
        FactKind::Owner => {
            "By type: byBundleId matched an app's id, byName only a folder name (say probably), installedBy an installer's receipt, homebrew Homebrew's folders, insideApp the app around it."
        }
        FactKind::Git => {
            "The git repository around it. notInStatus means git listed no change, not that it's committed."
        }
        FactKind::GitSnapshot => "The repository's history, as Cmdr's .git folder shows it.",
    }
}

pub async fn execute_about_item<R: Runtime>(_app: &AppHandle<R>, ctx: &ToolCtx, params: &Value) -> ToolResult {
    let input =
        LocationInput::from_params(params).ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;
    let looked_up = match look_up(input.path, input.volume_id, started(ctx.caller), &ctx.cancel).await {
        Ok(looked_up) => looked_up,
        Err(miss) => return to_value(&miss),
    };
    let mut notes = Vec::new();
    if !looked_up.still_out.is_empty() {
        notes.push(STILL_GATHERING);
    }
    if looked_up.refused_in_background {
        notes.push(REFUSED_IN_BACKGROUND);
    }
    to_value(&AboutItemResult {
        location: looked_up.location,
        about: looked_up.card,
        how_cmdr_knows: looked_up
            .rows
            .into_iter()
            .map(|kind| (kind, how_cmdr_knows(kind)))
            .collect(),
        still_gathering: looked_up.still_out,
        notes,
    })
}

/// Who started the lookup. An outside client reaches the handler only in direct
/// mode (`admit`), a developer or a test at the keyboard.
fn started(caller: Caller) -> Started {
    match caller {
        Caller::Agent { origin } if !origin.user_started() => Started::ByCmdr,
        Caller::Agent { .. } | Caller::External { .. } => Started::ByPerson,
    }
}

fn to_value(value: &impl Serialize) -> ToolResult {
    serde_json::to_value(value).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
