//! The names people typed for their manual SMB servers, kept in memory so a label
//! can use one where no `AppHandle` reaches (the volume list built off the mount
//! table, the saved-share rows).
//!
//! ❗ A mirror of the APP's store file only: refreshed every time Cmdr reads or writes
//! that file, ❌ never from a test's temp path, which would put a fixture's names on
//! another cell's labels.

use super::ManualServerEntry;
use crate::ignore_poison::RwLockIgnorePoison as _;
use crate::network::NetworkHost;
use crate::network::server_identity::SmbServer;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Where the app's own store lives, learned the first time anything asks for it.
static APP_STORE: OnceLock<PathBuf> = OnceLock::new();

/// Each NAMED manual server, as the store last read or wrote it.
static TYPED_NAMES: RwLock<Vec<(SmbServer, String)>> = RwLock::new(Vec::new());

/// Notes the app's store path, so [`mirror`] can tell it from a test's.
pub(super) fn note_app_store(path: &Path) {
    let _ = APP_STORE.set(path.to_path_buf());
}

/// Refreshes the mirror from `servers` when `path` is the app's store.
pub(super) fn mirror(path: &Path, servers: &[ManualServerEntry]) {
    if APP_STORE.get().map(PathBuf::as_path) != Some(path) {
        return;
    }
    *TYPED_NAMES.write_ignore_poison() = named(servers);
}

/// The named entries of `servers`, each by where it dials.
fn named(servers: &[ManualServerEntry]) -> Vec<(SmbServer, String)> {
    servers
        .iter()
        .filter(|entry| entry.is_named())
        .map(|entry| (SmbServer::new(&entry.address, entry.port), entry.label()))
        .collect()
}

/// The name a person typed for the server any of `candidates` is (the same port, the
/// same machine under any name discovery pairs it with), or `None` when nobody named it.
/// Asks discovery only when some server has a name.
pub fn typed_name(candidates: &[SmbServer]) -> Option<String> {
    let named = TYPED_NAMES.read_ignore_poison();
    if named.is_empty() {
        return None;
    }
    typed_name_among(&named, candidates, &crate::network::fresh_discovered_hosts())
}

/// [`typed_name`] over `named`, so a test needs no store.
pub(crate) fn typed_name_among(
    named: &[(SmbServer, String)],
    candidates: &[SmbServer],
    hosts: &[NetworkHost],
) -> Option<String> {
    named
        .iter()
        .find(|(server, _)| candidates.iter().any(|candidate| server.is(candidate, hosts)))
        .map(|(_, name)| name.clone())
}

/// [`named`] for a test that builds its own table.
#[cfg(test)]
pub(crate) fn named_for_test(servers: &[ManualServerEntry]) -> Vec<(SmbServer, String)> {
    named(servers)
}
