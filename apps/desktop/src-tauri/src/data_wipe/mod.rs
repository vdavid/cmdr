//! "Delete all Cmdr data" (Settings › Updates & privacy): removes everything Cmdr keeps on this
//! Mac (the data, cache, and log dirs, its `~/Library` state, every secret-store item), then the
//! frontend quits the app. The EU Cyber Resilience Act's Annex I Part I (2)(m) asks for it.
//!
//! - `plan.rs`: which paths, from this instance's own roots, with a guard against a bad one.
//! - `execute.rs`: secrets, then each target through a tombstone rename, then the leftovers.
//! - `summary.rs`: the sizes the dialog quotes.
//!
//! This file is the app side: resolving the roots, stopping what writes, and the exit sweep.
//! `CLAUDE.md` beside it has the must-knows.

mod execute;
mod plan;
mod summary;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Runtime};

pub use execute::WipeReport;
use plan::{WipeRoots, WipeTarget};

/// What the dialog shows before anyone confirms.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DataWipeSummary {
    /// Everything that would go, in bytes.
    pub total_bytes: u64,
    /// The downloaded AI models among it (the local LLM and the image-search model), in bytes.
    pub model_bytes: u64,
    /// A license key is activated on this Mac, so the dialog says how to get it back.
    pub has_license: bool,
}

/// Why a wipe didn't start. Nothing was deleted in either case.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DataWipeError {
    /// Copies, moves, or deletes are still running; they'd write into the operation log and
    /// the temp ledger mid-wipe. The person finishes or cancels them first.
    FileOperationsRunning { count: u32 },
    /// Another wipe is already running (a double confirm, or MCP and a click at once).
    AlreadyRunning,
    /// The background task died before it answered.
    Interrupted,
    /// Adding up the sizes took too long (the summary only; a wipe has no deadline).
    TimedOut,
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// The plan the wipe ran, kept for the exit sweep. Set once a wipe finished.
static WIPED: OnceLock<Vec<WipeTarget>> = OnceLock::new();

/// Whether this process has wiped its data. Exit-time writers check it so they don't write a
/// fresh state back on the way out (`window_state::save_on_exit`).
pub fn has_wiped() -> bool {
    WIPED.get().is_some()
}

/// The roots this instance uses, from the same helpers that put the data there.
fn roots<R: Runtime>(app: &AppHandle<R>) -> Option<WipeRoots> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let home_dir = dirs::home_dir();
    let library_dir = if cfg!(target_os = "macos") {
        home_dir.as_ref().map(|home| home.join("Library"))
    } else {
        None
    };
    Some(WipeRoots {
        data_dir,
        cache_dir: crate::config::app_cache_dir(),
        log_dir: crate::logging::log_dir().map(std::path::Path::to_path_buf),
        library_dir,
        identifier: app.config().identifier.clone(),
        home_dir,
    })
}

/// Sizes and the license flag for the confirmation. Walks the folders, so run it off the IPC
/// thread.
pub fn summary(app: &AppHandle) -> DataWipeSummary {
    let has_license = crate::licensing::get_license_info(app).is_some();
    let Some(roots) = roots(app) else {
        return DataWipeSummary {
            total_bytes: 0,
            model_bytes: 0,
            has_license,
        };
    };
    let sizes = summary::measure(&plan::build_plan(&roots), &roots.data_dir);
    DataWipeSummary {
        total_bytes: sizes.total,
        model_bytes: sizes.models,
        has_license,
    }
}

/// Stop what writes, delete everything, and report what's left. Blocking: it drains the drive
/// index's writers and walks gigabytes, so it runs on a blocking thread. The caller quits the
/// app afterwards; until then, [`on_exit`] has the plan for one more pass.
pub fn delete_all<R: Runtime>(app: &AppHandle<R>) -> Result<WipeReport, DataWipeError> {
    let running = crate::operations_host::operations()
        .map(cmdr_operations::Operations::list_operations)
        .unwrap_or_default()
        .iter()
        .filter(|op| crate::quit::blocks_quit(op))
        .count();
    if running > 0 {
        return Err(DataWipeError::FileOperationsRunning {
            count: u32::try_from(running).unwrap_or(u32::MAX),
        });
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(DataWipeError::AlreadyRunning);
    }
    let report = run(app);
    RUNNING.store(false, Ordering::SeqCst);
    Ok(report)
}

fn run<R: Runtime>(app: &AppHandle<R>) -> WipeReport {
    let Some(roots) = roots(app) else {
        log::warn!(target: "data_wipe", "the data dir didn't resolve, so there's nothing to delete");
        return WipeReport::default();
    };
    let plan = plan::build_plan(&roots);
    log::info!(target: "data_wipe", "deleting all Cmdr data: {} target(s)", plan.len());
    quiesce();
    let report = execute::wipe(
        &plan,
        &roots.identifier,
        crate::secrets::store(),
        &crate::secrets::item_name(),
        &OsHost,
    );
    log::info!(target: "data_wipe", "done, {} leftover(s)", report.leftovers.len());
    // Only the first wipe's plan is kept; a second one plans the same paths.
    let _ = WIPED.set(plan);
    report
}

/// Stop the writers that would otherwise race the delete or hold big files open. The drive
/// index goes through its own forget, which drains each volume's writer before deleting its
/// database. Anything not stopped here keeps its open handles into the tombstone, which is
/// harmless (POSIX keeps an unlinked file alive for its holder), and whatever it writes back by
/// path is the exit sweep's.
fn quiesce() {
    crate::search::cancel_all_live_runs();
    if let Err(e) = crate::index_host::index().forget_all_volumes() {
        log::warn!(target: "data_wipe", "the drive index didn't stop cleanly: {e}");
    }
    crate::search::drop_all_indices();
    crate::ai::manager::shutdown();
    crate::mcp::stop_mcp_server();
}

/// The last pass, at `RunEvent::Exit`, after the plugins have saved their stores: removes what
/// shutdown wrote back. Does nothing unless this process wiped.
pub fn on_exit() {
    if let Some(plan) = WIPED.get() {
        execute::remove_again(plan);
    }
}

/// Finishes a wipe that a crash interrupted: removes the tombstones it left beside each target.
/// Called once at launch on a background thread; costs a few directory reads.
pub fn sweep_interrupted<R: Runtime>(app: &AppHandle<R>) {
    let Some(roots) = roots(app) else { return };
    let plan = plan::build_plan(&roots);
    std::thread::spawn(move || {
        let removed = execute::sweep_tombstones(&plan);
        if removed > 0 {
            log::info!(target: "data_wipe", "finished an interrupted wipe: removed {removed} leftover(s)");
        }
    });
}

/// The real outside world: macOS preferences go through `defaults`, which tells `cfprefsd` to
/// drop its in-memory copy (deleting the plist alone, it writes the file straight back).
struct OsHost;

/// `defaults` answers in milliseconds; this only bounds a wedged `cfprefsd`.
const DEFAULTS_DEADLINE: Duration = Duration::from_secs(5);

impl execute::WipeHost for OsHost {
    fn forget_preferences(&self, identifier: &str) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let mut command = tokio::process::Command::new("/usr/bin/defaults");
        command.arg("delete").arg(identifier);
        let outcome = tauri::async_runtime::block_on(crate::subprocess::output_within(
            "defaults delete",
            DEFAULTS_DEADLINE,
            &mut command,
        ));
        match outcome {
            // A non-zero exit means there was no such domain, which is the goal anyway.
            Ok(_) => {}
            Err(e) => log::warn!(target: "data_wipe", "`defaults delete` didn't run: {e}"),
        }
    }
}
