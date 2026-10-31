//! The servers a person let SMB links sign in to, on disk.
//!
//! The answers themselves live in `cmdr_smb::link_consent`, which every smb2
//! session asks; this file is what makes them outlast a launch, and the only way
//! one gets added. Why it exists: `crates/cmdr-smb/DETAILS.md` § "Signing in
//! through a link".
//!
//! ❗ A file that doesn't read is set aside and reads as nothing allowed
//! (`server_list_file::read`): the links ask again, and no sign-in goes anywhere
//! on the strength of a guess.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ignore_poison::IgnorePoison;
use crate::network::server_list_file;
use crate::state_recovery::StateFile;

/// One allowed pair: links on `server` may sign in to `target`, with the account
/// used for `server`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SmbLinkConsent {
    /// The server the share is on.
    pub server: String,
    /// The other server the link leads to.
    pub target: String,
    /// When it was allowed, ISO 8601, so the settings screen can show it.
    pub approved_at: String,
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmbLinkConsentsStore {
    #[serde(default, deserialize_with = "crate::state_recovery::lenient_entries")]
    consents: Vec<SmbLinkConsent>,
}

static STORE: Mutex<SmbLinkConsentsStore> = Mutex::new(SmbLinkConsentsStore { consents: Vec::new() });
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Loads the store and hands the pairs to `cmdr_smb`. One call at startup, ❗
/// before any volume is built, or a share behind an allowed link asks again.
pub fn load_smb_link_consents<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some((store, path)) =
        server_list_file::load::<R, SmbLinkConsentsStore>(app, "smb-link-consents.json", StateFile::SmbLinkConsents)
    else {
        return;
    };
    adopt(store);
    let _ = STORE_PATH.set(path);
}

fn adopt(store: SmbLinkConsentsStore) {
    cmdr_smb::LinkConsents::replace_all(
        store
            .consents
            .iter()
            .map(|entry| (entry.server.clone(), entry.target.clone())),
    );
    *STORE.lock_ignore_poison() = store;
}

fn save() {
    let Some(path) = STORE_PATH.get() else {
        return; // Before the load there's nowhere to write. Tests land here.
    };
    let store = STORE.lock_ignore_poison().clone();
    server_list_file::save(path, "the servers SMB links may sign in to", &store);
}

/// Remembers that links on `server` may sign in to `target`. ❗ Only a person's
/// yes reaches this: no MCP or Ask Cmdr tool calls it. Answers `false` for a name
/// `cmdr_smb` refuses to ask about.
pub fn allow(server: &str, target: &str) -> bool {
    if !cmdr_smb::LinkConsents::allow(server, target) {
        return false;
    }
    {
        let mut store = STORE.lock_ignore_poison();
        if !store.consents.iter().any(|e| e.server == server && e.target == target) {
            store.consents.push(SmbLinkConsent {
                server: server.to_string(),
                target: target.to_string(),
                approved_at: chrono::Utc::now().to_rfc3339(),
            });
        }
    }
    save();
    true
}

/// Takes the pair back, so links on `server` ask again on their next call.
/// Answers whether anything was there.
pub fn forget(server: &str, target: &str) -> bool {
    cmdr_smb::LinkConsents::forget(server, target);
    let removed = {
        let mut store = STORE.lock_ignore_poison();
        let before = store.consents.len();
        store.consents.retain(|e| !(e.server == server && e.target == target));
        store.consents.len() != before
    };
    if removed {
        save();
    }
    removed
}

/// Every allowed pair, for the settings screen.
pub fn list() -> Vec<SmbLinkConsent> {
    STORE.lock_ignore_poison().consents.clone()
}

#[cfg(test)]
#[path = "smb_link_consents_test.rs"]
mod smb_link_consents_test;
