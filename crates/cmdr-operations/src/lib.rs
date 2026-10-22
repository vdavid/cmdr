// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These three can't go with them: `unused_crate_dependencies` is judged per
// compilation unit, and `missing_docs` and `unreachable_pub` are this crate's own
// contract, as in every crate whose surface is capped: the re-exports below are
// the API, so a `pub` deep in a private module is a promise nobody can keep.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]
// The internals `testing` re-exports for a host's test cells are `pub` so it can;
// without the feature nothing reaches them, so only a `testing` build (every lint
// lane's, through the dev-dependencies) can say which `pub` is truly unreachable.
#![cfg_attr(not(feature = "testing"), allow(unreachable_pub))]
// `clippy.toml` refuses an ambient spawn and a global `sync(2)` in the engine; a
// test drives the engine from threads and tasks of its own, as a host does.
#![cfg_attr(
    test,
    allow(
        clippy::disallowed_methods,
        reason = "tests drive the engine from their own threads and tasks, as a host would"
    )
)]

//! The write engine: copy, move, delete, trash, rename, create, and zip edits as
//! managed operations with streaming progress, behind one [`Operations`] handle
//! the host builds once. The re-exports below ARE the API; every module but
//! `host` (and the `testing`-gated fixtures) is private.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.
//!
//! Safety features:
//! - Path canonicalization to prevent ".." and symlink bypass of recursion checks
//! - Destination writability check before starting
//! - Pre-flight disk space validation after scan
//! - Inode identity check to prevent copy-over-self via symlinks/hard links
//! - Path/name length validation (255-byte name, 1024-byte path)
//! - Special file filtering (skips sockets, FIFOs, devices)
//! - macOS copyfile(3) for full metadata preservation (xattrs, ACLs, resource forks)
//! - Symlink preservation (not dereferenced)
//! - Symlink loop detection to prevent infinite recursion
//! - Atomic cross-filesystem moves using staging directory

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_operations` an extern
// crate of its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_operations as _;
//noinspection RsUnusedImport
// Used by `benches/`, a separate compilation unit the lint can't see from here.
#[cfg(test)]
use criterion as _;

mod archive_edit;
mod cancellable;
mod compress_estimate;
mod conflict;
mod conflict_policy_cell;
mod conflict_preflight;
mod conflict_slot;
mod create;
mod delete;
mod durability;
mod error_classification;
mod eta;
mod event_sinks;
mod free_space;
mod handle;
// Two engines share nothing.
#[cfg(test)]
mod handle_tests;
mod hidden_links;
pub mod host;
mod human_wait;
mod in_flight_temps;
mod journal;
mod journal_search;
// The local-filesystem copy, move, delete, and trash starters.
mod ledger;
mod local_start;
mod look_alike;
#[cfg(test)]
mod look_alike_instant_tests;
mod manager;
mod metadata_losses;
mod mount_boundary;
mod mutation_error;
mod mutation_reply;
mod operation_intent;
mod outage_slot;
mod overwrite;
// The engine half of "paste clipboard content as a file"; its only caller is macOS.
#[cfg(target_os = "macos")]
mod create_numbered;
mod locked_sources;
mod locked_sources_slot;
mod queued_sources;
mod rename;
mod reversal;
mod rollback;
mod routed_writes;
#[cfg(test)]
mod routed_writes_tests;
mod routing;
mod scan;
mod scan_bridge;
mod scan_cache;
mod scan_dry_run;
mod scan_preview;
mod scan_source_tracker;
mod scan_walker;
mod scan_watchdog;
mod scope;
mod scratch_dir;
mod source_binding;
mod sources_gone_slot;
mod state;
mod status_cache;
mod summary;
mod target_names;
mod transfer;
mod transfer_sides;
mod transfer_tree;
mod tree_view;
mod types;
mod unblocked_removal;
mod unique_name;
mod validation;

use std::sync::Arc;

// ============================================================================
// The public API: what code outside the engine may name. Every submodule but
// `host` (the seams the host answers) is private, so the compiler keeps
// callers on this list; behavior is `Operations` methods, these are the types
// they take and return, plus the few engine-free helpers.
// ============================================================================

// The handle and its builder.
pub use handle::OperationsBuilder;
/// Why a second `Operations::claim_ledger` refused.
pub use in_flight_temps::AlreadyClaimed;

// The two event sink seams. The app builds its Tauri-backed answers
// (`operations_host::sinks`) at the IPC edge and hands them to a start; the
// engine itself never constructs a sink. `write-conflict` carries an
// `ArmedConflict`.
pub use conflict_slot::ArmedConflict;
pub use event_sinks::{OperationEventSink, ScanPreviewEventSink};

// The queue: `operations-changed`'s payload and what the queue controls answer.
pub use manager::{
    OperationSnapshot, OperationsChanged, PauseAllOutcome, PauseOutcome, QueueMoveDirection, QueueMoveOutcome,
};

// The live transfer tree's one IPC answer: a window of rows for the dialog and
// the queue (`transfer_tree/CLAUDE.md`), whose rows `cmdr://state` reads too.
pub use transfer_tree::{TransferTreeRow, TransferTreeView, TransferTreeViewRequest, TreeRowKind, TreeRowState};
pub use tree_view::TransferTreeViewError;

// Instant mutations: the typed refusal, the reply-within-a-deadline contract
// (`mutation_reply.rs`), and rename's batch and validity vocabulary.
pub use mutation_error::MutationError;
pub use mutation_reply::{MUTATION_REPLY_DEADLINE, Replied, reply_or_hand_off, reply_within};
pub use mutation_reply::{MutationReply, MutationSettled};
pub use rename::{BulkRenameRow, RenameStartError, RenameValidityResult, RenamesStarted};

// Pre-flight: the transfer dialog's conflict check, a preview's cost facts
// (the S3 estimate prices them), the source binding a reviewed batch supplies
// (`source_binding.rs`), and an undo's report.
pub use conflict_preflight::CONFLICT_CHECK_BUDGET;
pub use conflict_preflight::{SourceItemInput, VolumeScanError};
pub use rollback::UndoReport;
pub use scan_cache::ScanCostFacts;
pub use source_binding::{ExpectedSources, LocalContent, RemoteContent, SourceFingerprint};

// Engine-free helpers the IPC edge asks before or beside an operation: the F8
// routing question, where a path's trash is, the bare unjournaled trash move
// (for the app's own leftovers, which no undo should list), the look-alike name
// check, whether two paths are one local file, and anchoring a volume-relative
// destination (`routing.rs`).
pub use delete::cloud_trash::{TrashRoutingAnswer, routing_for_selection as trash_routing_for_selection};
pub use delete::trash::{move_to_trash_sync, trash_dir_for_path};
pub use look_alike::held_in_another_spelling;
pub use rename::same_local_file;
pub use routing::resolve_dest_path;

// The wire vocabulary: payloads, configs, and typed outcomes.
pub use types::{
    ConflictId, ConflictInfo, ConflictResolution, ConflictResolutionOutcome, DryRunResult, KnownSize,
    LeftoversKeptEvent, LifecycleStatus, LockedSourcesAnswer, LockedSourcesAnswerOutcome, OperationStatus,
    ReadOnlySide, RetryOutcome, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanPreviewPurpose, ScanPreviewRefusal, ScanPreviewStartResult, ScanPreviewTotals,
    ScanProgressEvent, SourceItemOutcome, SourcesGoneAnswerOutcome, TransferActivity, VolumeCopyConfig,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent,
    WriteLockedSourcesEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult,
    WriteOperationType, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};

// What the app-side test cells reach beyond the list above.
#[cfg(any(test, feature = "testing"))]
pub mod testing;

// ============================================================================
// Reach for the engine's own modules: siblings and the tests below name these
// at the root. Private, so nothing outside the engine can.
// ============================================================================

use summary::path_summary;
use transfer::volume::copy_volumes_with_progress;

// What `OperationSnapshot`'s field docs link to. Those docs reach `bindings.ts`
// verbatim, so the name comes to them rather than the other way round.
#[cfg(doc)]
use manager::OperationDescriptor;

#[cfg(any(test, feature = "testing"))]
use event_sinks::CollectorEventSink;
#[cfg(test)]
use state::OperationIntent;
#[cfg(test)]
use transfer::volume::{move_volumes_with_progress, move_within_same_volume_with_progress};
#[cfg(test)]
use validation::{
    ensure_destination_dir, is_same_file, is_same_filesystem, validate_destination_not_inside_source,
    validate_destination_writable, validate_path_length, validate_source_names_are_distinct, validate_sources,
};

// ============================================================================
// The engine handle
// ============================================================================

/// The write engine: a cheap-to-clone handle, every clone sharing one state.
///
/// The host builds one with [`Operations::builder`] and holds it; a test or a
/// tool builds its own. Two engines share nothing: an operation registered in
/// one is invisible to the other.
#[derive(Clone)]
pub struct Operations {
    inner: Arc<Inner>,
}

/// The engine's state: the seams it asks, and everything it keeps between two
/// calls. One per [`Operations`]; nothing here is process-wide.
///
/// Defined here at the module root, the parent of every module that works on a
/// part of it, so those modules hang off one hub instead of circling through a
/// sibling.
pub(crate) struct Inner {
    /// Everything the engine reaches the host through, shared with each
    /// operation's scope.
    host: Arc<host::EngineHost>,
    /// The journal every operation this engine runs is recorded in.
    operation_log: cmdr_operation_log::OperationLog,
    /// Admission, lanes, and the queue's records.
    manager: Arc<manager::OperationManager>,
    /// Every live operation's state, by id.
    registry: Arc<state::WriteOperationRegistry>,
    /// Every registered operation's status, and the busy-volume set.
    status: Arc<status_cache::StatusCache>,
    /// The stores each operation's scope shares.
    stores: Arc<EngineStores>,
}

/// What the engine keeps between two calls that its operations reach through
/// their scope: the scan previews, the in-flight ledger, the lanes' outage
/// records, and the transfer probes.
///
/// Here at the hub, beside [`Inner`], for the same reason `Inner` is: the
/// modules that own each store also work on operation states, so a scope
/// naming those modules would circle back through `state`. The scope names
/// this type instead.
pub(crate) struct EngineStores {
    previews: Arc<scan_cache::ScanPreviews>,
    /// What this engine has on a disk that isn't the user's file yet. In
    /// memory until the engine's one claim points it at a data dir.
    ledger: in_flight_temps::Ledger,
    /// Set by the claim, once ever (`Operations::claim_ledger`).
    ledger_claimed: std::sync::atomic::AtomicBool,
    /// One outage record per lane, shared by every transfer waiting on it.
    outages: Arc<transfer::volume_outage::Outages>,
    /// Every live transfer's in-flight table, for its stall watchdog and the
    /// activity each progress event carries.
    probes: Arc<transfer::transfer_probe::Probes>,
    /// The background passes finishing what stopped transfers couldn't clean
    /// up at once, and the quit's switch for them.
    leftover_cleanups: transfer::leftover_cleanup::LeftoverCleanups,
}

impl EngineStores {
    fn new() -> Self {
        Self {
            previews: Arc::new(scan_cache::ScanPreviews::new()),
            ledger: in_flight_temps::Ledger::new(),
            ledger_claimed: std::sync::atomic::AtomicBool::new(false),
            outages: Arc::new(transfer::volume_outage::Outages::new()),
            probes: Arc::new(transfer::transfer_probe::Probes::new()),
            leftover_cleanups: transfer::leftover_cleanup::LeftoverCleanups::new(),
        }
    }

    /// The background cleanup passes of stopped transfers.
    pub(crate) fn leftover_cleanups(&self) -> &transfer::leftover_cleanup::LeftoverCleanups {
        &self.leftover_cleanups
    }

    /// Every live transfer's probe, shared, since a registration's guard holds
    /// the table until it deregisters.
    pub(crate) fn probes(&self) -> &Arc<transfer::transfer_probe::Probes> {
        &self.probes
    }

    /// The lanes' outage records, shared, since a waiter's membership holds
    /// them until it leaves.
    pub(crate) fn outages(&self) -> &Arc<transfer::volume_outage::Outages> {
        &self.outages
    }

    /// The in-flight ledger every operation on this engine records into.
    pub(crate) fn ledger(&self) -> &in_flight_temps::Ledger {
        &self.ledger
    }

    /// Takes the ledger's one claim: `true` the first time, `false` ever after.
    pub(crate) fn take_ledger_claim(&self) -> bool {
        !self.ledger_claimed.swap(true, std::sync::atomic::Ordering::SeqCst)
    }

    /// Every scan preview this engine knows about, in flight or settled.
    pub(crate) fn previews(&self) -> &scan_cache::ScanPreviews {
        &self.previews
    }

    /// The same, shared, for a part of the engine that keeps its own handle on
    /// it (the manager).
    pub(crate) fn previews_arc(&self) -> &Arc<scan_cache::ScanPreviews> {
        &self.previews
    }
}

#[cfg(test)]
mod approved_op_parity_tests;
#[cfg(test)]
mod journal_capture_tests;
#[cfg(test)]
mod journal_capture_volume_tests;
// Every write path announces each own write before the syscall that makes it.
#[cfg(test)]
mod own_write_announce_tests;
#[cfg(test)]
mod scan_bridge_tests;
// A walk that stops on a missing or unreadable item fails its operation naming that item.
#[cfg(test)]
mod scan_failure_tests;
#[cfg(test)]
mod scan_pause_tests;
#[cfg(test)]
mod scan_preview_listing_progress_tests;
#[cfg(test)]
mod scan_preview_oracle_tests;
#[cfg(test)]
mod scan_preview_tests;
// A transfer's preview builds the operation's live tree, and only a transfer's.
#[cfg(test)]
mod scan_preview_tree_tests;
#[cfg(test)]
mod scan_watchdog_tests;
#[cfg(test)]
mod settle_event_tests;
// The one cooperative-stop boundary every serial loop here asks at.
#[cfg(test)]
mod free_space_tests;
#[cfg(test)]
mod stop_or_park_tests;
#[cfg(any(test, feature = "testing"))]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_integration_test;
