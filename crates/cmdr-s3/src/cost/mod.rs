//! What an operation will cost, at the provider's list prices.
//!
//! Three pieces: the price table (`prices.rs`, the JSON `apps/api-server` serves
//! and this crate bundles as the fallback), a [`Workload`] (`workload.rs`: the
//! billed requests, downloaded bytes, and deleted objects one planned operation
//! sends to one provider, counted the way this backend actually sends them), and
//! the sum (`estimate.rs`). Nothing here sends a request: the inputs come from
//! the scan the dialog already ran.
//!
//! Decisions and the math: `crates/cmdr-s3/DETAILS.md` § "Cost estimates".

mod estimate;
mod prices;
mod workload;

pub use estimate::{Estimate, LineItem};
pub use prices::{PriceTable, PriceTableError};
pub use workload::Workload;

#[cfg(test)]
#[path = "cost_test.rs"]
mod cost_test;
