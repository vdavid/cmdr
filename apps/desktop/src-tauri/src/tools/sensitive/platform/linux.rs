//! Linux: the XDG base dirs, which browsers and mail clients follow as well as tools
//! (Chrome's profile sits under `$XDG_CONFIG_HOME`, Firefox's from 147 on).

use std::path::PathBuf;

use super::{Dir, XDG_VARS, dirs_from};

pub(crate) fn moved_dirs() -> Vec<(Dir, PathBuf)> {
    dirs_from(XDG_VARS, |name| std::env::var_os(name))
}
