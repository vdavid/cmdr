//! What counts as a name's extension: the ONE rule the whole product uses.
//!
//! In short: a plain folder has none; a macOS package's (`Cmdr.app`) is what
//! follows its last dot; a file's is 1–10 ASCII letters and digits after the
//! last dot, or a known compressed tar (`tar.gz`); a leading or trailing dot
//! isn't one. Which of the three a name is comes from its entry ([`NameKind`]).
//! The rule, who asks it, the TypeScript mirror, and what deliberately doesn't
//! use it: `crates/cmdr-fs/DETAILS.md` § "What counts as an extension".

mod platform;

/// Folder suffixes that make macOS present the folder as one opaque item: the
/// free, I/O-less fast path of package detection, and the product's ONLY copy
/// of the list. A dotted folder this doesn't know can still be a package; the
/// app asks macOS about the visible ones (`enrich_packages`). Only macOS reads
/// names this way ([`folder_names_make_packages`]). Lowercase, no dot.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "component",
    "framework",
    "kext",
    "mdimporter",
    "photoslibrary",
    "pkg",
    "plugin",
    "prefpane",
    "qlgenerator",
    "rtfd",
    "saver",
    "wdgt",
    "xcodeproj",
    "xcworkspace",
];

/// Two-part extensions that count as one: the compressed tars whose
/// `ArchiveFormat::label` carries a dot. Lowercase, no leading dot.
pub const COMPOUND_EXTENSIONS: &[&str] = &["tar.gz", "tar.bz2", "tar.xz", "tar.zst"];

/// Longest plain extension, in ASCII characters.
const MAX_EXTENSION_LEN: usize = 10;

/// What a name belongs to, which decides what its extension can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// A file (or a symlink to one): 1–10 alphanumerics, or a compound.
    File,
    /// A plain folder: a dot in its name is part of the name.
    Folder,
    /// A macOS package (`Cmdr.app`): everything after the last dot.
    Package,
}

impl NameKind {
    /// The kind of an entry whose packageness is already known
    /// (`FileEntry::is_package`).
    pub fn of(is_directory: bool, is_package: bool) -> Self {
        match (is_directory, is_package) {
            (false, _) => Self::File,
            (true, true) => Self::Package,
            (true, false) => Self::Folder,
        }
    }

    /// The kind read off the name alone, through [`PACKAGE_EXTENSIONS`], for a
    /// caller with no entry to ask.
    pub fn by_name(name: &str, is_directory: bool) -> Self {
        Self::of(is_directory, is_directory && is_listed_package(name))
    }
}

/// Whether this OS reads a folder's name as a package at all: macOS (and the
/// browser demo, which shows the macOS app's listing). Elsewhere a folder named
/// `X.app` is a folder: no `pkg:` icon, no extension, no Enter policy of its own.
pub const fn folder_names_make_packages() -> bool {
    platform::FOLDER_NAMES_MAKE_PACKAGES
}

/// Whether a folder named `name` is a package by [`PACKAGE_EXTENSIONS`] on this
/// OS (case-insensitive, needs a stem). Pure, so it runs for every listed entry.
pub fn is_listed_package(name: &str) -> bool {
    folder_names_make_packages() && has_package_suffix(name)
}

/// Whether `name` ends in a [`PACKAGE_EXTENSIONS`] suffix (case-insensitive,
/// needs a stem), whatever this OS makes of it.
pub fn has_package_suffix(name: &str) -> bool {
    last_segment(name).is_some_and(|ext| PACKAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// Whether only macOS can say if a folder named `name` is a package: it has a
/// dotted suffix the list doesn't know. A name with no such suffix isn't asked.
/// Only the macOS app asks, so this reads the name alone.
pub fn needs_package_lookup(name: &str) -> bool {
    last_segment(name).is_some_and(|ext| !ext.is_empty()) && !has_package_suffix(name)
}

/// The extension of `name`, without its dot and in the name's own case, or
/// `None` when it has none under the product's rule (see the module docs).
pub fn extension_of(name: &str, kind: NameKind) -> Option<&str> {
    match kind {
        NameKind::Folder => None,
        NameKind::Package => last_segment(name).filter(|ext| !ext.is_empty()),
        NameKind::File => compound_extension(name).or_else(|| plain_extension(name)),
    }
}

/// `name` with its extension and the dot before it stripped, or the whole name
/// when it has none.
pub fn name_without_extension(name: &str, kind: NameKind) -> &str {
    match extension_of(name, kind) {
        Some(ext) => &name[..name.len() - ext.len() - 1],
        None => name,
    }
}

/// The part after the last dot, when there's a stem before it.
fn last_segment(name: &str) -> Option<&str> {
    let dot = name.rfind('.').filter(|&dot| dot > 0)?;
    Some(&name[dot + 1..])
}

fn plain_extension(name: &str) -> Option<&str> {
    last_segment(name)
        .filter(|ext| (1..=MAX_EXTENSION_LEN).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn compound_extension(name: &str) -> Option<&str> {
    COMPOUND_EXTENSIONS.iter().find_map(|compound| {
        // A real stem before the dot, so `.tar.gz` alone is a dotfile's `gz`.
        let dot = name.len().checked_sub(compound.len() + 1).filter(|&dot| dot > 0)?;
        let tail = name.get(dot..)?;
        let ext = tail.strip_prefix('.')?;
        ext.eq_ignore_ascii_case(compound).then_some(ext)
    })
}

#[cfg(test)]
#[path = "file_extension_test.rs"]
mod tests;
