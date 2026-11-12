//! Where this OS keeps the per-user dirs it lets a person or an admin move. A platform seam
//! (`docs/guides/platform-seams.md`).
//!
//! The rules in `../rules.rs` spell each dir the way a home folder holds it (`.config`,
//! `AppData/Roaming`). A moved dir has no such spelling: `XDG_CONFIG_HOME=/data/cfg` puts
//! `gh`'s token at `/data/cfg/gh`, and a redirected `%APPDATA%` puts a Firefox profile on a
//! file server. Each arm names the variables its OS and the tools on it follow.

use std::ffi::OsString;
use std::path::PathBuf;

use super::rules::Dir;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

const _: fn() -> Vec<(Dir, PathBuf)> = moved_dirs;

/// The per-user dirs this process's environment places somewhere, as absolute paths. Reads the
/// environment only, never the disk, so it can't hang on a dead mount.
pub(super) use imp::moved_dirs;

/// The XDG base dirs. Command-line tools follow them on every OS, macOS and Windows included:
/// `gh` reads `$XDG_CONFIG_HOME/gh` before `%AppData%\GitHub CLI` (verified on gh 2.x,
/// `gh help environment`, 2026-11-12).
pub(crate) const XDG_VARS: &[(&str, Dir)] = &[
    ("XDG_CONFIG_HOME", Dir::XdgConfig),
    ("XDG_DATA_HOME", Dir::XdgData),
    ("XDG_CACHE_HOME", Dir::XdgCache),
];

/// The dirs `vars` name in the environment `read` answers for. A relative value is skipped:
/// the XDG spec calls it invalid, and it can't be matched against an absolute path anyway.
pub(crate) fn dirs_from(vars: &[(&str, Dir)], read: impl Fn(&str) -> Option<OsString>) -> Vec<(Dir, PathBuf)> {
    vars.iter()
        .filter_map(|(name, dir)| {
            let path = PathBuf::from(read(name)?);
            path.is_absolute().then_some((*dir, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<OsString> {
        match name {
            "XDG_CONFIG_HOME" => Some("/data/cfg".into()),
            "XDG_DATA_HOME" => Some("relative/share".into()),
            "XDG_CACHE_HOME" => Some("".into()),
            _ => None,
        }
    }

    #[test]
    fn only_absolute_values_count() {
        assert_eq!(
            dirs_from(XDG_VARS, env),
            vec![(Dir::XdgConfig, PathBuf::from("/data/cfg"))]
        );
    }

    #[test]
    fn an_unset_variable_adds_nothing() {
        assert_eq!(dirs_from(XDG_VARS, |_| None), Vec::new());
    }

    /// The arm reads the live environment and must hand back absolute paths only.
    #[test]
    fn this_os_answers_absolute_paths() {
        assert!(moved_dirs().iter().all(|(_, path)| path.is_absolute()));
    }
}
