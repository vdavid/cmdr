//! Results (⌥⏎): the preview as a text file the user edits in their editor, and
//! the edited names read back.
//!
//! One line per row, `old name<TAB>new name`. A row is matched back by its OLD
//! name (composed, since an editor may recompose it), never by its line number,
//! so a file that appeared in the folder meanwhile can't shift a name onto the
//! wrong row. A line whose old name the file didn't hold, or that has no tab, is
//! skipped; the preview shows what came of each row.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
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
/// nothing behind.
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
        // Best effort: the file may be gone already (a data wipe, the user).
        if let Err(e) = std::fs::remove_file(&self.path)
            && e.kind() != io::ErrorKind::NotFound
        {
            log::debug!(
                target: "multi_rename::names_file",
                "couldn't remove {}: {e}",
                cmdr_fs::log_private::LogPrivate::file_path(&self.path)
            );
        }
    }
}

/// Writes `rows` to the Results file at `path`, in a folder only this user can open
/// ([`private_folder`]). A stale file at the name (one a crashed run left) is replaced; anything
/// else there, a symlink above all, is refused, so a planted link can't redirect the write.
pub fn write(path: &Path, rows: &[PreviewRow]) -> io::Result<Written> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the Results file has no folder"))?;
    private_folder(dir)?;
    sweep_orphans(dir);
    remove_stale(path)?;
    let mut file = create_new(path)?;
    // Built before the text goes in, so a failed write still deletes the file on drop.
    let written = Written {
        path: path.to_path_buf(),
        names: rows
            .iter()
            .filter(|row| row.status != RowStatus::Missing)
            .map(|row| (row.old_name.nfc().collect(), row.new_name.clone()))
            .collect(),
    };
    file.write_all(render(rows).as_bytes())?;
    Ok(written)
}

/// The Results file's text as the user saved it. Never follows a symlink at the name, and
/// refuses anything but a regular file: a named pipe there would hang the read.
pub fn read(path: &Path) -> io::Result<String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the Results file isn't a regular file",
        ));
    }
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

/// Makes `dir` with mode `0700`, or takes the one there only if it's a real folder (not a
/// symlink), owned by this user, and closed to everyone else. A looser one could be someone
/// else's, who could then plant links in it, swap in their own names, or read the user's.
#[cfg(unix)]
fn private_folder(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    // SAFETY: `geteuid` takes no arguments, shares no memory, and can't fail.
    let me = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != me || meta.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the Results folder isn't private to this user",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn private_folder(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

/// Removes the Results files of processes that are no longer running: a crash skips
/// `Written`'s drop, and no OS empties the cache dir. Only regular files named `<pid>-…` go;
/// a link, a folder, or a name without a pid stays. Best effort.
#[cfg(unix)]
fn sweep_orphans(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.split_once('-'))
            .map(|(pid, _)| pid)
            .filter(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
            .filter(|pid| *pid > 0)
        else {
            continue;
        };
        // `DirEntry::file_type` doesn't follow a link, so a link reads as one and stays.
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) || !process_is_gone(pid) {
            continue;
        }
        if let Err(e) = std::fs::remove_file(entry.path())
            && e.kind() != io::ErrorKind::NotFound
        {
            log::debug!(
                target: "multi_rename::names_file",
                "couldn't sweep {}: {e}",
                cmdr_fs::log_private::LogPrivate::file_path(&entry.path())
            );
        }
    }
}

#[cfg(not(unix))]
fn sweep_orphans(_dir: &Path) {}

/// Whether no process `pid` exists. `EPERM` (someone else's process) counts as running.
#[cfg(unix)]
fn process_is_gone(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 sends nothing; `kill` only checks that `pid` exists, and takes no pointers.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return false;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Removes a regular file at `path`, and refuses anything else there.
fn remove_stale(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => std::fs::remove_file(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "something other than a Results file sits at its name",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// A new `0600` file at `path`, failing if anything appeared there since [`remove_stale`].
fn create_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    options.open(path)
}

/// The Results folder's name inside the app's cache dir.
const RESULTS_DIR_NAME: &str = "multi-rename";

/// Where session `session_id` writes its Results file: `multi-rename/` in the app's cache dir
/// (`config::app_cache_dir`, so a dev or E2E instance keeps its own), which "Delete all Cmdr
/// data" removes whole. `None` when the OS has no cache dir. The process id keeps two Cmdrs on
/// one cache dir off each other's files: session ids count from 1 in each.
pub fn path_for(session_id: &str) -> Option<PathBuf> {
    let name = format!("{}-{session_id}.txt", std::process::id());
    Some(crate::config::app_cache_dir()?.join(RESULTS_DIR_NAME).join(name))
}
