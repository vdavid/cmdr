//! Unique-name generation: the ` (N)` convention every write path dedups with.
//!
//! Pure naming, no conflict policy. [`numbered_name`] writes one candidate,
//! [`split_sequence`] reads a trailing sequence off a stem, and
//! [`NameCandidates`] walks the sequence. The three claim strategies differ only
//! in how they TEST a candidate: [`find_unique_name`] reserves it with
//! `O_CREAT|O_EXCL`, [`next_available_name`] only probes, and
//! [`create_unique_dir`] claims it with `mkdir(2)`. Whatever the filesystem
//! can't answer on its own — a name one operation has picked but not yet
//! written — is what [`ClaimedNames`] remembers.
//!
//! The volume namer (`transfer/volume/conflict.rs::find_unique_volume_name`)
//! and the clipboard-paste writer (`paste_clipboard.rs`) walk these same
//! candidates, so the numbering can't drift between backends.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::ignore_poison::IgnorePoison;

use super::validation::path_exists_or_is_symlink;

/// The destination names ONE operation has already handed out but whose bytes
/// haven't landed yet.
///
/// [`next_available_name`] and the volume namer only PROBE the destination, and
/// the caller writes later, so the filesystem can't answer "is this name spoken
/// for" on its own. Two sources from one ` (N)` family duplicated together
/// (`photo.jpg` and `photo (1).jpg`) otherwise both arrive at `photo (2).jpg`:
/// `photo.jpg` skips the taken `(1)`, `photo (1).jpg` continues its series into
/// the same `(2)`, and two requested copies become one.
///
/// Interior-mutable and shareable, because the volume engine's concurrent driver
/// resolves several top-level sources at once. Lives on the op state
/// (`state::WriteOperationState::claimed_names`), so the ledger is per operation
/// and both engines read the same one.
///
// DEFAULT-OK: an empty ledger is the truth about a fresh operation, which has
// handed out no names yet.
#[derive(Debug, Default)]
pub(crate) struct ClaimedNames(Mutex<HashSet<PathBuf>>);

impl ClaimedNames {
    /// Records `path` as spoken for, answering whether it was still free. The
    /// test and the record are one step, so two tasks racing on one name can't
    /// both be told yes.
    pub(crate) fn claim(&self, path: &Path) -> bool {
        self.0.lock_ignore_poison().insert(path.to_path_buf())
    }
}

/// Builds the `counter`-th candidate name under the shared ` (N)` dedup
/// convention: `counter == 0` is the bare `stem[.ext]`, `1..` appends ` (N)`
/// before the extension. This is the ONE place the convention is written:
/// `find_unique_name`, `next_available_name`, the volume namer
/// (`transfer/volume/conflict.rs::find_unique_volume_name`), and the
/// clipboard-paste writer all go through it, so the numbering paths can't drift.
pub(super) fn numbered_name(stem: &str, ext: Option<&str>, counter: u32) -> String {
    match (ext, counter) {
        (Some(e), 0) => format!("{stem}.{e}"),
        (None, 0) => stem.to_string(),
        (Some(e), n) => format!("{stem} ({n}).{e}"),
        (None, n) => format!("{stem} ({n})"),
    }
}

/// Reads a trailing ` (N)` off a file stem so a search *continues* the series
/// rather than nesting inside it: duplicating `photo (1).jpg` gives
/// `photo (2).jpg`, never `photo (1) (1).jpg`. Returns the base to number from
/// and the first counter to try.
///
/// Pure, and the only rule for what a sequence IS. Searches reach it through
/// [`NameCandidates`] rather than calling it directly.
///
/// What counts as a sequence is deliberately narrow, because everything else is
/// somebody's filename:
/// - The separating space is required, and the digits must be ASCII, so
///   `Report (final).pdf`, `photo(1).jpg`, and `photo (+1).jpg` are all plain text.
/// - Zero padding isn't a format to preserve: `photo (007)` continues at `(8)`.
/// - A number with no `u32` successor (too large to parse, or `u32::MAX`) is
///   plain text too, which also keeps the returned counter always advanceable.
pub(super) fn split_sequence(stem: &str) -> (&str, u32) {
    if let Some(without_close) = stem.strip_suffix(')')
        && let Some((base, digits)) = without_close.rsplit_once(" (")
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && let Ok(current) = digits.parse::<u32>()
        && let Some(next) = current.checked_add(1)
    {
        return (base, next);
    }
    (stem, 1)
}

/// The ` (N)` candidate sequence for a path: where to put the result, the base
/// to number from (any trailing sequence already split off by
/// [`split_sequence`]), and the counter to try next. Built per item KIND, since
/// a file's extension has to stay at the end and a directory has none.
///
/// This is the whole of what the ` (N)` searches share. Each walks the same
/// candidates and differs only in how it TESTS one: [`find_unique_name`]
/// reserves with `O_CREAT|O_EXCL` and has to keep advancing when it loses that
/// race, [`next_available_name`] only probes, and the volume namer
/// (`transfer/volume/conflict.rs::find_unique_volume_name`) does either
/// depending on the backend. A search loop can't be shared across that
/// difference, so this carries the candidates and the searches keep their loops.
pub(super) struct NameCandidates<'a> {
    parent: &'a Path,
    base: String,
    extension: Option<String>,
    counter: u32,
    /// Where `counter` started, so a search that bounds its own effort can count
    /// ATTEMPTS. A name that already ends in a high ` (N)` starts the sequence
    /// there, so the counter's absolute value says nothing about effort spent.
    start: u32,
}

impl<'a> NameCandidates<'a> {
    /// Candidates for a FILE: the trailing extension stays put and the ` (N)`
    /// goes in front of it, `photo.jpg` → `photo (1).jpg`.
    pub(super) fn for_file(path: &'a Path) -> Self {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
        let extension = path.extension().map(|s| s.to_string_lossy().to_string());
        Self::from_parts(path, stem.unwrap_or_default(), extension)
    }

    /// Candidates for a DIRECTORY, which has no extension: everything after a
    /// dot is part of its name, so the number goes at the END. `my.dir` →
    /// `my.dir (1)`, ❌ never `my (1).dir`; `backup.2024` and `v1.2.3` likewise.
    pub(super) fn for_directory(path: &'a Path) -> Self {
        let name = path.file_name().map(|s| s.to_string_lossy().to_string());
        Self::from_parts(path, name.unwrap_or_default(), None)
    }

    fn from_parts(path: &'a Path, stem: String, extension: Option<String>) -> Self {
        let (base, counter) = split_sequence(&stem);
        Self {
            parent: path.parent().unwrap_or(Path::new("")),
            base: base.to_string(),
            extension,
            counter,
            start: counter,
        }
    }

    /// The candidate to try right now.
    pub(super) fn current(&self) -> PathBuf {
        self.parent
            .join(numbered_name(&self.base, self.extension.as_deref(), self.counter))
    }

    /// Moves past a candidate the caller found taken.
    pub(super) fn advance(&mut self) {
        self.counter = self.counter.saturating_add(1);
    }

    /// How many candidates this search has already rejected.
    pub(super) fn attempts(&self) -> u32 {
        self.counter.saturating_sub(self.start)
    }
}

/// Finds a unique filename by appending " (1)", " (2)", etc., **atomically
/// reserving** the chosen name via `O_CREAT|O_EXCL` so a concurrent process
/// (backup tool, cloud-sync agent, second Cmdr op) can't land a file at the
/// same path between our pick and the caller's write.
///
/// Pre-fix this returned the first non-existing candidate after an
/// `if !new_path.exists()` check; the caller then copied or renamed to that
/// path, leaving a ~ms TOCTOU window during which a concurrent write could
/// land an unrelated file at the same name — silently clobbered the next time
/// our copy / rename hit the path. By creating an empty placeholder under the
/// reserved name and letting downstream operations (`fs::copy` truncates;
/// `fs::rename` atomically replaces; `copyfile(3)` / `copy_file_range(2)` open
/// the dest with create+truncate) overwrite it, the race window collapses to
/// microseconds. Callers never observe the placeholder.
///
/// On the rare loss-of-the-placeholder edge case (a third party deletes our
/// empty file before the caller writes), the caller's write still succeeds
/// (creating fresh).
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
    let mut candidates = NameCandidates::for_file(path);

    loop {
        let new_path = candidates.current();

        match fs::OpenOptions::new().write(true).create_new(true).open(&new_path) {
            Ok(_) => return new_path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidates.advance();
            }
            Err(_) => {
                // Anything else (parent unwritable, ENOSPC, …) leaks back to
                // the caller's write attempt, which has its own error path.
                // Mirror the pre-fix behaviour of returning the path so the
                // caller surfaces the real error against the right operation.
                return new_path;
            }
        }
    }
}

/// Picks the next free ` (N)` name for a FILE **without reserving it** — same
/// convention and same sequence rule as [`find_unique_name`], but a probe and no
/// create. A directory goes through [`create_unique_dir`], which numbers its
/// name whole.
///
/// For callers that reserve the name themselves in a way an `O_CREAT|O_EXCL`
/// *file* placeholder would get in the way of: a directory claims its name with
/// `create_dir`, and a copy that lands through the ordinary non-overwrite write
/// path would otherwise find `find_unique_name`'s placeholder sitting at the
/// destination and raise a conflict against it.
///
/// Occupancy uses `path_exists_or_is_symlink`, so a dangling symlink counts as
/// taken; handing that name back would let the caller's write follow the
/// symlink to wherever it points. A name the operation already handed out counts
/// as taken too, and the pick is recorded in `claimed` before it's returned:
/// nothing else stands between two sources of one ` (N)` family and one shared
/// destination. See [`ClaimedNames`].
pub(super) fn next_available_name(path: &Path, claimed: &ClaimedNames) -> PathBuf {
    let mut candidates = NameCandidates::for_file(path);

    loop {
        let new_path = candidates.current();
        if !path_exists_or_is_symlink(&new_path) && claimed.claim(&new_path) {
            return new_path;
        }
        candidates.advance();
    }
}

/// Claims the next free ` (N)` name as a DIRECTORY, creating it. The `create_dir`
/// loop IS the reservation: `mkdir(2)` fails `AlreadyExists` on a taken name, so
/// advancing on that error is the directory analogue of [`find_unique_name`]'s
/// `O_CREAT|O_EXCL` file placeholder.
///
/// Separate from [`find_unique_name`] because a placeholder FILE is precisely
/// what a directory destination can't have sitting at its name. The caller
/// records the returned path so rollback can remove it.
///
/// `mkdir(2)` can't see a name an earlier [`next_available_name`] pick has only
/// spoken for, so this walks past those too and records its own claim
/// ([`ClaimedNames`]).
pub(super) fn create_unique_dir(path: &Path, claimed: &ClaimedNames) -> std::io::Result<PathBuf> {
    let mut candidates = NameCandidates::for_directory(path);

    loop {
        let new_path = candidates.current();
        if !claimed.claim(&new_path) {
            candidates.advance();
            continue;
        }
        match fs::create_dir(&new_path) {
            Ok(()) => return Ok(new_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => candidates.advance(),
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
#[path = "unique_name_tests.rs"]
mod tests;
