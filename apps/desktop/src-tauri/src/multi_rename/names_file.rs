//! Results (⌥⏎): the preview as a text file the user edits in their editor, and
//! the edited names read back.
//!
//! One line per row, `old name<TAB>new name`. A row is matched back by its OLD
//! name (composed, since an editor may recompose it), never by its line number,
//! so a file that appeared in the folder meanwhile can't shift a name onto the
//! wrong row. A line whose old name the file didn't hold, or that has no tab, is
//! skipped; the preview shows what came of each row.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use super::plan::{NameEdits, PreviewRow, RowStatus};

/// One line of the file: a row's name in the folder, and the name it should get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEdit {
    pub old_name: String,
    pub new_name: String,
}

/// The file's text for `rows`. A file that's gone gets no line: there's nothing to rename.
pub fn render(rows: &[PreviewRow]) -> String {
    let mut text = String::new();
    for row in rows.iter().filter(|row| row.status != RowStatus::Missing) {
        text.push_str(&row.old_name);
        text.push('\t');
        text.push_str(&row.new_name);
        text.push('\n');
    }
    text
}

/// The lines in `text`. The new name is everything after a line's LAST tab, so
/// an old name holding a tab still splits right; surrounding blanks the editor
/// added are trimmed off the new name.
pub fn parse(text: &str) -> Vec<NameEdit> {
    text.lines()
        .filter_map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let (old, new) = line.rsplit_once('\t')?;
            (!old.is_empty()).then(|| NameEdit {
                old_name: old.to_string(),
                new_name: new.trim().to_string(),
            })
        })
        .collect()
}

/// A Results file as written: where it is, and each row's new name in it, by
/// composed old name, so reading back can tell which lines the user changed.
/// Dropping it deletes the file, so a closed, evicted, or cleared session leaves
/// nothing behind in the temp folder.
#[derive(Debug)]
pub struct Written {
    pub path: PathBuf,
    names: HashMap<String, String>,
}

impl Written {
    /// `current` with the lines of `text` (the file as the user saved it) that
    /// changed since the write laid over it. A line left as written keeps whatever
    /// its row had, so an untouched row keeps following the settings and an earlier
    /// edit stays. Only the file's own rows count.
    pub fn merge(&self, text: &str, current: &NameEdits) -> NameEdits {
        let mut edits = current.clone();
        for line in parse(text) {
            let key: String = line.old_name.nfc().collect();
            match self.names.get(&key) {
                Some(written) if *written != line.new_name => edits.insert(key, line.new_name),
                _ => {}
            }
        }
        edits
    }
}

impl Drop for Written {
    fn drop(&mut self) {
        // Best effort: the temp folder is the OS's to clean, and it may be gone already.
        if let Err(e) = std::fs::remove_file(&self.path)
            && e.kind() != io::ErrorKind::NotFound
        {
            log::debug!(target: "multi_rename::names_file", "couldn't remove {}: {e}", self.path.display());
        }
    }
}

/// Writes `rows` to the Results file at `path`.
pub fn write(path: &Path, rows: &[PreviewRow]) -> io::Result<Written> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, render(rows))?;
    Ok(Written {
        path: path.to_path_buf(),
        names: rows
            .iter()
            .filter(|row| row.status != RowStatus::Missing)
            .map(|row| (row.old_name.nfc().collect(), row.new_name.clone()))
            .collect(),
    })
}

/// Where session `session_id` writes its Results file: the user's temp folder. The
/// process id keeps two running Cmdrs (a dev build beside the app, a test process
/// beside another) off each other's files: session ids count from 1 in each.
pub fn path_for(session_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("cmdr-multi-rename")
        .join(format!("{}-{session_id}.txt", std::process::id()))
}
