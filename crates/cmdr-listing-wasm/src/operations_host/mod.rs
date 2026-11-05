//! The write engine's demo host: `cmdr-operations` and the operation log over
//! the page's one in-memory volume, answering the desktop's write, queue, and
//! undo commands by their Tauri names. The mirror of
//! `apps/desktop/src-tauri/src/operations_host/`: seams (`volumes.rs`,
//! `events.rs`), per-op sinks (`sinks.rs`), event names (`wire.rs`), and the
//! thin command layer the desktop keeps in `commands/` (`commands/` here).
//!
//! The volume models one device (`crate::commands::Demo::new` sets its options):
//! synthetic content, shared bandwidth, a breath every slice, live free space,
//! and pane patches. `DETAILS.md` § "Writes".

mod commands;
#[cfg(all(test, not(target_family = "wasm")))]
mod commands_test;
mod events;
#[cfg(test)]
mod golden_test;
mod sinks;
mod volumes;
pub(crate) mod wire;

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_collation::NameCollator;
use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_listing::Listings;
use cmdr_operation_log::OperationLog;
use cmdr_operations::host::{NameOrder, OperationIndex};
use cmdr_operations::{OperationEventSink, Operations};

use crate::events::EventSink;

/// The engine over the page's one volume, and where its events go.
#[derive(Clone)]
pub struct DemoOperations {
    engine: Operations,
    volume_id: String,
    volume: Arc<dyn Volume>,
    /// The registry the engine resolves volume ids through.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only the golden replay registers more volumes")
    )]
    volumes: Arc<volumes::DemoVolumes>,
    sink: Arc<dyn EventSink>,
    log_db: PathBuf,
}

impl DemoOperations {
    /// The write engine over `volume` (filed under `volume_id`), on `volume_host`
    /// (which carries the panes), sorting names as `listings` does, sizing
    /// folders through `index`, journaling into a fresh operation log, and
    /// guarding `home` (the visitor's `/Users/you`) against delete and trash.
    #[allow(
        clippy::too_many_arguments,
        reason = "the host's seams, each its own value; a struct would only rename them"
    )]
    pub(crate) fn new(
        volume_id: String,
        volume: Arc<dyn Volume>,
        volume_host: VolumeHost,
        listings: &Listings,
        index: Arc<dyn OperationIndex>,
        sink: Arc<dyn EventSink>,
        home: Option<PathBuf>,
    ) -> Self {
        let volumes = Arc::new(volumes::DemoVolumes::new(volume_id.clone(), Arc::clone(&volume)));
        let registry = Arc::clone(&volumes);
        let events = Arc::new(events::DemoEngineEvents {
            sink: Arc::clone(&sink),
        });
        let log = OperationLog::detached();
        let log_db = log_db_path();
        // A log that can't open leaves the demo without undo, never without its
        // file operations (the app makes the same call).
        if let Err(e) = log.open(&log_db, &|_| None) {
            log::warn!(target: "operation_log", "the demo's operation log didn't open: {e}");
        }
        let mut builder = Operations::builder(volumes, events)
            .volume_host(volume_host)
            .operation_log(log)
            .index(index)
            .name_order(Arc::new(PaneNameOrder {
                listings: listings.clone(),
            }));
        if let Some(home) = home {
            builder = builder.home_folder(home);
        }
        let engine = builder.build();
        // Unpersisted on purpose: a page has nowhere to keep a ledger across
        // reloads, and an in-memory volume loses its leftovers with it.
        let _ = engine.claim_ledger(None);
        Self {
            engine,
            volume_id,
            volume,
            volumes: registry,
            sink,
            log_db,
        }
    }

    /// The engine itself, for a caller driving its entry points directly (the
    /// golden replay).
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &Operations {
        &self.engine
    }

    /// The operation log's database.
    #[cfg(test)]
    pub(crate) fn log_db(&self) -> &std::path::Path {
        &self.log_db
    }

    /// Files another volume beside the page's.
    #[cfg(test)]
    pub(crate) fn register_volume(&self, volume_id: &str, volume: Arc<dyn Volume>) {
        self.volumes.register(volume_id, volume);
    }

    /// Whether [`Self::invoke`] answers `command`.
    pub fn handles(command: &str) -> bool {
        commands::handles(command)
    }

    /// Every command [`Self::invoke`] answers, by its Tauri name.
    pub fn commands() -> impl Iterator<Item = &'static str> {
        commands::names()
    }

    /// A fresh sink for one operation.
    fn op_sink(&self) -> Arc<dyn OperationEventSink> {
        Arc::new(sinks::DemoOperationSink {
            sink: Arc::clone(&self.sink),
            engine: self.engine.clone(),
        })
    }
}

/// The panes' collator, asked per call as the desktop's `AppNameOrder` does.
struct PaneNameOrder {
    listings: Listings,
}

impl NameOrder for PaneNameOrder {
    fn collator(&self) -> Arc<NameCollator> {
        self.listings.collator()
    }
}

/// Where the demo's operation log lives: a path on SQLite's in-memory VFS in the
/// browser (gone on reload, like the volume), a scratch file natively (tests).
/// Numbered per engine, so two in one module (the golden replay's wasm tests)
/// never share a journal.
fn log_db_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[cfg(target_family = "wasm")]
    {
        PathBuf::from(format!(
            "/demo/operation-log-{}.db",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[cfg(not(target_family = "wasm"))]
    {
        std::env::temp_dir().join(format!(
            "cmdr-demo-operation-log-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
