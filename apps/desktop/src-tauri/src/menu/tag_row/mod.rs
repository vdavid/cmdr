//! The file context menu's Finder tag row (macOS): seven color circles on one line, with a
//! label under them that says what a click would do.
//!
//! The menu is built with seven plain tag items (`file_context_menu.rs`), and those stay
//! the fallback. When the menu starts tracking, `loan.rs` finds them, puts one `view.rs` row
//! on the first, and hides the other six. A click on a circle fires that circle's own item,
//! so `handle_menu_event` runs exactly as it does for the plain items.
//!
//! - `model.rs`: the portable rules (colors, layout, hit-testing, glyphs, labels, finding the
//!   seven items), tested on every platform.
//! - `view.rs` (macOS): the `NSView` that draws the row, follows the pointer, and gives
//!   VoiceOver one element per circle.
//! - `loan.rs` (macOS): arms the row before `popup()` and installs it on the tracking menu.

#[cfg(target_os = "macos")]
mod loan;
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "Finder tags are macOS-only; the portable half compiles everywhere so the Linux test lane covers it"
    )
)]
mod model;
#[cfg(target_os = "macos")]
mod view;

#[cfg(target_os = "macos")]
pub use loan::{lend_tag_row, set_applied};
#[cfg(target_os = "macos")]
pub(super) use model::{SWATCHES, ring_rgb};
