//! Filing onto a server or a phone: a copy or move that names each item
//! (`TransferLeaves::per_source`, `crates/cmdr-operations/src/transfer/DETAILS.md`
//! § Named destinations), started through the routed start the approval bridge
//! uses, then undone through the operation log. Written once against
//! `dyn Volume`; the SMB and MTP cells hand in a live volume.
//!
//! Each fixture keeps a bystander under a source's ORIGINAL name in the
//! destination, so a landing or an undo that read the original name instead of
//! the chosen one would touch the wrong file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::{StreamLength, Volume, WriteMode};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::rollback::{execute_rollback, rollback_operation};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::{CollectorEventSink, Reversal, TestOperationLog};
use cmdr_operations::{OperationEventSink, Operations, TransferLeaves, VolumeCopyConfig};

use super::network_transfer_test_support::{budget, clean_deep, read_all};
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::TestDir;
use cmdr_fs::ignore_poison::IgnorePoison;

const SETTLE_BUDGET: std::time::Duration = std::time::Duration::from_secs(6);

/// Copy or move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    Copy,
    Move,
}

/// An engine journaling into a log of its own, and both ends registered with
/// the app's volume manager, which the routed start and the undo both resolve
/// ids through.
struct Rig {
    engine: Operations,
    log: TestOperationLog,
    label: String,
}

impl Rig {
    fn new(label: &str) -> Self {
        let log = TestOperationLog::new(label);
        Self {
            engine: crate::operations_host::test_engine_journaling_into(&log),
            log,
            label: label.to_string(),
        }
    }

    fn register(&self, end: &str, volume: &Arc<dyn Volume>) -> String {
        let id = format!("{}-{end}", self.label);
        get_volume_manager().register(&id, Arc::clone(volume));
        id
    }

    /// Starts `verb` from `source_id` to `dest_id`, waits it out, and insists it
    /// reported nothing and finished as ONE operation.
    async fn run(
        &self,
        verb: Verb,
        (source_id, dest_id): (&str, &str),
        sources: Vec<PathBuf>,
        dest: &Path,
        leaves: TransferLeaves,
    ) -> String {
        let collector = Arc::new(CollectorEventSink::new());
        let events: Arc<dyn OperationEventSink> = collector.clone();
        let config = VolumeCopyConfig {
            leaves,
            ..VolumeCopyConfig::default()
        };
        let (from, to, dest) = (source_id.to_string(), dest_id.to_string(), dest.display().to_string());
        let started = match verb {
            Verb::Copy => {
                self.engine
                    .start_volume_copy(events, from, sources, to, dest, config, Initiator::User, None)
                    .await
            }
            Verb::Move => {
                self.engine
                    .start_volume_move(events, from, sources, to, dest, config, Initiator::User, None)
                    .await
            }
        };
        let started = started.unwrap_or_else(|e| panic!("{}: the named {verb:?} must start: {e:?}", self.label));
        crate::test_support::wait_until_async(budget(SETTLE_BUDGET), "the transfer to settle", || {
            !collector.settled.lock_ignore_poison().is_empty()
        })
        .await;
        let errors = collector.errors.lock_ignore_poison().clone();
        assert!(errors.is_empty(), "{}: the {verb:?} reported {errors:?}", self.label);
        assert_eq!(
            collector.complete.lock_ignore_poison().len(),
            1,
            "{}: one operation",
            self.label
        );
        started.operation_id
    }

    /// Undoes `op_id` the way the app does: the gate decides, then the engine runs.
    async fn undo(&self, op_id: &str) {
        let writer = self.log.writer();
        writer.flush_blocking().expect("the journal flushes");
        let vm = get_volume_manager();
        let plan = rollback_operation(vm, writer, op_id, |_plan| Ok(()))
            .unwrap_or_else(|refusal| panic!("{}: the named transfer must reverse: {refusal:?}", self.label));
        let reversal = Reversal::on(&self.engine, &self.label);
        let report = execute_rollback(
            vm,
            writer,
            &plan.original,
            &plan.inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await;
        assert_eq!(report.skipped, 0, "{}: nothing skipped, got {report:?}", self.label);
    }

    fn unregister(&self, ids: &[&str]) {
        for id in ids {
            get_volume_manager().unregister(id);
        }
    }
}

async fn names_on(volume: &dyn Volume, dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = volume
        .list_directory(dir, None)
        .await
        .unwrap_or_else(|e| panic!("list {}: {e:?}", dir.display()))
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

/// Waits for `dir` to list exactly `expected`, for a listing right after an undo.
///
/// A delete that returned can stay listed for a moment on a server: another
/// client's open handle on the folder leaves it delete-pending until that handle
/// closes, and the fixture's share is every suite's (`crates/cmdr-smb/DETAILS.md`
/// § "Delete safety"). A folder the undo really left behind outlasts the budget.
async fn names_settle_to(volume: &dyn Volume, dir: &Path, expected: &[&str], label: &str) {
    let deadline = tokio::time::Instant::now() + budget(SETTLE_BUDGET);
    loop {
        let names = names_on(volume, dir).await;
        if names == expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{label}: {} still lists {names:?}, expected {expected:?}",
            dir.display()
        );
        // allowed-test-sleep: `wait_until_async` takes no async condition, and this one is a listing
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// `dir` as the transfer dialog addresses it: under the volume's root. The SMB
/// fixture hands out a share-relative scratch folder, and a same-share move
/// journaled from relative paths doesn't undo (a fixture shape the app never
/// sends, so the scenario takes the app's).
fn anchored(remote: &dyn Volume, dir: PathBuf) -> PathBuf {
    if dir.is_relative() {
        remote.root().join(dir)
    } else {
        dir
    }
}

/// Writes `bytes` at `path` on the remote through a stream, the one write every
/// backend takes (a phone has no `create_file`).
async fn put(volume: &dyn Volume, path: &Path, bytes: &[u8]) {
    volume
        .create_directory_all(path.parent().expect("a parent"))
        .await
        .expect("the fixture's folder");
    let staged = TestDir::new("named_leaves_put");
    std::fs::write(staged.join("bytes"), bytes).expect("the fixture's bytes");
    let local = LocalPosixVolume::local_folder("Staged", &*staged, crate::volume_host::host());
    let stream = local
        .open_read_stream(Path::new("bytes"))
        .await
        .expect("the fixture's stream");
    volume
        .write_from_stream(
            path,
            WriteMode::CreateNew,
            StreamLength::Known(bytes.len() as u64),
            stream,
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("a fixture file at {}: {e:?}", path.display()));
}

/// ❗ Filing from Downloads onto the server: an invoice and a folder of scans
/// each land under a chosen name beside an item keeping its own, as one
/// operation, and one undo takes it all back. A copy's undo deletes exactly its
/// copies; a move's puts every item home under its ORIGINAL name.
pub(super) async fn local_items_land_named_on_the_server_and_undo_restores_them(
    remote: Arc<dyn Volume>,
    dir: PathBuf,
    verb: Verb,
) {
    let label = format!("named-leaves-up-{verb:?}-{}", std::process::id());
    let rig = Rig::new(&label);
    let dir = anchored(remote.as_ref(), dir);
    let local_dir = TestDir::new("named_leaves_up");
    std::fs::create_dir_all(local_dir.join("downloads/scans")).expect("the local tree");
    std::fs::write(local_dir.join("downloads/Invoice-X.pdf"), b"invoice").expect("invoice");
    std::fs::write(local_dir.join("downloads/receipt.pdf"), b"receipt").expect("receipt");
    std::fs::write(local_dir.join("downloads/scans/page.png"), b"page").expect("page");
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*local_dir,
        crate::volume_host::host(),
    ));
    let dest = dir.join("invoices paid");
    put(remote.as_ref(), &dest.join("Invoice-X.pdf"), b"bystander").await;
    let ids = (rig.register("local", &local), rig.register("remote", &remote));

    let invoice = PathBuf::from("downloads/Invoice-X.pdf");
    let scans = PathBuf::from("downloads/scans");
    let leaves = TransferLeaves::per_source([
        (invoice.clone(), "2026-09-27 OpenAI invoice Rymd.pdf".to_string()),
        (scans.clone(), "2026-09 scans".to_string()),
    ]);
    let sources = vec![invoice, PathBuf::from("downloads/receipt.pdf"), scans];
    let op_id = rig.run(verb, (&ids.0, &ids.1), sources, &dest, leaves).await;

    assert_eq!(
        names_on(remote.as_ref(), &dest).await,
        vec![
            "2026-09 scans",
            "2026-09-27 OpenAI invoice Rymd.pdf",
            "Invoice-X.pdf",
            "receipt.pdf"
        ],
        "{label}: each item under the name it was given, beside the bystander"
    );
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("2026-09-27 OpenAI invoice Rymd.pdf")).await,
        b"invoice"
    );
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("2026-09 scans/page.png")).await,
        b"page"
    );
    assert_eq!(
        local_dir.join("downloads/Invoice-X.pdf").exists(),
        verb == Verb::Copy,
        "{label}: a move takes the original, a copy keeps it"
    );

    rig.undo(&op_id).await;

    names_settle_to(remote.as_ref(), &dest, &["Invoice-X.pdf"], &label).await;
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("Invoice-X.pdf")).await,
        b"bystander"
    );
    assert_eq!(
        std::fs::read(local_dir.join("downloads/Invoice-X.pdf")).expect("the original"),
        b"invoice"
    );
    assert_eq!(
        std::fs::read(local_dir.join("downloads/scans/page.png")).expect("the folder's original name"),
        b"page"
    );
    assert_eq!(
        std::fs::read(local_dir.join("downloads/receipt.pdf")).expect("kept"),
        b"receipt"
    );

    rig.unregister(&[&ids.0, &ids.1]);
    clean_deep(remote.as_ref(), &dir).await;
}

/// ❗ A move within the server that names each item lands them named in one
/// operation (on a phone, MTP moves an object and then renames it, so a
/// bystander under the ORIGINAL name in the destination is exactly what could
/// get renamed instead), and one undo puts every original back.
pub(super) async fn items_move_named_within_the_server_and_undo_restores_them(remote: Arc<dyn Volume>, dir: PathBuf) {
    let label = format!("named-leaves-within-{}", std::process::id());
    let rig = Rig::new(&label);
    let dir = anchored(remote.as_ref(), dir);
    let (src, dest) = (dir.join("inbox"), dir.join("filed"));
    put(remote.as_ref(), &src.join("Invoice-X.pdf"), b"invoice").await;
    put(remote.as_ref(), &src.join("scans/page.png"), b"page").await;
    put(remote.as_ref(), &dest.join("Invoice-X.pdf"), b"bystander").await;
    let id = rig.register("remote", &remote);

    let leaves = TransferLeaves::per_source([
        (src.join("Invoice-X.pdf"), "2026-09-27 invoice.pdf".to_string()),
        (src.join("scans"), "2026-09 scans".to_string()),
    ]);
    let sources = vec![src.join("Invoice-X.pdf"), src.join("scans")];
    let op_id = rig.run(Verb::Move, (&id, &id), sources, &dest, leaves).await;

    assert_eq!(
        names_on(remote.as_ref(), &dest).await,
        vec!["2026-09 scans", "2026-09-27 invoice.pdf", "Invoice-X.pdf"],
        "{label}"
    );
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("2026-09-27 invoice.pdf")).await,
        b"invoice"
    );
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("Invoice-X.pdf")).await,
        b"bystander",
        "{label}: the bystander is untouched"
    );
    assert!(names_on(remote.as_ref(), &src).await.is_empty(), "{label}: both moved");

    rig.undo(&op_id).await;

    names_settle_to(remote.as_ref(), &dest, &["Invoice-X.pdf"], &label).await;
    assert_eq!(
        read_all(remote.as_ref(), &dest.join("Invoice-X.pdf")).await,
        b"bystander"
    );
    assert_eq!(
        names_on(remote.as_ref(), &src).await,
        vec!["Invoice-X.pdf", "scans"],
        "{label}"
    );
    assert_eq!(read_all(remote.as_ref(), &src.join("Invoice-X.pdf")).await, b"invoice");
    assert_eq!(read_all(remote.as_ref(), &src.join("scans/page.png")).await, b"page");

    rig.unregister(&[&id]);
    clean_deep(remote.as_ref(), &dir).await;
}
