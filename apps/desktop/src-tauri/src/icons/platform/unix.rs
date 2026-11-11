use std::path::Path;

/// `getxattr`: `None` for an absent attribute and for any read error (permission, dead mount).
pub(crate) fn extended_attribute(path: &Path, name: &str) -> Option<Vec<u8>> {
    xattr::get(path, name).ok().flatten()
}
