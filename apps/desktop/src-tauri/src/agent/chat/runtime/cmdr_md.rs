//! Where `CMDR.md` comes from, and how much of it a turn carries.
//!
//! `CMDR.md` is the user's own standing instructions to the agent, read into the stable prefix of
//! every turn. Two things are decided here: which copy of the file an app instance reads, and how
//! much of it the prompt is willing to pay for.

use std::io::Read;
use std::path::{Path, PathBuf};

/// The most of `CMDR.md` that is ever read off disk, as a guard on the read itself.
///
/// The system string is never elided (`context::assemble_prompt` tightens tool results only), so
/// every byte here is a permanent tax on every turn, and nothing else stops a hand-written file
/// from growing until it crowds the conversation out of the window.
const MAX_CMDR_MD_BYTES: usize = 64 * 1024;

/// What a cut file says about itself. Kept in step with [`MAX_CMDR_MD_BYTES`] by hand, since a
/// `const` string can't format one.
///
/// Cutting silently would leave the model reading a sentence that stops mid-thought and treating
/// it as the whole of what the user asked for.
const TRUNCATION_NOTE: &str = "\n\n[Cut off here: CMDR.md is larger than the 64 KB Cmdr reads.]";

/// Read the user's `CMDR.md` for the stable prefix. Absent, empty, unreadable, or not text →
/// `None`, and the prefix is then just the system prompt.
pub(super) fn read_cmdr_md() -> Option<String> {
    let path = cmdr_md_path(
        std::env::var("CMDR_DATA_DIR").ok().as_deref(),
        dirs::home_dir().as_deref(),
    )?;
    read_at(&path)
}

/// Which `CMDR.md` this instance reads: the one in an isolated data dir when there is one, else
/// the dotfile in home.
///
/// Production sets no `CMDR_DATA_DIR`, so it keeps reading `~/.cmdr/CMDR.md`, where a hand-edited,
/// dotfiles-repo-able config belongs. Only isolated instances (an E2E run, a worktree dev
/// instance) move, and that is the point: otherwise one developer's standing instructions ride
/// along in every automated run's prompt and in every worktree's turns.
///
/// Empty is unset, matching `config::data_dir_from_env`.
fn cmdr_md_path(data_dir_env: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match data_dir_env {
        Some(dir) if !dir.is_empty() => Some(Path::new(dir).join("CMDR.md")),
        _ => Some(home?.join(".cmdr").join("CMDR.md")),
    }
}

/// The file at `path`, capped, or `None` when there is nothing worth prefixing a turn with.
fn read_at(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    // One byte past the cap, so a file sitting exactly on it isn't reported as cut.
    file.take(MAX_CMDR_MD_BYTES as u64 + 1).read_to_end(&mut bytes).ok()?;

    let over_cap = bytes.len() > MAX_CMDR_MD_BYTES;
    if over_cap {
        bytes.truncate(MAX_CMDR_MD_BYTES);
        log::warn!(
            target: "agent::chat",
            "CMDR.md is over the {MAX_CMDR_MD_BYTES}-byte cap, so turns carry its beginning only"
        );
    }

    let mut text = match String::from_utf8(bytes) {
        Ok(text) => text,
        // A cut at the cap can land inside a multi-byte character, and giving up over that would
        // turn a large non-ASCII `CMDR.md` into no `CMDR.md` at all. Anything invalid further back
        // is a file that isn't text, and mojibake in the prompt is worse than nothing.
        Err(error) if over_cap && error.utf8_error().valid_up_to() + 4 > MAX_CMDR_MD_BYTES => {
            let valid = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).ok()?
        }
        Err(_) => {
            log::warn!(target: "agent::chat", "CMDR.md is not UTF-8 text, so no turn will carry it");
            return None;
        }
    };

    if text.trim().is_empty() {
        return None;
    }
    if over_cap {
        text.push_str(TRUNCATION_NOTE);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// An isolated environment (an E2E run, a worktree dev instance) sets `CMDR_DATA_DIR`, and its
    /// `CMDR.md` has to come from there. Reading the real `~/.cmdr/CMDR.md` would bleed one
    /// developer's hand-written instructions into every automated run's prompt.
    #[test]
    fn an_isolated_data_dir_supplies_the_file() {
        let path = cmdr_md_path(Some("/tmp/cmdr-e2e-data"), Some(Path::new("/Users/someone")));

        assert_eq!(path, Some(PathBuf::from("/tmp/cmdr-e2e-data/CMDR.md")));
    }

    /// Production sets no `CMDR_DATA_DIR`, so the file stays where a hand-edited,
    /// dotfiles-repo-able config belongs.
    #[test]
    fn without_the_env_var_it_stays_a_dotfile_in_home() {
        let path = cmdr_md_path(None, Some(Path::new("/Users/someone")));

        assert_eq!(path, Some(PathBuf::from("/Users/someone/.cmdr/CMDR.md")));
    }

    /// Empty is unset, matching `config::data_dir_from_env`: an empty var must not resolve the
    /// file against the current directory.
    #[test]
    fn an_empty_env_var_is_treated_as_unset() {
        let path = cmdr_md_path(Some(""), Some(Path::new("/Users/someone")));

        assert_eq!(path, Some(PathBuf::from("/Users/someone/.cmdr/CMDR.md")));
    }

    #[test]
    fn with_no_home_and_no_env_var_there_is_no_file_to_read() {
        assert_eq!(cmdr_md_path(None, None), None);
    }

    #[test]
    fn a_missing_file_reads_as_absent() {
        let dir = tempfile::tempdir().expect("temp dir");

        assert_eq!(read_at(&dir.path().join("CMDR.md")), None);
    }

    #[test]
    fn a_whitespace_only_file_reads_as_absent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("CMDR.md");
        std::fs::write(&path, "   \n\n\t").expect("write");

        assert_eq!(read_at(&path), None);
    }

    #[test]
    fn a_normal_file_is_read_whole() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("CMDR.md");
        std::fs::write(&path, "Call me David.").expect("write");

        assert_eq!(read_at(&path).as_deref(), Some("Call me David."));
    }

    /// The prefix is never elided, so every byte of this file is paid for on every turn, forever.
    /// A hand-written file that grew without anyone noticing must cost a bounded amount rather
    /// than quietly eating the window the conversation needs.
    #[test]
    fn an_oversized_file_is_cut_down_to_the_cap() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("CMDR.md");
        std::fs::write(&path, "x".repeat(MAX_CMDR_MD_BYTES + 5_000)).expect("write");

        let read = read_at(&path).expect("something is there");

        assert!(
            read.len() <= MAX_CMDR_MD_BYTES + TRUNCATION_NOTE.len(),
            "read {} bytes against a {MAX_CMDR_MD_BYTES}-byte cap",
            read.len()
        );
    }

    /// Cutting silently would leave the model reading a sentence that stops mid-thought and
    /// treating it as the whole of what the user said. Saying so costs one line.
    #[test]
    fn a_cut_file_says_it_was_cut() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("CMDR.md");
        std::fs::write(&path, "y".repeat(MAX_CMDR_MD_BYTES * 2)).expect("write");

        let read = read_at(&path).expect("something is there");

        assert!(read.ends_with(TRUNCATION_NOTE), "no truncation note in {read:.80}");
    }

    /// The cap is a byte count and the file is text, so the cut can land inside a multi-byte
    /// character. A `String` cannot hold that, so a naive cut turns a big non-ASCII file into no
    /// file at all.
    #[test]
    fn cutting_a_multibyte_file_still_produces_text() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("CMDR.md");
        // Three bytes each, so the cut lands mid-character unless the cap divides by three.
        std::fs::write(&path, "\u{2603}".repeat(MAX_CMDR_MD_BYTES)).expect("write");

        let read = read_at(&path).expect("a snowman-only file is still a file");

        assert!(read.starts_with('\u{2603}'), "the head survives the cut");
        assert!(read.ends_with(TRUNCATION_NOTE));
    }
}
