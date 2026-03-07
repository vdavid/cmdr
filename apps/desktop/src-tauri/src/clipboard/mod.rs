//! Clipboard file operations for copy/cut/paste workflows.

#[cfg(target_os = "macos")]
mod pasteboard;
mod state;

pub use state::clear_cut_state;
#[cfg(target_os = "macos")]
pub use state::{get_cut_state, set_cut_state};

#[cfg(target_os = "macos")]
pub use pasteboard::{read_file_urls_from_clipboard, read_text_from_clipboard, write_file_urls_to_clipboard};
