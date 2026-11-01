//! The servers and shares the user saved, below every caller: the servers hub's
//! commands (`commands::servers`, thin IPC over this) and Ask Cmdr
//! (`tools::connect`) read, save, edit, and dial them through the same
//! functions. Module map and must-knows: `CLAUDE.md`.

mod dial;
mod listing;
pub(crate) mod nfs_places;
pub(crate) mod outcome;
mod s3_accounts;
mod saved;
mod saves;
pub(crate) mod smb_hosts;
pub(crate) mod target_ids;
mod wire;

#[cfg(test)]
pub(crate) mod test_fixtures;

pub(crate) use dial::{SavedPlaceDial, cancel_connect, connect_saved_place};
pub(crate) use listing::{listing, saved_now};
pub use outcome::{SavedPlaceRefusal, ServerConnectOutcome};
pub(crate) use outcome::{outcome_from_nfs, outcome_from_s3, outcome_from_sftp, outcome_from_webdav};
pub(crate) use saved::{SavedEntry, saved_by_id, webdav_params};
pub(crate) use saves::{PlaceEdit, save_edit, save_place_edit, save_target};
pub use wire::{SavedPlace, SavedServer, ServerNameSource, ServerProtocol, ServerTarget};
