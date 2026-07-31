//! What every in-flight transfer task is doing right now, and a watchdog that
//! says so when a transfer stops moving.
//!
//! A wedged transfer parks its driver and its tasks on `.await`, so no thread
//! carries a transfer frame and a stack sample sees nothing (that is exactly how
//! the 2026-07-31 incident resisted diagnosis; see
//! `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). The only way to
//! learn where a parked async task is stuck is for it to say so on the way in, so
//! every phase transition records itself here and the watchdog prints the table.
//!
//! **Cost.** A phase transition is one relaxed atomic store, and per-chunk byte
//! progress is one more; nothing on the hot path takes a lock. The per-operation
//! registry lock is touched only when a task starts or finishes, and by the
//! watchdog tick.
//!
//! **Reaching the probe.** A copy task's body runs inside
//! [`CURRENT_TASK_PROBE`]`.scope(...)`, so code arbitrarily deep inside it
//! (`copy_single_path` → `stream_pipe_file` → `CheckpointStream`) reaches its own
//! probe with no signature threading. Outside a copy task the lookup simply
//! misses and every call is a no-op, which is what the unit tests and the
//! local-FS path rely on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

use super::super::state::{OperationIntent, WriteOperationState, load_intent};

/// How often the watchdog looks at an operation, and how long a transfer may
/// show zero byte movement before it is called out.
///
/// 20 s is chosen to sit well clear of a slow-but-alive SMB write window while
/// still landing inside the window a user spends wondering whether to force-quit.
pub(super) const STALL_TICK: Duration = Duration::from_secs(5);
pub(super) const STALL_AFTER: Duration = Duration::from_secs(20);

/// What a single copy task is doing. Ordinals are stable only within a build;
/// nothing persists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum TaskPhase {
    /// Spawned into the window, not yet doing I/O.
    Spawned = 0,
    /// Opening the source stream (a device round-trip on MTP / SMB).
    OpeningSource = 1,
    /// Actively piping chunks.
    Streaming = 2,
    /// Parked between windows because the user paused.
    ParkedPause = 3,
    /// Parked between windows for foreground work on the SOURCE device
    /// (unbounded by design).
    ParkedSourceYield = 4,
    /// Parked between windows for foreground work on the DESTINATION share
    /// (hard-capped; it holds an open write handle).
    ParkedDestYield = 5,
    /// Past the last byte: safe-replace finalize, journal, cleanup.
    Finalizing = 6,
    /// Resolving a nested conflict inside a directory source (may be waiting on
    /// the human).
    ResolvingConflict = 7,
}

impl TaskPhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::OpeningSource => "opening-source",
            Self::Streaming => "streaming",
            Self::ParkedPause => "parked(pause)",
            Self::ParkedSourceYield => "parked(source-yield)",
            Self::ParkedDestYield => "parked(dest-yield)",
            Self::Finalizing => "finalizing",
            Self::ResolvingConflict => "resolving-conflict",
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::OpeningSource,
            2 => Self::Streaming,
            3 => Self::ParkedPause,
            4 => Self::ParkedSourceYield,
            5 => Self::ParkedDestYield,
            6 => Self::Finalizing,
            7 => Self::ResolvingConflict,
            _ => Self::Spawned,
        }
    }
}

/// What the DRIVER (the loop that fills and drains the concurrency window) is
/// doing. Distinguishing this from the tasks is the point: in the incident the
/// driver stopped after a destination `get_metadata` pre-check with six of eight
/// slots free, and nothing recorded that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum DriverPhase {
    Starting = 0,
    /// Running the destination pre-check / conflict resolution for the next
    /// source, before it can be spawned.
    PreparingNext = 1,
    /// Window full or sources exhausted: awaiting the next task to finish.
    AwaitingTasks = 2,
    /// Loop finished; running cleanup, rollback, or finalize.
    PostLoop = 3,
}

impl DriverPhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::PreparingNext => "preparing-next",
            Self::AwaitingTasks => "awaiting-tasks",
            Self::PostLoop => "post-loop",
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::PreparingNext,
            2 => Self::AwaitingTasks,
            3 => Self::PostLoop,
            _ => Self::Starting,
        }
    }
}

/// One in-flight copy task's live state.
pub(super) struct TaskProbe {
    /// Position of this source in the operation's source list, so a dump can be
    /// read against the spawn log.
    index: usize,
    source: String,
    dest: String,
    phase: AtomicU8,
    /// Millis since the operation started, at the last phase transition.
    phase_since_ms: AtomicU64,
    bytes_done: AtomicU64,
    total_bytes: AtomicU64,
    started: Instant,
}

impl TaskProbe {
    /// Record a phase transition. One relaxed store plus a timestamp; safe to
    /// call on any path, including per-chunk.
    pub(super) fn set_phase(&self, phase: TaskPhase) {
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.phase.store(phase as u8, Ordering::Relaxed);
        self.phase_since_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Per-chunk byte progress. Deliberately a plain store, not an add: the
    /// caller owns the running total for its own file.
    pub(super) fn set_bytes(&self, bytes_done: u64, total_bytes: u64) {
        self.bytes_done.store(bytes_done, Ordering::Relaxed);
        self.total_bytes.store(total_bytes, Ordering::Relaxed);
    }

    fn render(&self) -> String {
        let phase = TaskPhase::from_u8(self.phase.load(Ordering::Relaxed));
        let since_ms = self.phase_since_ms.load(Ordering::Relaxed);
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let total = self.total_bytes.load(Ordering::Relaxed);
        format!(
            "#{idx} {phase} for {held}ms, {done}/{total} bytes, {source} -> {dest}",
            idx = self.index,
            phase = phase.label(),
            held = now_ms.saturating_sub(since_ms),
            done = self.bytes_done.load(Ordering::Relaxed),
            total = total,
            source = self.source,
            dest = self.dest,
        )
    }
}

/// One operation's live state: the driver plus every task currently in the
/// concurrency window.
pub(super) struct OperationProbe {
    operation_id: String,
    concurrency: usize,
    total_files: usize,
    driver_phase: AtomicU8,
    /// Free-form detail for the driver's current step (the path it is
    /// pre-checking, typically). Written only at phase transitions.
    driver_detail: Mutex<String>,
    tasks: Mutex<Vec<Arc<TaskProbe>>>,
    /// The operation's aggregate byte counter, shared with the driver, so the
    /// watchdog measures the same number the user sees.
    bytes_done: Arc<AtomicU64>,
    state: Arc<WriteOperationState>,
    started: Instant,
}

impl OperationProbe {
    pub(super) fn set_driver_phase(&self, phase: DriverPhase, detail: &str) {
        self.driver_phase.store(phase as u8, Ordering::Relaxed);
        detail.clone_into(&mut self.driver_detail.lock_ignore_poison());
    }

    /// Register a task entering the window. The returned handle removes it on
    /// drop, so a task that panics or is aborted still leaves the table clean.
    pub(super) fn begin_task(self: &Arc<Self>, index: usize, source: &str, dest: &str) -> TaskProbeHandle {
        let probe = Arc::new(TaskProbe {
            index,
            source: source.to_owned(),
            dest: dest.to_owned(),
            phase: AtomicU8::new(TaskPhase::Spawned as u8),
            phase_since_ms: AtomicU64::new(u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            bytes_done: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            started: self.started,
        });
        self.tasks.lock_ignore_poison().push(Arc::clone(&probe));
        TaskProbeHandle {
            operation: Arc::clone(self),
            probe,
        }
    }

    /// The whole in-flight table as log lines. This is the record the incident
    /// needed and did not have.
    fn render_dump(&self, reason: &str) -> String {
        let tasks = self.tasks.lock_ignore_poison();
        let driver = DriverPhase::from_u8(self.driver_phase.load(Ordering::Relaxed));
        let intent = match load_intent(&self.state.intent) {
            OperationIntent::Running => "running",
            OperationIntent::RollingBack => "rolling-back",
            OperationIntent::Stopped => "stopped",
        };
        let mut out = format!(
            "transfer probe ({reason}): op={op} elapsed={elapsed}s bytes_done={bytes} files_total={files} \
             driver={driver}({detail}) intent={intent} paused={paused} in_flight={in_flight}/{concurrency}",
            op = self.operation_id,
            elapsed = self.started.elapsed().as_secs(),
            bytes = self.bytes_done.load(Ordering::Relaxed),
            files = self.total_files,
            driver = driver.label(),
            detail = self.driver_detail.lock_ignore_poison(),
            paused = self.state.pause_gate.is_paused(),
            in_flight = tasks.len(),
            concurrency = self.concurrency,
        );
        if tasks.is_empty() {
            out.push_str("\n  (no tasks in flight)");
        }
        for task in tasks.iter() {
            out.push_str("\n  ");
            out.push_str(&task.render());
        }
        out
    }
}

/// RAII registration for one task. Dropping it removes the task from the table,
/// including on panic or abort.
pub(super) struct TaskProbeHandle {
    operation: Arc<OperationProbe>,
    probe: Arc<TaskProbe>,
}

impl TaskProbeHandle {
    pub(super) fn probe(&self) -> Arc<TaskProbe> {
        Arc::clone(&self.probe)
    }
}

impl Drop for TaskProbeHandle {
    fn drop(&mut self) {
        self.operation
            .tasks
            .lock_ignore_poison()
            .retain(|t| !Arc::ptr_eq(t, &self.probe));
    }
}

tokio::task_local! {
    /// The probe for the copy task currently being polled. Set by
    /// `volume_copy`'s task body; read by anything nested inside it.
    pub(super) static CURRENT_TASK_PROBE: Arc<TaskProbe>;
}

/// Set the current copy task's phase, if there is one.
///
/// A no-op outside a copy task (unit tests, the local-FS path), so callers never
/// need to know whether they are being driven by the volume copy driver.
pub(super) fn set_task_phase(phase: TaskPhase) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_phase(phase));
}

/// Report per-chunk byte progress for the current copy task, if there is one.
pub(super) fn set_task_bytes(bytes_done: u64, total_bytes: u64) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_bytes(bytes_done, total_bytes));
}

/// Live operations, so a watchdog tick (and any future debug command) can see
/// every transfer at once.
static REGISTRY: LazyLock<Mutex<HashMap<String, Arc<OperationProbe>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register an operation and start its stall watchdog.
///
/// The returned guard deregisters on drop, which also stops the watchdog on its
/// next tick.
pub(super) fn register_operation(
    operation_id: &str,
    concurrency: usize,
    total_files: usize,
    bytes_done: Arc<AtomicU64>,
    state: Arc<WriteOperationState>,
) -> OperationProbeGuard {
    let probe = Arc::new(OperationProbe {
        operation_id: operation_id.to_owned(),
        concurrency,
        total_files,
        driver_phase: AtomicU8::new(DriverPhase::Starting as u8),
        driver_detail: Mutex::new(String::new()),
        tasks: Mutex::new(Vec::new()),
        bytes_done,
        state,
        started: Instant::now(),
    });
    REGISTRY
        .lock_ignore_poison()
        .insert(operation_id.to_owned(), Arc::clone(&probe));
    spawn_watchdog(operation_id.to_owned());
    OperationProbeGuard {
        operation_id: operation_id.to_owned(),
        probe,
    }
}

/// Deregisters its operation on drop.
pub(super) struct OperationProbeGuard {
    operation_id: String,
    probe: Arc<OperationProbe>,
}

impl OperationProbeGuard {
    pub(super) fn probe(&self) -> Arc<OperationProbe> {
        Arc::clone(&self.probe)
    }
}

impl Drop for OperationProbeGuard {
    fn drop(&mut self) {
        REGISTRY.lock_ignore_poison().remove(&self.operation_id);
    }
}

/// Watches one operation's aggregate byte counter and logs the in-flight table
/// when it stops moving.
///
/// Deliberately quiet while paused: a paused transfer moves no bytes on purpose.
/// The dump repeats every `STALL_AFTER` for as long as the stall lasts, because
/// a user who force-quits after 20 minutes should leave behind more than one
/// record of it.
fn spawn_watchdog(operation_id: String) {
    tauri::async_runtime::spawn(async move {
        let mut last_bytes = u64::MAX;
        let mut still_since = Instant::now();
        let mut last_reported = Instant::now();
        loop {
            tokio::time::sleep(STALL_TICK).await;
            let Some(probe) = REGISTRY.lock_ignore_poison().get(&operation_id).cloned() else {
                return; // operation finished; guard removed it
            };
            if probe.state.pause_gate.is_paused() {
                still_since = Instant::now();
                continue;
            }
            let bytes = probe.bytes_done.load(Ordering::Relaxed);
            if bytes != last_bytes {
                last_bytes = bytes;
                still_since = Instant::now();
                continue;
            }
            if still_since.elapsed() < STALL_AFTER || last_reported.elapsed() < STALL_AFTER {
                continue;
            }
            last_reported = Instant::now();
            log::warn!(
                "{}",
                probe.render_dump(&format!("no byte movement for {}s", still_since.elapsed().as_secs()))
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::test_support::TestOperationGuard;

    fn probe_for(id: &str, state: &Arc<WriteOperationState>) -> Arc<OperationProbe> {
        Arc::new(OperationProbe {
            operation_id: id.to_owned(),
            concurrency: 8,
            total_files: 764,
            driver_phase: AtomicU8::new(DriverPhase::AwaitingTasks as u8),
            driver_detail: Mutex::new("sms-20260724020237.xml".to_owned()),
            tasks: Mutex::new(Vec::new()),
            bytes_done: Arc::new(AtomicU64::new(83_650_000)),
            state: Arc::clone(state),
            started: Instant::now(),
        })
    }

    /// The dump has to answer the questions the 2026-07-31 incident could not:
    /// what each task awaits, what the driver was doing, and what the intent was.
    #[test]
    fn dump_names_driver_phase_intent_and_every_parked_task() {
        let guard = TestOperationGuard::register("probe-dump");
        let state = guard.state();
        let probe = probe_for(guard.id(), state);

        let a = probe.begin_task(9, "/src/sms-0726.xml", "/dst/sms-0726.xml");
        a.probe().set_phase(TaskPhase::ParkedDestYield);
        a.probe().set_bytes(0, 13_421_021);
        let b = probe.begin_task(11, "/src/sms-0725.xml", "/dst/sms-0725.xml");
        b.probe().set_phase(TaskPhase::Streaming);
        b.probe().set_bytes(4_194_304, 13_421_021);

        let dump = probe.render_dump("test");

        assert!(dump.contains("driver=awaiting-tasks(sms-20260724020237.xml)"), "{dump}");
        assert!(dump.contains("intent=running"), "{dump}");
        assert!(dump.contains("in_flight=2/8"), "{dump}");
        assert!(dump.contains("#9 parked(dest-yield)"), "{dump}");
        assert!(dump.contains("0/13421021 bytes"), "{dump}");
        assert!(dump.contains("#11 streaming"), "{dump}");
        assert!(dump.contains("4194304/13421021 bytes"), "{dump}");
    }

    /// A task that is dropped mid-flight (abort, panic) must not linger in the
    /// table and make the next dump lie about what is in flight.
    #[test]
    fn dropping_a_task_handle_removes_it_from_the_table() {
        let guard = TestOperationGuard::register("probe-drop");
        let state = guard.state();
        let probe = probe_for(guard.id(), state);

        let a = probe.begin_task(0, "/src/a", "/dst/a");
        {
            let _b = probe.begin_task(1, "/src/b", "/dst/b");
            assert!(probe.render_dump("test").contains("in_flight=2/8"));
        }
        assert!(probe.render_dump("test").contains("in_flight=1/8"));
        drop(a);
        let dump = probe.render_dump("test");
        assert!(dump.contains("in_flight=0/8"), "{dump}");
        assert!(dump.contains("(no tasks in flight)"), "{dump}");
    }

    /// Outside a copy task the task-local is unset; the helpers must be silent
    /// no-ops rather than panicking.
    #[test]
    fn phase_helpers_are_noops_outside_a_copy_task() {
        set_task_phase(TaskPhase::Streaming);
        set_task_bytes(1, 2);
    }
}
