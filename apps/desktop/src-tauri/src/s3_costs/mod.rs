//! What a copy, move, or delete on S3 will cost, at list prices, for the line
//! the dialogs show ("About $0.02 at AWS list prices").
//!
//! The dialog's own scan preview supplies the files (`cached_cost_facts`), the
//! volumes supply the providers, `plan.rs` turns them into billed work per
//! provider, and `cmdr_s3::cost` prices it against the table `price_source.rs`
//! keeps current. ❗ No request goes to S3 for an estimate.

mod plan;
mod price_source;

use std::path::Path;

use cmdr_s3::S3Volume;
use cmdr_s3::cost::{Estimate, PriceTable};
use serde::{Deserialize, Serialize};

pub use plan::{ClashPlan, CostedOperation};
use plan::{Overwrite, Sides, overwritten, plan};

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::write_operations::{ScanCostFacts, cached_cost_facts};

/// What a dialog asks about, once its scan preview has settled.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimateRequest {
    pub operation: CostedOperation,
    /// The settled preview whose files the operation will touch.
    pub preview_id: String,
    pub source_volume_id: String,
    /// `None` for a delete.
    pub destination_volume_id: Option<String>,
    /// What the dialog's conflict check found at the destination, and the
    /// policy answering it, so overwrites are priced. `None` for a delete, or
    /// before the check answers. Only the clashes the check's one listing saw:
    /// a file inside a folder that merges isn't known until it's written.
    #[serde(default)]
    pub clashes: Option<ClashPlan>,
}

/// One provider's share of the cost.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    /// Unrounded; the dialog rounds it to the currency and hides a zero.
    pub amount: f64,
    /// ISO 4217 (`USD`, `EUR`), for the formatter.
    pub currency: String,
    /// The provider as its prices page names it (`AWS`, `Cloudflare R2`).
    pub provider_label: String,
}

/// The cost of `request` per S3 provider it touches. Empty when neither end is
/// an S3 place with list prices, or the preview isn't settled.
pub fn estimate(request: &CostEstimateRequest, data_dir: Option<&Path>) -> Vec<CostEstimate> {
    let manager = get_volume_manager();
    let source = manager.get(&request.source_volume_id);
    let destination = request.destination_volume_id.as_deref().and_then(|id| manager.get(id));
    let source_s3 = source.as_deref().and_then(|v| v.as_any().downcast_ref::<S3Volume>());
    let destination_s3 = destination
        .as_deref()
        .and_then(|v| v.as_any().downcast_ref::<S3Volume>());
    if source_s3.is_none() && destination_s3.is_none() {
        return Vec::new();
    }
    let Some(facts) = cached_cost_facts(&request.preview_id) else {
        return Vec::new();
    };
    let overwrites = request.clashes.as_ref().map(overwritten).unwrap_or_default();
    priced(
        request.operation,
        sides_of(source_s3, destination_s3),
        &facts,
        &overwrites,
        &price_source::current(data_dir),
    )
}

/// Each end's empty workload, and whether the two copy on the server.
fn sides_of(source: Option<&S3Volume>, destination: Option<&S3Volume>) -> Sides {
    Sides {
        source: source.map(S3Volume::cost_workload),
        destination: destination.map(S3Volume::cost_workload),
        server_copy: matches!((source, destination), (Some(from), Some(to)) if to.copies_on_server_from(from)),
    }
}

/// The billed work [`estimate`] prices for `operation` between these ends, for
/// a cell comparing it with the requests the engine actually sent
/// (`backend_suites/s3_engine_integration_test.rs`).
#[cfg(test)]
pub(crate) fn planned_workloads(
    operation: CostedOperation,
    source: Option<&S3Volume>,
    destination: Option<&S3Volume>,
    facts: &ScanCostFacts,
    clashes: Option<&ClashPlan>,
) -> Vec<cmdr_s3::cost::Workload> {
    let overwrites = clashes.map(overwritten).unwrap_or_default();
    plan(operation, sides_of(source, destination), facts, &overwrites)
}

/// What renaming an entry on `volume_id` costs when the rename runs as a move
/// (`RenameWork::CopyThenDelete`), from the files the rename editor's bounded
/// tally already listed. Empty off S3.
pub fn estimate_rename(volume_id: &str, facts: &ScanCostFacts, data_dir: Option<&Path>) -> Vec<CostEstimate> {
    let Some(volume) = get_volume_manager().get(volume_id) else {
        return Vec::new();
    };
    let Some(s3) = volume.as_any().downcast_ref::<S3Volume>() else {
        return Vec::new();
    };
    priced_rename(s3.cost_workload(), facts, &price_source::current(data_dir))
}

/// A rename within one place: a move whose copies run on the server.
fn priced_rename(workload: cmdr_s3::cost::Workload, facts: &ScanCostFacts, table: &PriceTable) -> Vec<CostEstimate> {
    let sides = Sides {
        source: Some(workload.clone()),
        destination: Some(workload),
        server_copy: true,
    };
    // A rename's new name is free: the editor refuses a taken one.
    priced(CostedOperation::Move, sides, facts, &[], table)
}

fn priced(
    operation: CostedOperation,
    sides: Sides,
    facts: &ScanCostFacts,
    overwrites: &[Overwrite],
    table: &PriceTable,
) -> Vec<CostEstimate> {
    plan(operation, sides, facts, overwrites)
        .iter()
        .filter_map(|work| Estimate::of(table, work))
        .map(|estimate| {
            log::debug!(target: "s3_costs", "{operation:?} at {}: {:?}", estimate.provider_label, estimate.line_items);
            CostEstimate {
                amount: estimate.total,
                currency: estimate.currency,
                provider_label: estimate.provider_label,
            }
        })
        .collect()
}

/// Whether every share shows as zero once rounded to its currency's cents, the
/// rule the dialogs' cost line hides by (`s3-cost-line.ts::roundsToZero`). The
/// table's currencies (USD, EUR) all have two decimals.
pub fn rounds_to_zero(estimates: &[CostEstimate]) -> bool {
    estimates.iter().all(|estimate| estimate.amount.abs() < 0.005)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod plan_tests;

#[cfg(test)]
#[path = "rename_tests.rs"]
mod rename_tests;
