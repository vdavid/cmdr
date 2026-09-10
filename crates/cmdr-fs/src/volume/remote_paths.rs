//! The two spellings of a remote volume's tree, and the one translation between
//! them.
//!
//! A remote backend has no mount, so it needs an app-facing spelling of its
//! files that is self-describing. `sftp://ada@nas.local:22/srv/data/photos` is
//! one; `/srv/data/photos` is not, and that matters because
//! `commands/volumes.rs::resolve_path_to_volume` falls through to the mount
//! table, which on both platforms walks up to `/` and answers the LOCAL root for
//! any absolute path it doesn't recognize. Every un-hinted resolver call (a
//! restored tab, a favorite, a drag between panes, go-to-path, MCP, the trash)
//! would send a remote path to the boot disk. It is also the convention the app
//! already has for every volume without a local mount (`mtp://`, `adb://`).
//!
//! So a remote volume is rooted at `<prefix><remote root>`, where the prefix is
//! minted beside the volume id in [`super::ids`] (`sftp_app_root`,
//! `webdav_app_root`, `adb_app_root`) and the tree under it is the server's own. [`RemoteRoot`]
//! holds both and is the ONLY translation: `to_remote_path` going down,
//! `to_app_path` coming back.
//!
//! ❗ **A bare server-absolute path is REFUSED, not anchored.** Five app sites
//! run a path through [`super::root_anchored`] before the volume sees it, and
//! that helper JOINS anything not under the root onto it: `/srv/data/x` becomes
//! `sftp://…/srv/data/srv/data/x`, strips back to a real server path, and never
//! refuses. With the prefix in place the app never spells a remote path bare, so
//! leniency buys nothing but that hole. The three root aliases stay (`/`, `.`,
//! and the empty path), because a backend's own code passes a bare `/` for its
//! root (`cmdr-webdav`'s `query.rs` does, from `is_root` and
//! `get_space_info_impl`).

use std::path::{Component, Path, PathBuf};

/// A remote volume's root, in both the spellings it has.
///
/// Built once per volume and held by it, so the translation costs no allocation
/// per call beyond the answer itself.
pub struct RemoteRoot {
    /// `sftp://<user>@<host>:<port>`: what every app path on this volume starts
    /// with. From [`super::ids::sftp_app_root`] or one of its twins, ❌ never
    /// hand-built, so a path and the id it resolves to can't disagree.
    prefix: String,
    /// The same, as a `Path`, so the match is by whole COMPONENTS. A string
    /// compare would let `…nas.local:2222` borrow `…nas.local:22`'s volume.
    prefix_path: PathBuf,
    /// What the app addresses the root by: `<prefix><remote>`.
    app: PathBuf,
    /// The server-side directory this volume is rooted at, normalized and
    /// absolute.
    remote: PathBuf,
}

impl RemoteRoot {
    /// Builds a root from its prefix and the server-side directory under it.
    ///
    /// `remote_root` is normalized and made absolute, so a backend handed a
    /// relative or `..`-carrying root still ends up with one spelling of it.
    pub fn new(prefix: String, remote_root: &Path) -> Self {
        let remote = normalize(&Path::new("/").join(remote_root));
        let app = PathBuf::from(format!("{prefix}{}", remote.to_string_lossy()));
        Self {
            prefix_path: PathBuf::from(&prefix),
            prefix,
            app,
            remote,
        }
    }

    /// What the app addresses this volume's root by, and what `Volume::root`
    /// answers.
    pub fn app_root(&self) -> &Path {
        &self.app
    }

    /// The server-side directory this volume is rooted at.
    pub fn remote_root(&self) -> &Path {
        &self.remote
    }

    /// The absolute server-side path for `path`, or `None` when `path` isn't on
    /// this volume.
    ///
    /// Accepts the three root aliases, this volume's own prefixed paths, and
    /// paths relative to the root. ❌ Refuses everything else, including a bare
    /// server-absolute path and another server's prefix.
    ///
    /// # Two ways of guessing that would each send a request somewhere wrong
    ///
    /// - **The prefix and the root are matched by whole COMPONENTS.** A string
    ///   prefix compare strips `/srv/data` off a sibling `/srv/data-1/photos`
    ///   and asks the server for `-1/photos`, which is a legal name.
    /// - **`..` is resolved before the check, never after.**
    ///   `photos/../../etc` is the same escape spelled relatively, and the
    ///   server would resolve it happily.
    pub fn to_remote_path(&self, path: &Path) -> Option<String> {
        // `.`, `/`, and the empty path are how the app (and a backend's own
        // code) spell "this volume's root".
        if path.as_os_str().is_empty() || path == Path::new(".") || path == Path::new("/") {
            return Some(self.remote.to_string_lossy().into_owned());
        }
        let joined = if let Ok(under_prefix) = path.strip_prefix(&self.prefix_path) {
            normalize(&Path::new("/").join(under_prefix))
        } else if path.is_absolute() || carries_a_scheme(path) {
            // A bare server path, or another server's, or another backend's.
            // None of the three is on this volume.
            return None;
        } else {
            normalize(&self.remote.join(path))
        };
        joined
            .starts_with(&self.remote)
            .then(|| joined.to_string_lossy().into_owned())
    }

    /// The app-facing spelling of a server-side path: the exact inverse of
    /// [`to_remote_path`](Self::to_remote_path).
    ///
    /// This is what a backend's `display_path_for` answers, so the listing-cache
    /// patcher spells paths the way the panes hold them.
    pub fn to_app_path(&self, remote: &str) -> PathBuf {
        PathBuf::from(format!("{}{remote}", self.prefix))
    }
}

/// Whether `path` opens with a `<scheme>://` that isn't this volume's.
///
/// Checked separately from `is_absolute` because a scheme path is RELATIVE in
/// `Path`'s terms (`sftp://…` starts with a `sftp:` component), so without this
/// another server's path would be joined onto our root and sent to ours.
fn carries_a_scheme(path: &Path) -> bool {
    path.to_string_lossy()
        .split_once("://")
        .is_some_and(|(scheme, _)| !scheme.is_empty() && !scheme.contains('/'))
}

/// Resolves `.` and `..` lexically, with no round trip and no symlink following.
///
/// Lexical on purpose: asking the server would be a round trip per path AND a
/// TOCTOU window, and the question here is "did the caller address something
/// outside this volume", which is about the path they wrote rather than about
/// what it resolves to. `..` at the root is absorbed, matching what a POSIX
/// server does with `/..`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push(Component::RootDir),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
            // Windows-only, and every backend here is reached over a network
            // from wherever Cmdr runs, so there is no drive letter to carry.
            Component::Prefix(_) => {}
        }
    }
    if out.as_os_str().is_empty() {
        out.push(Component::RootDir);
    }
    out
}

#[cfg(test)]
#[path = "remote_paths_test.rs"]
mod remote_paths_test;
