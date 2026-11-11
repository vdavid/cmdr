//! Windows gives each volume its own drive letter (or a folder mount anywhere), so no prefix
//! names one yet.

pub(crate) fn user_mount_prefixes() -> &'static [&'static str] {
    &[]
}
