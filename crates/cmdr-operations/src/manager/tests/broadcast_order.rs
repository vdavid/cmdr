//! The order `operations-changed` reaches the host in, when two broadcasts race.

use super::*;
use crate::host::testing::FakeVolumes;
use crate::host::{EngineEvents, InstantMutationFact, OperationErrorReport};
use crate::types::{LeftoversKeptEvent, OperationsChanged, WriteProgressEvent};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::Initiator;
use std::sync::{Mutex, mpsc};

/// Records the ids each `operations-changed` lists, but holds the first one
/// inside the call until the test releases it, so a second broadcast can race it.
struct GatedQueueEvents {
    snapshots: Mutex<Vec<Vec<String>>>,
    first_entered: Mutex<Option<mpsc::Sender<()>>>,
    first_release: Mutex<Option<mpsc::Receiver<()>>>,
}

impl EngineEvents for GatedQueueEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        let entered = self.first_entered.lock_ignore_poison().take();
        if let Some(entered) = entered {
            let release = self
                .first_release
                .lock_ignore_poison()
                .take()
                .expect("set with the sender");
            let _ = entered.send(());
            let _ = release.recv();
        }
        let ids = snapshot.operations.into_iter().map(|row| row.operation_id).collect();
        self.snapshots.lock_ignore_poison().push(ids);
    }
    fn volumes_busy_changed(&self, _volume_ids: Vec<String>) {}
    fn transfer_started(&self, _volume_ids: &[String]) {}
    fn transfer_finished(&self, _volume_ids: &[String]) {}
    fn leftovers_kept(&self, _notice: LeftoversKeptEvent) {}
    fn instant_mutation_settled(&self, _fact: InstantMutationFact) {}
    fn rollback_settled(&self, _initiator: Initiator, _narrowed: bool, _report: &RollbackReport) {}
    fn error_report(&self, _report: OperationErrorReport) {}
    fn scan_wait_progress(&self, _event: WriteProgressEvent) {}
}

/// The last queue snapshot the host hears is the current one, however two
/// broadcasts race. Here an operation's registration broadcast (which lists it)
/// is held mid-call while the operation runs and settles: if the settle's
/// broadcast could go around it, the registration's older snapshot would land
/// last, and every queue view would show a finished operation as running until
/// some other operation changed the queue. An undo's reversal of one small file
/// is quick enough to do exactly that on a busy machine.
#[test]
fn a_settle_racing_its_own_registration_reports_the_empty_queue_last() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let events = Arc::new(GatedQueueEvents {
        snapshots: Mutex::new(Vec::new()),
        first_entered: Mutex::new(Some(entered_tx)),
        first_release: Mutex::new(Some(release_rx)),
    });
    let engine = crate::Operations::builder(
        Arc::new(FakeVolumes::default()),
        Arc::clone(&events) as Arc<dyn EngineEvents>,
    )
    .build();
    let op = OperationId::from("quick-op");
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let (settled_tx, settled_rx) = mpsc::channel();
    let start: DeferredStart = {
        let op = op.clone();
        Box::new(move |ops: crate::Operations| {
            Box::pin(async move {
                let guard = ManagedTaskGuard::new(ops.manager(), op.clone());
                guard.disarm();
                ops.manager().on_settled(&op);
                let _ = settled_tx.send(());
            })
        })
    };

    let registering = {
        let engine = engine.clone();
        let descriptor = descriptor(&op, vec!["lane"]);
        std::thread::spawn(move || engine.manager().spawn_managed(descriptor, state, start))
    };
    entered_rx.recv().expect("the registration reaches its broadcast");

    // A racy manager lets the settle's broadcast finish while the registration's
    // is held, so wait for that; an ordered one keeps it waiting behind the held
    // broadcast, and the timeout only moves the release along. Neither way can
    // it turn a correct manager red.
    let settled_early = settled_rx.recv_timeout(Duration::from_millis(500)).is_ok();
    release_tx.send(()).expect("the registration's broadcast is waiting");
    registering.join().expect("registration");
    if !settled_early {
        settled_rx.recv_timeout(WAIT).expect("the operation settles");
    }

    let snapshots = events.snapshots.lock_ignore_poison();
    assert_eq!(
        snapshots.last(),
        Some(&Vec::new()),
        "the queue reported last must be the current, empty one, got {snapshots:?}"
    );
}
