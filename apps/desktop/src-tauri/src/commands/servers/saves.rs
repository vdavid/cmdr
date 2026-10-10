//! The per-protocol save behind `update_saved_server`: an add without a dial, an
//! edit in place, or an edit that moves a server to a new address. Split out of
//! `servers.rs` to keep that file under its length cap; the facade's rationale
//! stays there.

use super::outcomes::{SavedEntry, saved_by_id};
use super::s3_accounts::s3_place;
use super::wire::ServerTarget;
use crate::network::s3_known_places::KnownS3Place;
use crate::network::s3_volume_wiring;
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::sftp_volume_wiring;
use crate::network::webdav_known_servers::KnownWebdavServer;
use crate::network::webdav_volume_wiring;

/// Saves `server` with no saved entry behind the request: an "Add anyway".
///
/// ❗ Calls each wiring's `save_without_connecting` directly: there is no
/// per-protocol IPC command behind it any more (nothing but its own test ever
/// called `update_known_sftp_server` / `update_known_webdav_server`), so this
/// facade builds the known-servers entry itself instead of forwarding.
pub(super) async fn save_target(server: ServerTarget) -> SavedServerOutcome {
    save_entry(entry_of(server)).await
}

async fn save_entry(entry: Entry) -> SavedServerOutcome {
    match entry {
        Entry::Sftp(entry) => sftp_volume_wiring::save_without_connecting(entry).await,
        Entry::Webdav(entry) => webdav_volume_wiring::save_without_connecting(entry).await,
        Entry::S3 { place, account_name } => s3_volume_wiring::save_without_connecting(place, &account_name).await,
    }
}

/// Saves an edit to the saved place `id` names: in place when the address is the
/// same, else as a move to the new address (`server_move.rs`).
///
/// ❗ A place nobody saved any more (a Forget in another pane) answers
/// `Unreachable`, the save nobody could confirm, ❌ never a quiet re-save. One
/// naming another protocol or account answers `AccountChanged`.
pub(super) async fn save_edit(id: &str, server: ServerTarget) -> SavedServerOutcome {
    let Some(saved) = saved_by_id(id) else {
        return SavedServerOutcome::Unreachable;
    };
    match (saved, entry_of(server)) {
        (SavedEntry::Sftp(saved), Entry::Sftp(edit)) => crate::server_move::edit_sftp(saved, edit).await,
        (SavedEntry::Webdav(saved), Entry::Webdav(edit)) => crate::server_move::edit_webdav(saved, edit).await,
        // A place's endpoint is its ACCOUNT's, which only the account's own edit
        // moves (`update_saved_s3_account`), so a place's edit keeps its id.
        (SavedEntry::S3(_), entry @ Entry::S3 { .. }) if entry.s3_place_id().as_deref() == Some(id) => {
            save_entry(entry).await
        }
        _ => SavedServerOutcome::AccountChanged,
    }
}

/// The saved entry a target describes.
enum Entry {
    Sftp(KnownSftpServer),
    Webdav(KnownWebdavServer),
    /// One place under an account, and the ACCOUNT's name the target carried
    /// (`s3_known_places::adopt_typed_name`).
    S3 {
        place: KnownS3Place,
        account_name: String,
    },
}

impl Entry {
    fn s3_place_id(&self) -> Option<String> {
        match self {
            Self::S3 { place, .. } => place.volume_id(),
            _ => None,
        }
    }
}

/// The store entry `server` describes.
///
/// ❗ `pinned: true` and a fresh `last_connected_at` are read only for a NEW
/// entry: `remember` keeps a stored pin, and a move keeps both.
fn entry_of(server: ServerTarget) -> Entry {
    match server {
        ServerTarget::Sftp {
            display_name,
            host,
            port,
            username,
            remote_root,
            start_folder,
            key_file,
            use_agent,
            auto_reconnect,
        } => Entry::Sftp(KnownSftpServer {
            host,
            port,
            username,
            display_name,
            remote_root,
            start_folder,
            key_file,
            use_agent,
            auto_reconnect,
            pinned: true,
            last_connected_at: chrono::Utc::now().to_rfc3339(),
        }),
        ServerTarget::Webdav {
            display_name,
            url,
            username,
            remote_root,
            start_folder,
            auto_reconnect,
        } => Entry::Webdav(KnownWebdavServer {
            url,
            username,
            display_name,
            remote_root,
            start_folder,
            auto_reconnect,
            pinned: true,
            last_connected_at: chrono::Utc::now().to_rfc3339(),
        }),
        ServerTarget::S3 {
            display_name,
            provider,
            access_key_id,
            bucket,
            auto_reconnect,
        } => Entry::S3 {
            place: s3_place(provider, access_key_id, bucket, auto_reconnect),
            account_name: display_name,
        },
    }
}
