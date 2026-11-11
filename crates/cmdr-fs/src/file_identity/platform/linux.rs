//! `st_dev`, `st_ino`, and `st_nlink`, shared with the other Unix.

pub(crate) use super::unix::{device_of, file_id, link_count};
