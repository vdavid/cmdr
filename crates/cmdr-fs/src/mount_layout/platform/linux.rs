//! Linux: `/media/` for what udisks mounts on plug-in, `/mnt/` for what a person mounts by hand.

pub(crate) fn user_mount_prefixes() -> &'static [&'static str] {
    &["/mnt/", "/media/"]
}
