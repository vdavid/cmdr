//! Brief mode's column widths: how wide each column of a pane in Brief mode has
//! to be for its widest name, from per-code-point widths the frontend measured.
//!
//! Rust can't see the system's fonts, so the frontend measures code points with
//! the Canvas API and hands the widths over ([`FontMetricsStore`]); from then on
//! a column's width is hash lookups over the names the pane draws
//! ([`compute_brief_column_text_widths`]). Hosted by the desktop app (which
//! persists each font's widths to disk) and by the browser demo
//! (`crates/cmdr-listing-wasm`, memory only). `CLAUDE.md` has the must-knows.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

mod columns;
mod metrics;

#[cfg(test)]
mod columns_test;
#[cfg(test)]
mod metrics_test;

pub use columns::{
    BriefColumnWidths, BriefColumnsError, BriefColumnsErrorKind, BriefColumnsIpcError, BriefColumnsQuery,
    compute_brief_column_text_widths,
};
pub use metrics::{FontMetrics, FontMetricsError, FontMetricsStore, widths_from_parallel};
