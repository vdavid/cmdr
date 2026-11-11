//! macOS mounts every volume but the boot disk under `/Volumes/`.

pub(crate) fn user_mount_prefixes() -> &'static [&'static str] {
    &["/Volumes/"]
}
