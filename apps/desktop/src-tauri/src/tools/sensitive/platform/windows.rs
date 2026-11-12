//! Windows: the two `AppData` folders, which folder redirection and roaming profiles put on a
//! file server under any name, plus the XDG dirs a command-line tool follows.
//!
//! `%APPDATA%` and `%LOCALAPPDATA%` are what Windows sets for the session from the known
//! folders, so they follow a redirect without a shell call.

use std::path::PathBuf;

use super::{Dir, XDG_VARS, dirs_from};

const APPDATA_VARS: &[(&str, Dir)] = &[("APPDATA", Dir::WinRoaming), ("LOCALAPPDATA", Dir::WinLocal)];

pub(crate) fn moved_dirs() -> Vec<(Dir, PathBuf)> {
    let mut dirs = dirs_from(XDG_VARS, |name| std::env::var_os(name));
    dirs.extend(dirs_from(APPDATA_VARS, |name| std::env::var_os(name)));
    dirs
}
