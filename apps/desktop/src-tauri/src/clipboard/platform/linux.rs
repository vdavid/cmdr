//! Linux: no clipboard reader or writer yet (`text/uri-list` is the one to add).

pub(crate) use super::unsupported::{paste_as_file, read_file_urls, read_text, write_file_urls};
