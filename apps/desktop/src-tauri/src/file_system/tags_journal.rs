//! A tag write's entry in the operation log, so the history can undo it.
//!
//! The writers in `tags_io` collect every file they changed into [`TagWrites`](crate::file_system::tags_journal::TagWrites) (the
//! raw `_kMDItemUserTags` value before and after), and the caller journals the batch
//! as ONE `tag` operation once it's done, with whoever started it. The undo itself is
//! the operation log's (`crates/cmdr-operation-log/DETAILS.md` § "A tag write restores
//! the exact prior value"). macOS only, like the writers.

use std::time::{SystemTime, UNIX_EPOCH};

use cmdr_fs::entry::TagsChange;
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::capture::FinalizeInputs;
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RowRole};
use cmdr_operation_log::writer::{JournalItem, OpenOperation};

use super::tags::encode_tags_plist;

/// One file a tag write changed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TagWrite {
    volume_id: String,
    path: String,
    change: TagsChange,
}

/// The files one user-level tag action changed, in the order it first wrote them.
///
/// A file written twice (a `toggle` of two colors makes one pass per color) is ONE
/// entry: the value before the first write and after the last. A file that ended where
/// it started changed nothing, so it isn't journaled.
// DEFAULT-OK: empty claims nothing: no file written yet.
#[derive(Debug, Default)]
pub struct TagWrites {
    files: Vec<TagWrite>,
}

/// The value a file with no tags attribute gets back on an undo: the empty array, which
/// reads as no tags everywhere. A restore can't REMOVE the attribute instead: a QNAP
/// answers success to that and keeps it (`file_system/DETAILS.md` § "Finder tags").
fn no_tags_value() -> Vec<u8> {
    encode_tags_plist(&[]).expect("an empty plist array always encodes")
}

impl TagWrites {
    /// Note that `path` on `volume_id` now holds `after`, having held `before` (`None`:
    /// no attribute at all).
    pub(crate) fn record(&mut self, volume_id: &str, path: &str, before: Option<&[u8]>, after: Vec<u8>) {
        if let Some(file) = self
            .files
            .iter_mut()
            .find(|file| file.path == path && file.volume_id == volume_id)
        {
            file.change.after = after;
            return;
        }
        let before = before.map_or_else(no_tags_value, <[u8]>::to_vec);
        self.files.push(TagWrite {
            volume_id: volume_id.to_string(),
            path: path.to_string(),
            change: TagsChange { before, after },
        });
    }

    /// The item rows, one per file whose value actually changed.
    fn rows(self) -> Vec<JournalItem> {
        self.files
            .into_iter()
            .filter(|file| file.change.before != file.change.after)
            .enumerate()
            .map(|(seq, file)| {
                let path = std::path::Path::new(&file.path);
                JournalItem {
                    seq: seq as i64,
                    entry_type: EntryType::File,
                    row_role: RowRole::RollbackUnit,
                    source_volume_id: file.volume_id,
                    source_dir: path
                        .parent()
                        .map(|dir| dir.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    source_name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    dest_volume_id: None,
                    dest_dir: None,
                    dest_name: None,
                    size: None,
                    mtime: None,
                    outcome: ItemOutcome::Done,
                    overwrote: false,
                    dest_identity: None,
                    tags: Some(file.change),
                }
            })
            .collect()
    }

    /// Journal the batch as one `tag` operation on `log`, started by `initiator`.
    /// `finished` is false when a write failed partway: the files before it still
    /// changed, so they're journaled all the same, under a `failed` operation.
    ///
    /// Answers the operation's id when it reached the log (nothing changed, recording
    /// is off, or the log never opened ⇒ `None`). Blocks on the log's writer, so an
    /// async caller goes through [`Self::journal_off_thread`].
    pub(crate) fn journal(self, log: &OperationLog, initiator: Initiator, finished: bool) -> Option<String> {
        let rows = self.rows();
        let first = rows.first()?;
        let op_id = cmdr_operation_log::new_operation_id();
        let count = rows.len() as u64;
        let journal = log.journal_open(OpenOperation {
            op_id: op_id.clone().into(),
            kind: OpKind::Tag,
            initiator,
            source_volume_id: Some(first.source_volume_id.clone()),
            dest_volume_id: None,
            item_count: count,
            started_at: now_secs(),
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        });
        journal.record_items(rows);
        let outcome = journal.finalize(FinalizeInputs {
            execution_status: if finished {
                ExecutionStatus::Done
            } else {
                ExecutionStatus::Failed
            },
            kind: OpKind::Tag,
            archive_subkind: None,
            net_new: false,
            ended_at: now_secs(),
            item_count: Some(count),
            items_done: count,
            bytes_total: 0,
            dev_summary: None,
        });
        (outcome.rollback_unit_rows > 0).then_some(op_id)
    }

    /// [`Self::journal`] on the app's log, off the async worker (the finalize waits on
    /// the writer thread's reply).
    pub(crate) async fn journal_off_thread(self, initiator: Initiator, finished: bool) -> Option<String> {
        if self.files.is_empty() {
            return None;
        }
        let app_log = crate::operation_log_host::operation_log();
        match tauri::async_runtime::spawn_blocking(move || self.journal(app_log, initiator, finished)).await {
            Ok(op_id) => op_id,
            Err(e) => {
                log::warn!(target: "tags", "couldn't journal a tag write: {e}");
                None
            }
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(test)]
#[path = "tags_journal_test.rs"]
mod tests;
