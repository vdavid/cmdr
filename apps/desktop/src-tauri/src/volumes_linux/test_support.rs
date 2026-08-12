//! The `/proc/mounts` fixture the discovery tests share. One sample covering
//! every branch the filters take (virtual, root, plain, removable) beats a
//! per-module copy that drifts.

use crate::file_system::linux_mounts::{self, MountEntry};

const SAMPLE_MOUNTS: &str = "\
sysfs /sys sysfs rw,nosuid,nodev,noexec,relatime 0 0
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/sda1 / ext4 rw,relatime 0 0
/dev/sda2 /home ext4 rw,relatime 0 0
/dev/sdb1 /mnt/data xfs rw,relatime 0 0
tmpfs /tmp tmpfs rw,nosuid,nodev 0 0
/dev/sdc1 /run/media/testuser/USB btrfs rw,relatime 0 0
";

pub(super) fn parse_test_mounts() -> Vec<MountEntry> {
    linux_mounts::parse_proc_mounts_from_content(SAMPLE_MOUNTS)
}
