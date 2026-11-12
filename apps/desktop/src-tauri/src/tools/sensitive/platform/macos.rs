//! macOS: `~/Library` can't move, so only the XDG dirs a command-line tool follows can.

use std::path::PathBuf;

use super::{Dir, XDG_VARS, dirs_from};

pub(crate) fn moved_dirs() -> Vec<(Dir, PathBuf)> {
    dirs_from(XDG_VARS, |name| std::env::var_os(name))
}
