//! Brief mode's column widths on the desktop: `cmdr-brief-columns` over the app's
//! listing engine and font store, plus the E2E failure hook.

use cmdr_brief_columns::{BriefColumnWidths, BriefColumnsError, BriefColumnsQuery};

use crate::listing_host::listings;

/// How many more `compute_brief_column_text_widths` calls the E2E harness wants to fail.
///
/// The frontend's recovery (bounded retries, provisional widths, a cursor that stays
/// visible) only runs when measurement DOESN'T arrive, and nothing reachable from a spec
/// can make the real computation fail on demand: the listing is healthy and the font
/// metrics are loaded. So the failure is injected here.
#[cfg(feature = "playwright-e2e")]
pub static FAIL_NEXT_WIDTH_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Consumes one injected failure, if any are armed. Compiled out of production builds.
#[cfg(feature = "playwright-e2e")]
fn take_injected_failure() -> bool {
    use std::sync::atomic::Ordering;
    FAIL_NEXT_WIDTH_CALLS
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}

#[cfg(not(feature = "playwright-e2e"))]
fn take_injected_failure() -> bool {
    false
}

/// The widest filename's text-only width per Brief-mode column of the pane's
/// listing, from the app's font store (`cmdr_brief_columns::compute_brief_column_text_widths`).
pub fn compute_brief_column_text_widths(query: &BriefColumnsQuery) -> Result<BriefColumnWidths, BriefColumnsError> {
    // Additive E2E hook: `take_injected_failure` is a `const false` in production builds,
    // so this branch compiles away entirely. A zero `items_per_column` is rejected
    // before it, so it never consumes an armed failure.
    if query.items_per_column != 0 && take_injected_failure() {
        return Err(BriefColumnsError::Other("injected width failure (E2E)".to_string()));
    }
    cmdr_brief_columns::compute_brief_column_text_widths(listings(), super::store(), query)
}
