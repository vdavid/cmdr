//! Cmdr's own history for a card: which operation put the item where it is.
//!
//! One read of `operation-log.db`, never the disk. The log answers only for the
//! very node an operation landed (`cmdr_operation_log::query::landed_at`), so a
//! file replaced since reads `NodeChangedSince` and one with no record reads
//! `NoRecord`: the card never credits an item with another file's history.

use std::path::{Path, PathBuf};

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::provenance::{CmdrOperation, LandedBy, StartedBy};
use cmdr_about::text::ItemText;
use cmdr_fs::volume::{DEFAULT_VOLUME_ID, NodeIdentity};
use cmdr_operation_log::query::{LandedAt, landed_at};
use cmdr_operation_log::store::{OperationLogStoreError, open_read_connection};
use cmdr_operation_log::types::{Initiator, OpKind};

/// Where a card reads Cmdr's history from, and the volume its item is on.
#[derive(Debug, Clone)]
pub(crate) struct History {
    /// `operation-log.db`.
    pub(crate) db: PathBuf,
    /// The item's volume, as the resolver named it.
    pub(crate) volume_id: String,
}

impl History {
    /// The app's open log, for an item on `volume_id`. `None` when the log
    /// isn't open: the card then has no history row.
    pub(crate) fn of_the_app(volume_id: &str) -> Option<Self> {
        Some(Self {
            db: crate::operation_log_host::db_path()?,
            volume_id: volume_id.to_string(),
        })
    }
}

/// The operation that landed the item at `path`, for the app's volumes.
pub(crate) fn cmdr_operation(history: &History, path: &str, node: Option<NodeIdentity>) -> Fact<CmdrOperation> {
    cmdr_operation_naming(history, path, node, &registered_volume_name)
}

/// A registered volume's display name. A lookup in the registry, which never
/// dials or touches the volume.
fn registered_volume_name(volume_id: &str) -> Option<String> {
    crate::file_system::volume::manager::get_volume_manager()
        .get(volume_id)
        .map(|volume| volume.name().to_string())
}

/// [`cmdr_operation`], naming a source volume through `volume_name`.
pub(crate) fn cmdr_operation_naming(
    history: &History,
    path: &str,
    node: Option<NodeIdentity>,
    volume_name: &dyn Fn(&str) -> Option<String>,
) -> Fact<CmdrOperation> {
    // No node to ask about (a filesystem with no birth time): the log records
    // none there either, so nothing it holds could be tied to this item.
    let Some(node) = node else {
        return Fact::unknown(UnknownWhy::NoRecord);
    };
    if !history.db.exists() {
        return Fact::unknown(UnknownWhy::NoRecord);
    }
    match read(history, Path::new(path), node) {
        Ok(Some((landed_on, landed))) => fact_of(landed, &landed_on, volume_name),
        Ok(None) => Fact::unknown(UnknownWhy::NoRecord),
        Err(error) => {
            // The kind of failure, ❌ never the path: logs ride error reports.
            log::warn!(target: "about", "couldn't read the operation log for a card: {error}");
            Fact::unknown(UnknownWhy::Unparseable)
        }
    }
}

/// What the log says about the node at `path`, and the volume id its row is under.
///
/// The local engine journals every path the OS can see under the local disk's id,
/// whichever volume the pane showed it on, and the volume engine under the
/// destination volume's own, so an item not on the local disk is asked about
/// under both.
fn read(
    history: &History,
    path: &Path,
    node: NodeIdentity,
) -> Result<Option<(String, LandedAt)>, OperationLogStoreError> {
    let conn = open_read_connection(&history.db)?;
    let mut volume_ids = vec![DEFAULT_VOLUME_ID];
    if history.volume_id != DEFAULT_VOLUME_ID {
        volume_ids.push(&history.volume_id);
    }
    let mut answers = Vec::with_capacity(volume_ids.len());
    for volume_id in volume_ids {
        answers.push((volume_id.to_string(), landed_at(&conn, volume_id, path, node)?));
    }
    Ok(strongest(answers))
}

/// The answer that says most: the newest operation that landed the node, else
/// that the node was replaced since, else nothing.
fn strongest(answers: Vec<(String, LandedAt)>) -> Option<(String, LandedAt)> {
    let when = |landed: &LandedAt| match landed {
        LandedAt::ByOperation { at, .. } => Some(*at),
        LandedAt::NodeChangedSince | LandedAt::NoRecord => None,
    };
    let newest_operation = answers
        .iter()
        .filter(|(_, landed)| when(landed).is_some())
        .max_by_key(|(_, landed)| when(landed))
        .cloned();
    newest_operation.or_else(|| {
        answers
            .into_iter()
            .find(|(_, landed)| *landed == LandedAt::NodeChangedSince)
    })
}

/// The card's fact for what the log answered. `landed_on` is the volume id the
/// row was found under.
fn fact_of(landed: LandedAt, landed_on: &str, volume_name: &dyn Fn(&str) -> Option<String>) -> Fact<CmdrOperation> {
    let (kind, initiator, at, source_volume_id, source_path) = match landed {
        LandedAt::ByOperation {
            kind,
            initiator,
            at,
            source_volume_id,
            source_path,
        } => (kind, initiator, at, source_volume_id, source_path),
        LandedAt::NodeChangedSince => return Fact::unknown(UnknownWhy::NodeChangedSince),
        LandedAt::NoRecord => return Fact::unknown(UnknownWhy::NoRecord),
    };
    let Some(kind) = landed_by(kind, &source_path) else {
        // A kind that lands nothing recorded a node: nothing writes that. Saying
        // nothing beats wording an operation the card has no word for.
        return Fact::unknown(UnknownWhy::NoRecord);
    };
    let made_new = matches!(kind, LandedBy::Create | LandedBy::Compress);
    Fact::Known(CmdrOperation {
        kind,
        at,
        initiator: started_by(initiator),
        source_path: (!made_new).then(|| ItemText::new(&source_path)),
        source_volume: (!made_new && source_volume_id != landed_on)
            .then(|| volume_name(&source_volume_id))
            .flatten()
            .map(|name| ItemText::new(&name)),
    })
}

/// How an operation of `kind` landed an item, or `None` for a kind that lands
/// nothing.
///
/// An extract is journaled as a copy whose source sits inside an archive, so it's
/// told by the source path's shape: a component named like an archive, with
/// something after it. A plain folder named `photos.zip` reads as an archive here
/// too; the row still shows the true source path.
fn landed_by(kind: OpKind, source_path: &str) -> Option<LandedBy> {
    Some(match kind {
        OpKind::Copy if comes_out_of_an_archive(source_path) => LandedBy::Extract,
        OpKind::Copy => LandedBy::Copy,
        OpKind::Move => LandedBy::Move,
        OpKind::Trash => LandedBy::Trash,
        OpKind::Rename => LandedBy::Rename,
        OpKind::CreateFolder | OpKind::CreateFile => LandedBy::Create,
        OpKind::ArchiveEdit => LandedBy::Compress,
        OpKind::Delete | OpKind::Tag => return None,
    })
}

fn comes_out_of_an_archive(source_path: &str) -> bool {
    cmdr_fs::archive_format::archive_boundary_candidate(Path::new(source_path))
        .is_some_and(|(_archive, inner)| !inner.as_os_str().is_empty())
}

fn started_by(initiator: Initiator) -> StartedBy {
    match initiator {
        Initiator::User => StartedBy::You,
        // A plan the person edited before approving is still Ask Cmdr's.
        Initiator::Agent | Initiator::AgentEdited => StartedBy::AskCmdr,
        Initiator::AiClient => StartedBy::AiClient,
    }
}

#[cfg(test)]
mod tests;
