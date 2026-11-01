//! The one gate for every path that yields file contents or image-derived text, for both
//! callers: `inspect_file`, `image_facts`, `search_photos`, and `multi_rename`'s media
//! fields. It blocks CONTENTS only: file names in these folders may still reach a provider.
//!
//! - [`classify`] is pure: path components against the rules below. It's the whole check
//!   for a path with no realpath (a phone, a routed path, a stored index path).
//! - [`path_gate`] adds Cmdr's real data dir and the path after following symlinks. It can
//!   block on a hung mount, so it only runs where a deadline can abandon it (`inspect`'s
//!   runner). [`path_gate_within`] brings its own deadline for callers without one.
//! - [`refuse_private_key_text`] catches a key copied somewhere no path rule can see.
//!
//! Why each rule, and which call sites: `../DETAILS.md` § The sensitive-content gate.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::security_log::{self, ContentTool, ProtectedContent};

/// What a refused row tells the model, so it relays a refusal instead of retrying or
/// guessing at a permission problem.
pub(crate) const PROTECTED_NOTE: &str = "Cmdr doesn't hand this file's contents to AI tools. Tell the user; asking again won't change it. The file's name is fine to mention.";

/// Why a file's contents stay home. A token on the wire, so the model can relay it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Protected {
    /// SSH, GPG, cloud, and tool credentials, and the OS keychains.
    Credentials,
    /// A browser's profile: cookies, saved passwords, history.
    BrowserData,
    /// The Full Disk Access stores: Mail, Messages, and Notes.
    MessageStore,
    /// Cmdr's own data dir: tokens, databases, raw logs.
    CmdrData,
    /// The file's text holds a private key, wherever the file lives.
    PrivateKey,
}

impl From<Protected> for ProtectedContent {
    fn from(reason: Protected) -> Self {
        match reason {
            Protected::Credentials => Self::Credentials,
            Protected::BrowserData => Self::BrowserData,
            Protected::MessageStore => Self::MessageStore,
            Protected::CmdrData => Self::CmdrData,
            Protected::PrivateKey => Self::PrivateKey,
        }
    }
}

/// The security log's line for one refusal: the tool and the reason, never the path.
pub(crate) fn note_refusal(tool: ContentTool, reason: Protected) {
    security_log::note_content_refusal(tool, reason.into());
}

/// Why [`path_gate_within`] said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateStop {
    Protected(Protected),
    /// The resolve didn't answer in time (a hung mount). Fails closed: no read.
    TimedOut,
}

/// A run of folded path components that protects whatever sits at or under it.
struct Rule {
    parts: &'static [&'static str],
    reason: Protected,
}

const fn rule(parts: &'static [&'static str], reason: Protected) -> Rule {
    Rule { parts, reason }
}

/// Folded, matched anywhere in the path, so a NAS backup of a home folder counts.
///
/// ❌ Never a bare `notes`, `edge`, `arc`, `brave`, `chrome`, `safari`, or `firefox`: at any
/// depth, case-folded, those would refuse `~/Documents/notes/*`. Browsers stay anchored.
const RULES: &[Rule] = &[
    rule(&[".ssh"], Protected::Credentials),
    rule(&[".gnupg"], Protected::Credentials),
    rule(&[".password-store"], Protected::Credentials),
    rule(&[".aws"], Protected::Credentials),
    rule(&[".azure"], Protected::Credentials),
    rule(&[".kube"], Protected::Credentials),
    rule(&[".config", "gcloud"], Protected::Credentials),
    rule(&[".config", "gh"], Protected::Credentials),
    rule(&[".docker", "config.json"], Protected::Credentials),
    // Tokens in remote URLs.
    rule(&[".git", "config"], Protected::Credentials),
    rule(&[".cargo", "credentials.toml"], Protected::Credentials),
    rule(&[".claude", ".credentials.json"], Protected::Credentials),
    rule(&["sops", "age", "keys.txt"], Protected::Credentials),
    rule(&["library", "keychains"], Protected::Credentials),
    rule(&[".local", "share", "keyrings"], Protected::Credentials),
    rule(&["library", "mail"], Protected::MessageStore),
    rule(&["library", "messages"], Protected::MessageStore),
    rule(
        &["library", "group containers", "group.com.apple.notes"],
        Protected::MessageStore,
    ),
    rule(&[".mozilla"], Protected::BrowserData),
    rule(&["library", "cookies"], Protected::BrowserData),
    rule(&["library", "safari"], Protected::BrowserData),
    rule(
        &["library", "application support", "google", "chrome"],
        Protected::BrowserData,
    ),
    rule(&["library", "application support", "chromium"], Protected::BrowserData),
    rule(
        &["library", "application support", "bravesoftware"],
        Protected::BrowserData,
    ),
    rule(
        &["library", "application support", "microsoft edge"],
        Protected::BrowserData,
    ),
    rule(&["library", "application support", "arc"], Protected::BrowserData),
    rule(&["library", "application support", "firefox"], Protected::BrowserData),
    rule(&[".config", "google-chrome"], Protected::BrowserData),
    rule(&[".config", "chromium"], Protected::BrowserData),
    rule(&[".config", "bravesoftware"], Protected::BrowserData),
    rule(&[".config", "microsoft-edge"], Protected::BrowserData),
];

/// Folded, matched against the last component only: a `.env` FOLDER is a Python venv.
const CREDENTIAL_FILE_NAMES: &[&str] = &[
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".pgpass",
    ".vault-token",
    "rclone.conf",
];

/// Any component starting with this is a Cmdr data dir: every instance, so a dev build can't
/// read the prod `mcp.token`. A custom `CMDR_DATA_DIR` is [`path_gate`]'s job.
const CMDR_COMPONENT_PREFIX: &str = "com.veszelovszki.cmdr";

fn fold(component: &str) -> String {
    component.nfc().collect::<String>().to_lowercase()
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

/// Which rule protects `path`, if any. Pure: no filesystem, so it can't see a symlink.
pub(crate) fn classify(path: &str) -> Option<Protected> {
    let parts = folded_components(path);
    if parts.iter().any(|part| part.starts_with(CMDR_COMPONENT_PREFIX)) {
        return Some(Protected::CmdrData);
    }
    let hit = RULES.iter().find(|rule| {
        parts
            .windows(rule.parts.len())
            .any(|run| run.iter().map(String::as_str).eq(rule.parts.iter().copied()))
    });
    if let Some(rule) = hit {
        return Some(rule.reason);
    }
    let name = parts.last()?;
    (name.starts_with(".env") || CREDENTIAL_FILE_NAMES.contains(&name.as_str())).then_some(Protected::Credentials)
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

fn gate_against(path: &str, data_dir: Option<&Path>) -> Result<(), Protected> {
    check_rules(path, data_dir)?;
    // A phone, a server, or a routed scheme has no realpath: its components were the whole check.
    if crate::mcp::is_virtual_path(path) || !Path::new(path).is_absolute() {
        return Ok(());
    }
    let Some(real) = resolve_existing(Path::new(path)) else {
        return Ok(());
    };
    check_rules(&real.to_string_lossy(), data_dir)?;
    // The data dir resolved too: `/tmp` and `/var` are links on a Mac, so a resolved path never
    // sits under the name a custom `CMDR_DATA_DIR` gives it.
    let real_data_dir = data_dir.and_then(resolve_existing);
    check_rules(&real.to_string_lossy(), real_data_dir.as_deref())
}

fn check_rules(path: &str, data_dir: Option<&Path>) -> Result<(), Protected> {
    if let Some(reason) = classify(path) {
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
#[path = "sensitive_tests.rs"]
mod tests;
