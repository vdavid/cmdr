//! Windows: no clipboard reader or writer yet (`CF_HDROP`, with `Preferred DropEffect` for a cut,
//! is the one to add).

pub(crate) use super::unsupported::{paste_as_file, read_file_urls, read_text, write_file_urls};
