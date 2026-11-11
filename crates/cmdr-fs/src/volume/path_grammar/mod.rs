//! The app-path grammar: what a path the app holds looks like, on every OS, and the one
//! conversion to and from the path the local OS opens.
//!
//! An app path is **a scheme URL, or a `/`-separated absolute path whose first component
//! is the root**. The roots, by the local backend's [`PathFlavor`]:
//!
//! - [`PathFlavor::Posix`] (macOS, Linux): `/`.
//! - [`PathFlavor::Windows`]: a drive, `C:/` (the letter always uppercase), or a UNC
//!   share, `//server/share/` (server and share are ONE root component). Each is its own
//!   volume. A bare `/x` is no app path there, and neither is anything holding a `\`.
//!
//! It extends the rule [`super::remote_paths`] sets for remote volumes, that an app path
//! names its own volume: a restored tab, a favorite, or a drag that loses its volume hint
//! still lands on the right disk. A scheme URL's root is its volume's business, so the
//! grammar only does slash arithmetic on one.
//!
//! [`AppPath`] is a parsed, normalized LOCAL app path: an [`AppRoot`] plus the names under
//! it, `.` and `..` resolved lexically and clamped at the root, so a path that climbs off
//! its volume can't be built. [`AppPath::to_native`] and [`AppPath::from_native`] are the
//! ONE boundary where backslashes exist: Windows' `\\?\C:\…` long-path form (and
//! `\\?\UNC\server\share\…`). Both are pure and compiled on every OS, so Linux CI tests the
//! Windows conversion.
//!
//! The frontend holds the same grammar (`apps/desktop/src/lib/path/grammar.ts`), and both
//! suites run one case table, `path_grammar_cases.json` beside this module. Case
//! sensitivity is never the grammar's: it's a per-volume property (the app's
//! `LocationInfo.case_sensitive`).

mod platform;

use std::fmt;

/// Which roots the local backend has: `/` alone, or drives and UNC shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFlavor {
    /// macOS and Linux: one root, `/`.
    Posix,
    /// Windows: a root per drive (`C:/`) and per UNC share (`//server/share/`).
    Windows,
}

impl PathFlavor {
    /// The flavor of the OS this build runs on.
    pub const HOST: PathFlavor = platform::HOST_FLAVOR;
}

/// The shape of an app path's root, scheme URLs included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// `/…` on macOS and Linux.
    Posix,
    /// `C:/…` on Windows.
    Drive,
    /// `//server/share/…` on Windows.
    Unc,
    /// `<scheme>://…`: a volume with no local mount, whose root is its own business.
    Scheme,
}

/// The root a local app path starts at.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppRoot {
    /// `/`, the one root on macOS and Linux.
    Posix,
    /// `C:/`: always an uppercase ASCII letter.
    Drive(char),
    /// `//server/share/`, spelled as given: a server and share fold case on the wire, but
    /// the spelling is what a person typed or the OS reported.
    Unc {
        /// The server, never empty, never `?` or `.` (Win32's device namespaces).
        server: String,
        /// The share, one component, never empty.
        share: String,
    },
}

impl AppRoot {
    /// The shape of this root.
    pub fn kind(&self) -> PathKind {
        match self {
            AppRoot::Posix => PathKind::Posix,
            AppRoot::Drive(_) => PathKind::Drive,
            AppRoot::Unc { .. } => PathKind::Unc,
        }
    }
}

impl fmt::Display for AppRoot {
    /// The wire spelling, always ending in `/`: `/`, `C:/`, `//server/share/`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppRoot::Posix => f.write_str("/"),
            AppRoot::Drive(letter) => write!(f, "{letter}:/"),
            AppRoot::Unc { server, share } => write!(f, "//{server}/{share}/"),
        }
    }
}

/// A parsed, normalized local app path: a root and the names under it.
///
/// Built only by [`AppPath::parse`], [`AppPath::root_path`], [`AppPath::from_native`], and
/// the methods that derive one from another, none of which can climb above the root, so
/// every `AppPath` names a place on its own volume.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppPath {
    root: AppRoot,
    names: Vec<String>,
}

/// Win32's long-path prefix, and its UNC form.
const VERBATIM: &str = r"\\?\";
const VERBATIM_UNC: &str = r"\\?\UNC\";

/// The names in `rest`, `.` and empty ones dropped, `..` resolved lexically and clamped at
/// the root.
fn resolve_names(rest: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in rest.split('/') {
        match name {
            "" | "." => {}
            ".." => {
                names.pop();
            }
            _ => names.push(name.to_string()),
        }
    }
    names
}

/// Whether `text` starts with `<scheme>://`, the scheme being RFC 3986's shape.
pub fn is_scheme_url(text: &str) -> bool {
    let Some((scheme, _)) = text.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// `text`'s root and what follows it, or `None` when `text` is no local app path in `flavor`.
fn split_root(text: &str, flavor: PathFlavor) -> Option<(AppRoot, &str)> {
    match flavor {
        PathFlavor::Posix => text.strip_prefix('/').map(|rest| (AppRoot::Posix, rest)),
        PathFlavor::Windows => {
            if text.contains('\\') {
                return None;
            }
            let bytes = text.as_bytes();
            if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
                let rest = &text[2..];
                if !(rest.is_empty() || rest.starts_with('/')) {
                    return None;
                }
                return Some((AppRoot::Drive(char::from(bytes[0].to_ascii_uppercase())), rest));
            }
            let after = text.strip_prefix("//")?;
            let mut parts = after.splitn(3, '/');
            let server = parts.next().filter(|s| !s.is_empty() && *s != "?" && *s != ".")?;
            let share = parts.next().filter(|s| !s.is_empty())?;
            let rest = parts.next().unwrap_or("");
            let root = AppRoot::Unc {
                server: server.to_string(),
                share: share.to_string(),
            };
            Some((root, rest))
        }
    }
}

impl AppPath {
    /// Parses a local app path, normalizing it. `None` for a scheme URL, and for text that is
    /// no app path in `flavor`: relative, `~`-rooted, empty, or malformed (`//server`, `C:x`,
    /// anything with a `\` on Windows).
    pub fn parse(text: &str, flavor: PathFlavor) -> Option<AppPath> {
        if is_scheme_url(text) {
            return None;
        }
        let (root, rest) = split_root(text, flavor)?;
        Some(AppPath {
            root,
            names: resolve_names(rest),
        })
    }

    /// The root itself, as a path.
    pub fn root_path(root: AppRoot) -> AppPath {
        AppPath {
            root,
            names: Vec::new(),
        }
    }

    /// Where this path starts: `/`, `C:/`, or `//server/share/`.
    pub fn root(&self) -> &AppRoot {
        &self.root
    }

    /// The names under the root, outermost first.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Whether this is its volume's root.
    pub fn is_root(&self) -> bool {
        self.names.is_empty()
    }

    /// The folder holding this path, or `None` at a root: there is no parent INSIDE the
    /// volume, and what "up" from `C:/` shows is the UI's question, not the grammar's.
    pub fn parent(&self) -> Option<AppPath> {
        let (_, kept) = self.names.split_last()?;
        Some(AppPath {
            root: self.root.clone(),
            names: kept.to_vec(),
        })
    }

    /// The last name, or `None` at a root.
    pub fn file_name(&self) -> Option<&str> {
        self.names.last().map(String::as_str)
    }

    /// `relative` (one name or several, `/`-separated) inside this folder, `.` and `..`
    /// resolved and clamped at the root: a join never leaves the volume.
    pub fn join(&self, relative: &str) -> AppPath {
        let mut names = self.names.clone();
        for name in relative.split('/') {
            match name {
                "" | "." => {}
                ".." => {
                    names.pop();
                }
                _ => names.push(name.to_string()),
            }
        }
        AppPath {
            root: self.root.clone(),
            names,
        }
    }

    /// Whether `self` is `ancestor` or sits under it, by whole components. Exact, case
    /// included: a caller on a case-insensitive volume folds both sides first.
    pub fn starts_with(&self, ancestor: &AppPath) -> bool {
        self.root == ancestor.root && self.names.starts_with(&ancestor.names)
    }

    /// The path the local OS opens. On Windows the long-path form, which lifts `MAX_PATH`
    /// and skips Win32's own normalization (`\\?\C:\Users\ada`, `\\?\UNC\server\share\x`);
    /// on macOS and Linux the wire spelling itself. Pure, so it's tested on every OS.
    pub fn to_native(&self) -> String {
        match &self.root {
            AppRoot::Posix => self.to_string(),
            AppRoot::Drive(letter) => format!("{VERBATIM}{letter}:\\{}", self.names.join("\\")),
            AppRoot::Unc { server, share } => {
                let mut out = format!("{VERBATIM_UNC}{server}\\{share}\\");
                out.push_str(&self.names.join("\\"));
                out
            }
        }
    }

    /// The app path for a path the local OS reported, the inverse of [`Self::to_native`].
    ///
    /// On Windows it takes the long-path forms and the plain ones (`C:\x`, `\\server\share\x`,
    /// either separator), so an OS answer with or without the prefix lands on one spelling.
    /// `None` for anything that isn't absolute, and for Win32's other device paths
    /// (`\\.\PhysicalDrive0`, `\\?\Volume{…}`), which name no folder a pane can show.
    pub fn from_native(native: &str, flavor: PathFlavor) -> Option<AppPath> {
        match flavor {
            PathFlavor::Posix => AppPath::parse(native, flavor),
            PathFlavor::Windows => {
                let slashed = if let Some(unc) = native.strip_prefix(VERBATIM_UNC) {
                    format!("//{}", unc.replace('\\', "/"))
                } else if let Some(local) = native.strip_prefix(VERBATIM) {
                    local.replace('\\', "/")
                } else {
                    native.replace('\\', "/")
                };
                AppPath::parse(&slashed, flavor)
            }
        }
    }

    /// What a person reads: on Windows `C:\Users\ada`, `\\server\share\x` (a share's root
    /// without the trailing `\`); elsewhere the wire spelling.
    pub fn shown(&self) -> String {
        match &self.root {
            AppRoot::Posix => self.to_string(),
            AppRoot::Drive(letter) => format!("{letter}:\\{}", self.names.join("\\")),
            AppRoot::Unc { server, share } if self.names.is_empty() => format!(r"\\{server}\{share}"),
            AppRoot::Unc { server, share } => format!(r"\\{server}\{share}\{}", self.names.join("\\")),
        }
    }
}

impl fmt::Display for AppPath {
    /// The one wire spelling: the root, then the names joined by `/`, no trailing `/` except
    /// the root's own.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.root, self.names.join("/"))
    }
}

/// The shape of `text`'s root, or `None` when it's no app path in `flavor`.
pub fn path_kind(text: &str, flavor: PathFlavor) -> Option<PathKind> {
    if is_scheme_url(text) {
        return Some(PathKind::Scheme);
    }
    AppPath::parse(text, flavor).map(|path| path.root.kind())
}

/// Whether `text` is a local app path (`/…`, `C:/…`, `//server/share/…`) in `flavor`.
pub fn is_local_path(text: &str, flavor: PathFlavor) -> bool {
    AppPath::parse(text, flavor).is_some()
}

/// The one spelling of an app path: a local one normalized, a scheme URL as is, `None` for
/// no app path.
pub fn normalize_path(text: &str, flavor: PathFlavor) -> Option<String> {
    if is_scheme_url(text) {
        return Some(text.to_string());
    }
    AppPath::parse(text, flavor).map(|path| path.to_string())
}

/// The folder holding `text`, normalized, or `None` at a root and for no app path. A scheme
/// URL is plain slash arithmetic, as the frontend's twin does it.
pub fn parent_in_volume(text: &str, flavor: PathFlavor) -> Option<String> {
    if is_scheme_url(text) {
        return Some(text[..text.rfind('/').unwrap_or(0)].to_string());
    }
    AppPath::parse(text, flavor)?.parent().map(|parent| parent.to_string())
}

/// The last name in `text` (`""` at a root), or `None` for no app path.
pub fn basename_in_volume(text: &str, flavor: PathFlavor) -> Option<String> {
    if is_scheme_url(text) {
        return Some(text[text.rfind('/').map_or(0, |i| i + 1)..].to_string());
    }
    let path = AppPath::parse(text, flavor)?;
    Some(path.file_name().unwrap_or("").to_string())
}

/// What a person reads for `text`: a Windows local path with `\`, every other text as is.
pub fn shown_path(text: &str, flavor: PathFlavor) -> String {
    match AppPath::parse(text, flavor) {
        Some(path) if flavor == PathFlavor::Windows => path.shown(),
        _ => text.to_string(),
    }
}

/// What typed or pasted text becomes on the wire. On Windows both `\` and `/` separate and a
/// local path comes out normalized; text that isn't one (relative, `~`) only has its
/// separators turned. On macOS and Linux the text is untouched: `\` is a legal name character.
pub fn from_user_input(text: &str, flavor: PathFlavor) -> String {
    if flavor == PathFlavor::Posix || is_scheme_url(text) {
        return text.to_string();
    }
    let slashed = text.replace('\\', "/");
    normalize_path(&slashed, flavor).unwrap_or(slashed)
}

#[cfg(test)]
#[path = "../path_grammar_test.rs"]
mod path_grammar_test;
