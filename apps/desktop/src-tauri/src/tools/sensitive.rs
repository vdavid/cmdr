//! The one gate for every path that yields file contents or image-derived text, for both
//! callers: `inspect_file`, `image_facts`, `search_photos`, and `multi_rename`'s media
//! fields. It blocks CONTENTS only: file names in these folders may still reach a provider.
//!
//! - [`classify`] is pure: path components against the rules. It's the whole check
//!   for a path with no realpath (a phone, a routed path, a stored index path).
//! - [`path_gate`] adds Cmdr's real data dir and the path after following symlinks. It can
//!   block on a hung mount, so it only runs where a deadline can abandon it (`inspect`'s
//!   runner). [`path_gate_within`] brings its own deadline for callers without one.
//! - [`refuse_private_key_text`] catches a key copied somewhere no path rule can see.
//!
//! The rules are `sensitive/rules.rs`; where this OS moved its per-user dirs is
//! `sensitive/platform/`. Why each rule, and which call sites: `DETAILS.md` § The
//! sensitive-content gate.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::security_log::{self, ContentTool, ProtectedContent};

mod platform;
mod rules;

use rules::{Dir, Moved};

/// What a refused row tells the model, so it relays a refusal instead of retrying or
/// guessing at a permission problem.
pub(crate) const PROTECTED_NOTE: &str = "Cmdr doesn't hand this file's contents to AI tools. Tell the user; asking again won't change it. The file's name is fine to mention.";

/// Why a file's contents stay home. A token on the wire, so the model can relay it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Protected {
    /// SSH, GPG, cloud, and tool credentials, the OS credential stores, and password managers.
    Credentials,
    /// A browser's profile: cookies, saved passwords, history.
    BrowserData,
    /// Mail, messages, and notes: Apple's Mail, Messages, and Notes, and the mail clients
    /// each OS has (Thunderbird, Outlook, Evolution, Geary, KMail).
    MessageStore,
    /// Cmdr's own data dir: tokens, databases, raw logs.
    CmdrData,
    /// The file's text holds a private key, wherever the file lives.
    PrivateKey,
}

impl Protected {
    /// A method here rather than a `From` on the log's type: `module-cycles` reads an impl as the
    /// module defining the type it produces, which would weld `security_log` to this file.
    fn log_reason(self) -> ProtectedContent {
        match self {
            Protected::Credentials => ProtectedContent::Credentials,
            Protected::BrowserData => ProtectedContent::BrowserData,
            Protected::MessageStore => ProtectedContent::MessageStore,
            Protected::CmdrData => ProtectedContent::CmdrData,
            Protected::PrivateKey => ProtectedContent::PrivateKey,
        }
    }
}

/// The security log's line for one refusal: the tool and the reason, never the path.
pub(crate) fn note_refusal(tool: ContentTool, reason: Protected) {
    security_log::note_content_refusal(tool, reason.log_reason());
}

/// Why [`path_gate_within`] said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateStop {
    Protected(Protected),
    /// The resolve didn't answer in time (a hung mount). Fails closed: no read.
    TimedOut,
}

/// Any component starting with this is a Cmdr data dir: every instance, so a dev build can't
/// read the prod `mcp.token`. A custom `CMDR_DATA_DIR` is [`path_gate`]'s job.
const CMDR_COMPONENT_PREFIX: &str = "com.veszelovszki.cmdr";

/// One component the way every OS's filesystem could mean it: NFC, lowercased, and in the
/// spellings Win32 opens as the same name. `AppData.` and `AppData ` open `AppData`, and
/// `id_rsa::$DATA` or `id_rsa:x` reads a stream of `id_rsa`. Folding them on every OS only
/// over-refuses, and a Windows share reached from a Mac needs it.
fn fold(component: &str) -> String {
    let folded = component.nfc().collect::<String>().to_lowercase();
    let name = folded.split(':').next().unwrap_or_default();
    let name = name.trim_end_matches(['.', ' ']);
    match name.is_empty() {
        true => folded,
        false => name.to_string(),
    }
}

/// A path's components, folded, with `.` gone and `..` applied. Splits on `\` as well as `/`:
/// an archive entry or an SMB path can carry either.
fn folded_components(path: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(fold(other)),
        }
    }
    parts
}

fn folded_dirs(dirs: &[(Dir, PathBuf)]) -> Vec<Moved> {
    dirs.iter()
        .map(|(dir, path)| (*dir, folded_components(&path.to_string_lossy())))
        .collect()
}

/// The per-user dirs this process's environment moved, read once: the environment doesn't
/// change under a running app.
static MOVED_DIRS: LazyLock<Vec<(Dir, PathBuf)>> = LazyLock::new(platform::moved_dirs);
static MOVED: LazyLock<Vec<Moved>> = LazyLock::new(|| folded_dirs(&MOVED_DIRS));

/// Which rule protects `path`, if any. Pure: no filesystem, so it can't see a symlink.
pub(crate) fn classify(path: &str) -> Option<Protected> {
    classify_on(path, &MOVED)
}

/// [`classify`] for a host that keeps its per-user dirs at `moved`.
fn classify_on(path: &str, moved: &[Moved]) -> Option<Protected> {
    let parts = folded_components(path);
    if parts.iter().any(|part| part.starts_with(CMDR_COMPONENT_PREFIX)) {
        return Some(Protected::CmdrData);
    }
    rules::protecting(&parts, moved)
}

/// Whether `path` is `dir` or sits under it, comparing folded components.
fn is_under(path: &str, dir: &str) -> bool {
    let dir = folded_components(dir);
    !dir.is_empty() && folded_components(path).starts_with(&dir)
}

/// `path` with symlinks followed: the longest prefix that exists, resolved, plus the rest.
/// A prefix is what makes `~/link.zip/inner` and `~/link-to-ssh/gone.txt` resolve at all.
/// Blocking.
fn resolve_existing(path: &Path) -> Option<PathBuf> {
    let mut rest = Vec::new();
    for ancestor in path.ancestors() {
        if let Ok(real) = std::fs::canonicalize(ancestor) {
            return Some(rest.iter().rev().fold(real, |joined, part| joined.join(part)));
        }
        rest.push(ancestor.file_name()?.to_os_string());
    }
    None
}

/// Whether contents of `path` may be read. Checks the rules and Cmdr's data dir on the path as
/// given and again after following symlinks. Blocking: run it under a deadline.
pub(crate) fn path_gate(path: &str) -> Result<(), Protected> {
    gate_against(path, crate::config::standalone_app_data_dir().as_deref())
}

/// [`path_gate`] on the path as given, following no link: the rules plus Cmdr's real data dir.
/// Pure, for a caller that may not touch the volume (a DB-only read on a turn Cmdr started).
pub(crate) fn names_gate(path: &str) -> Result<(), Protected> {
    check_rules(path, crate::config::standalone_app_data_dir().as_deref(), &MOVED)
}

fn gate_against(path: &str, data_dir: Option<&Path>) -> Result<(), Protected> {
    gate_on(path, data_dir, &MOVED_DIRS)
}

/// The gate for a host whose data dir and moved per-user dirs are these.
fn gate_on(path: &str, data_dir: Option<&Path>, moved: &[(Dir, PathBuf)]) -> Result<(), Protected> {
    check_rules(path, data_dir, &folded_dirs(moved))?;
    // A phone, a server, or a routed scheme has no realpath: its components were the whole check.
    if crate::tools::is_virtual_path(path) || !Path::new(path).is_absolute() {
        return Ok(());
    }
    let Some(real) = resolve_existing(Path::new(path)) else {
        return Ok(());
    };
    let real = real.to_string_lossy();
    check_rules(&real, data_dir, &folded_dirs(moved))?;
    // The host's dirs resolved too: `/tmp` and `/var` are links on a Mac, so a resolved path
    // never sits under the name a custom `CMDR_DATA_DIR` or `XDG_CONFIG_HOME` gives its dir.
    let real_data_dir = data_dir.and_then(resolve_existing);
    let real_moved: Vec<(Dir, PathBuf)> = moved
        .iter()
        .filter_map(|(dir, path)| Some((*dir, resolve_existing(path)?)))
        .collect();
    check_rules(&real, real_data_dir.as_deref(), &folded_dirs(&real_moved))
}

fn check_rules(path: &str, data_dir: Option<&Path>, moved: &[Moved]) -> Result<(), Protected> {
    if let Some(reason) = classify_on(path, moved) {
        return Err(reason);
    }
    match data_dir {
        Some(dir) if is_under(path, &dir.to_string_lossy()) => Err(Protected::CmdrData),
        _ => Ok(()),
    }
}

/// [`path_gate`] for a caller with no deadline of its own: the rules run here, the resolve on a
/// thread that's abandoned at `budget` if a hung mount holds it.
pub(crate) fn path_gate_within(path: &str, budget: Duration) -> Result<(), GateStop> {
    if let Some(reason) = classify(path) {
        return Err(GateStop::Protected(reason));
    }
    let (send, answer) = channel();
    let owned = path.to_string();
    let spawned = std::thread::Builder::new()
        .name("sensitive-path-gate".into())
        .spawn(move || {
            // The receiver is gone when the deadline passed first; nobody needs the answer then.
            let _ = send.send(path_gate(&owned));
        });
    if spawned.is_err() {
        return Err(GateStop::TimedOut);
    }
    match answer.recv_timeout(budget) {
        Ok(result) => result.map_err(GateStop::Protected),
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Err(GateStop::TimedOut),
    }
}

/// A PEM or PGP private key's armor line, either end. Case-sensitive, as the armor is: prose
/// about "a private key" doesn't trip it. Also fed to the viewer's matcher by `inspect/find.rs`.
pub(crate) const PRIVATE_KEY_PATTERN: &str = r"(?:BEGIN|END)[A-Z0-9 ]*PRIVATE KEY";

static PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(PRIVATE_KEY_PATTERN).expect("the private-key pattern is a constant that compiles"));

/// Refuses text holding a private key's armor line. Run it where text windows, `find` lines,
/// PDF pages, and OCR text form, over MORE than what's handed on: see the callers.
pub(crate) fn refuse_private_key_text(text: &str) -> Result<(), Protected> {
    match PRIVATE_KEY.is_match(text) {
        true => Err(Protected::PrivateKey),
        false => Ok(()),
    }
}

#[cfg(test)]
mod locations_tests;
#[cfg(test)]
#[path = "sensitive_tests.rs"]
mod tests;
