//! The real [`ServerSaver`]: the saved stores as the servers hub reads them, and each save
//! through the hub's own core. One arm per core, nothing else, so an approved card saves
//! exactly what "Add anyway" or the edit sheet would.

use futures_util::future::BoxFuture;

use super::{SaveResult, SavedServers, ServerSave, ServerSaver};
use crate::network::manual_servers::{self, HostEdit};
use crate::saved_servers::{save_place_edit, save_target, smb_hosts};

/// The servers hub's stores and save cores.
pub(crate) struct HubSaver {
    app: tauri::AppHandle,
}

impl HubSaver {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl ServerSaver for HubSaver {
    fn saved(&self) -> SavedServers {
        SavedServers::now(&self.app)
    }

    fn save(&self, save: ServerSave) -> BoxFuture<'static, SaveResult> {
        let app = self.app.clone();
        Box::pin(async move {
            match save {
                ServerSave::Add(target) => SaveResult::of_outcome(save_target(target).await),
                ServerSave::Edit { id, edit } => SaveResult::of_outcome(save_place_edit(&id, edit).await),
                // ❌ Never a checked `add_manual_server`: that one probes and joins the discovery list at once.
                ServerSave::SmbAdd {
                    address,
                    name,
                    username,
                } => SaveResult::of_smb_add(manual_servers::save_manual_server_without_contact(
                    &address,
                    &HostEdit { name, username },
                    &app,
                )),
                // No address, so the edit saves in place: it can't move the host or its passwords.
                ServerSave::SmbHostEdit {
                    host_id,
                    name,
                    username,
                } => SaveResult::of_outcome(
                    smb_hosts::edit_host(&host_id, HostEdit { name, username }, None, &app).await,
                ),
            }
        })
    }
}
