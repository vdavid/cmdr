//! Synthetic-home fixture generator for scoring against realistic trees.
//!
//! A builder over [`InMemoryVolume`] that constructs the kind of home directory
//! the scorer must rank well (agent-spec §15, §20.4): a Downloads full of mixed
//! junk, a `.git` project, a `node_modules`, a monoculture log folder, and a
//! Documents/invoices tree. It also derives a [`FolderSignals`] for any folder in
//! the tree, so a test can build one tree and assert the scorer's ranking over it
//! without a running app, a real volume, or Spotlight.
//!
//! This is TEST-SUPPORT code (a `cfg(test)` builder), not a production path: the
//! real signal-assembly-from-index lives in M2's scheduler. The two must agree on
//! what a signal means; this generator is the M1 stand-in that pins the formula's
//! expected behavior.

#![cfg(test)]

use super::classify::{is_denylisted, is_hidden_or_system, leaf_name, path_class};
use super::scorer::{FolderSignals, extension_count};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::InMemoryVolume;
use std::collections::BTreeSet;

/// A synthetic home tree plus the metadata needed to derive per-folder signals.
///
/// Holds every [`FileEntry`] in the tree (so signals derive without a listing
/// round-trip) alongside the set of project-root paths and the home root, which
/// [`Self::signals_for`] needs to classify a folder. [`Self::volume`] materializes
/// an [`InMemoryVolume`] on demand for tests that want to exercise the real
/// `Volume` listing surface against the same tree.
pub struct SyntheticHome {
    /// Every entry in the tree, in insertion order.
    entries: Vec<FileEntry>,
    /// The home root, e.g. `/Users/test`. Path-class priors are computed relative
    /// to it (Downloads/Desktop/Documents under it are `UserContent`).
    pub home: String,
    /// Directories that are project roots (a `.git`/marker sits in or under them).
    project_roots: BTreeSet<String>,
    /// The wall-clock "now" (Unix seconds) the tree's mtimes are relative to, so a
    /// test can pass the same value to the scorer for deterministic recency.
    pub now_secs: u64,
}

/// Seconds in a day, for readable mtime offsets.
const DAY: u64 = 24 * 60 * 60;

impl SyntheticHome {
    /// Builds the canonical synthetic home the scorer iterates against.
    ///
    /// Layout under `/Users/test`:
    /// - `Downloads/` — mixed junk: a `.pdf`, a `.dmg`, a `.zip`, a `.png`
    ///   (diverse, recent), the archetypal "matters" folder.
    /// - `projects/webapp/` — a project root: `package.json`, a `.git` child, a
    ///   `src/` with mixed source, and a `node_modules/` child.
    /// - `projects/webapp/node_modules/` — the archetypal near-floor folder:
    ///   denylisted name, hidden-ish, monoculture-ish.
    /// - `projects/webapp/.git/` — denylisted internals.
    /// - `logs/` — a monoculture: 200 `.log` files, one extension, stale.
    /// - `Documents/invoices/` — user content: mixed `.pdf`/`.xlsx`, moderately
    ///   recent.
    /// - `Library/Caches/` — system/cache, hidden-ish, old.
    pub fn canonical(now_secs: u64) -> Self {
        let mut project_roots = BTreeSet::new();

        let mut home = Self {
            entries: Vec::new(),
            home: "/Users/test".to_string(),
            project_roots: BTreeSet::new(),
            now_secs,
        };

        // Home root.
        let home_root = home.home.clone();
        home.add_dir(&home_root, now_secs - DAY);

        // Downloads: mixed, recent.
        let downloads = format!("{}/Downloads", home.home);
        home.add_dir(&downloads, now_secs - DAY);
        home.add_file(&format!("{downloads}/report.pdf"), now_secs - DAY);
        home.add_file(&format!("{downloads}/Installer.dmg"), now_secs - 2 * DAY);
        home.add_file(&format!("{downloads}/photos.zip"), now_secs - DAY);
        home.add_file(&format!("{downloads}/screenshot.png"), now_secs - DAY);

        // projects/webapp: a project root, recent, mixed source.
        let projects = format!("{}/projects", home.home);
        home.add_dir(&projects, now_secs - 3 * DAY);
        let webapp = format!("{projects}/webapp");
        home.add_dir(&webapp, now_secs - DAY);
        project_roots.insert(webapp.clone());
        home.add_file(&format!("{webapp}/package.json"), now_secs - DAY);
        home.add_file(&format!("{webapp}/README.md"), now_secs - 2 * DAY);

        let git = format!("{webapp}/.git");
        home.add_dir(&git, now_secs - DAY);
        home.add_file(&format!("{git}/HEAD"), now_secs - DAY);
        home.add_file(&format!("{git}/config"), now_secs - DAY);

        let src = format!("{webapp}/src");
        home.add_dir(&src, now_secs - DAY);
        home.add_file(&format!("{src}/main.ts"), now_secs - DAY);
        home.add_file(&format!("{src}/app.svelte"), now_secs - DAY);
        home.add_file(&format!("{src}/styles.css"), now_secs - DAY);

        let node_modules = format!("{webapp}/node_modules");
        home.add_dir(&node_modules, now_secs - DAY);
        for i in 0..50 {
            home.add_file(&format!("{node_modules}/index_{i}.js"), now_secs - DAY);
        }

        // logs: a monoculture, stale.
        let logs = format!("{}/logs", home.home);
        home.add_dir(&logs, now_secs - 90 * DAY);
        for i in 0..200 {
            home.add_file(&format!("{logs}/run_{i:04}.log"), now_secs - 90 * DAY);
        }

        // Documents/invoices: user content, mixed, moderately recent.
        let documents = format!("{}/Documents", home.home);
        home.add_dir(&documents, now_secs - 10 * DAY);
        let invoices = format!("{documents}/invoices");
        home.add_dir(&invoices, now_secs - 10 * DAY);
        home.add_file(&format!("{invoices}/january.pdf"), now_secs - 10 * DAY);
        home.add_file(&format!("{invoices}/january.xlsx"), now_secs - 10 * DAY);
        home.add_file(&format!("{invoices}/february.pdf"), now_secs - 12 * DAY);

        // Library/Caches: system/cache, old.
        let library = format!("{}/Library", home.home);
        home.add_dir(&library, now_secs - 40 * DAY);
        let caches = format!("{library}/Caches");
        home.add_dir(&caches, now_secs - 40 * DAY);
        home.add_file(&format!("{caches}/cache_0.bin"), now_secs - 40 * DAY);
        home.add_file(&format!("{caches}/cache_1.bin"), now_secs - 41 * DAY);

        home.project_roots = project_roots;
        home
    }

    /// Adds a directory entry with the given mtime.
    fn add_dir(&mut self, path: &str, mtime: u64) {
        let name = leaf_name(path);
        self.entries.push(FileEntry {
            modified_at: Some(mtime),
            ..FileEntry::new(name, path.to_string(), true, false)
        });
    }

    /// Adds a file entry with the given mtime.
    fn add_file(&mut self, path: &str, mtime: u64) {
        let name = leaf_name(path);
        self.entries.push(FileEntry {
            size: Some(1024),
            modified_at: Some(mtime),
            ..FileEntry::new(name, path.to_string(), false, false)
        });
    }

    /// Materializes an [`InMemoryVolume`] over the tree, for tests that want to
    /// exercise the real `Volume` listing surface.
    pub fn volume(&self) -> InMemoryVolume {
        InMemoryVolume::with_entries("synthetic-home", self.entries.clone())
    }

    /// Every entry in the tree, for a test that materializes a real drive index
    /// over the same tree (the scheduler's full-recompute integration test).
    pub fn all_entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// The direct children of `path` (entries whose parent is exactly `path`).
    pub fn direct_children(&self, path: &str) -> impl Iterator<Item = &FileEntry> {
        let prefix = format!("{path}/");
        self.entries.iter().filter(move |e| {
            // A direct child starts with `path/` and has no further `/` after it.
            e.path.strip_prefix(&prefix).is_some_and(|rest| !rest.contains('/'))
        })
    }

    /// Derives the [`FolderSignals`] the scorer consumes for the folder at `path`.
    ///
    /// Computes the file count and extension diversity from the folder's direct
    /// children, folds the leaf name against the M1 denylist, classifies the path
    /// relative to `home`, and checks whether a project marker sits in the folder
    /// or a descendant. Optional signals (`visit_count`, `last_used_secs`) stay
    /// `None` — M1 leaves them unwired.
    pub fn signals_for(&self, path: &str) -> FolderSignals {
        let files: Vec<&FileEntry> = self.direct_children(path).filter(|e| !e.is_directory).collect();
        let distinct_extension_count = extension_count(files.iter().map(|e| e.name.as_str()));

        let name = leaf_name(path);
        let name_denylisted = is_denylisted(&name);
        let hidden_or_system = is_hidden_or_system(path, &name, &self.home);

        let mtime_secs = self.entries.iter().find(|e| e.path == path).and_then(|e| e.modified_at);

        FolderSignals {
            name_denylisted,
            hidden_or_system,
            distinct_extension_count,
            file_count: files.len() as u32,
            mtime_secs,
            has_project_marker: self.has_marker_at_or_under(path),
            path_class: path_class(path, &self.home),
            visit_count: None,
            last_used_secs: None,
        }
    }

    /// Whether a project marker sits in `path` or any descendant of it.
    fn has_marker_at_or_under(&self, path: &str) -> bool {
        self.project_roots
            .iter()
            .any(|root| root == path || path.starts_with(&format!("{root}/")) || root.starts_with(&format!("{path}/")))
    }
}
