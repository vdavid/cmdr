//! The Results file: what it holds, and reading the user's edits back.

use super::names_file::{NameEdit, parse, render, write};
use super::plan::{NameEdits, PreviewRow, RowStatus};
use crate::test_support::TestDir;

fn row(old: &str, new: &str) -> PreviewRow {
    PreviewRow {
        row: 0,
        old_name: old.to_string(),
        new_name: new.to_string(),
        status: RowStatus::Ready,
        icon_id: None,
        is_directory: false,
        edited: false,
    }
}

fn edit(old: &str, new: &str) -> NameEdit {
    NameEdit {
        old_name: old.to_string(),
        new_name: new.to_string(),
    }
}

#[test]
fn one_line_per_row_old_then_new() {
    assert_eq!(
        render(&[row("Žádost.pdf", "Zadost.pdf"), row("b.txt", "b.txt")]),
        "Žádost.pdf\tZadost.pdf\nb.txt\tb.txt\n"
    );
}

#[test]
fn a_file_that_is_gone_gets_no_line() {
    let gone = PreviewRow {
        status: RowStatus::Missing,
        ..row("gone.txt", "gone.txt")
    };
    assert_eq!(render(&[gone, row("a.txt", "b.txt")]), "a.txt\tb.txt\n");
}

#[test]
fn reading_back_is_the_render_inverted() {
    let rows = [row("Žádost.pdf", "Zadost.pdf"), row("with space.txt", "x.txt")];
    assert_eq!(
        parse(&render(&rows)),
        vec![edit("Žádost.pdf", "Zadost.pdf"), edit("with space.txt", "x.txt")]
    );
}

#[test]
fn an_editor_s_line_endings_and_blanks_are_tolerated() {
    assert_eq!(
        parse("a.txt\t  new a.txt  \r\nb.txt\tb2.txt\r\n\n"),
        vec![edit("a.txt", "new a.txt"), edit("b.txt", "b2.txt")]
    );
}

#[test]
fn the_new_name_is_after_the_last_tab_and_a_line_without_one_is_skipped() {
    assert_eq!(
        parse("odd\tname.txt\tfixed.txt\nno tab here\n\tno old name\n"),
        vec![edit("odd\tname.txt", "fixed.txt")]
    );
}

#[test]
fn write_puts_the_rows_in_a_text_file() {
    let dir = TestDir::new("multi-rename-names-file");
    let path = dir.join("results").join("names.txt");
    let written = write(&path, &[row("a.txt", "b.txt")]).expect("the scratch dir is writable");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads"),
        "a.txt\tb.txt\n"
    );

    drop(written);
    assert!(!path.exists(), "the file goes with its session");
}

#[test]
fn only_a_line_the_user_changed_becomes_an_edit() {
    let dir = TestDir::new("multi-rename-names-changed");
    let path = dir.join("results").join("names.txt");
    let written = write(&path, &[row("a.txt", "a-1.txt"), row("b.txt", "b-1.txt")]).expect("writes");
    std::fs::write(&path, "a.txt\ta-1.txt\nb.txt\tbee.txt\n").expect("the editor saves");

    let edits = written.merge(&std::fs::read_to_string(&path).expect("reads"), &NameEdits::default());

    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits.get("a.txt"),
        None,
        "an untouched line keeps following the settings"
    );
    assert_eq!(edits.get("b.txt"), Some("bee.txt"));
}

#[test]
fn an_earlier_edit_stays_when_its_line_is_left_alone_and_a_stranger_s_line_is_ignored() {
    let dir = TestDir::new("multi-rename-names-merge");
    let path = dir.join("results").join("names.txt");
    let earlier = NameEdits::new(&[("a.txt", "ay.txt")]);
    let written = write(&path, &[row("a.txt", "ay.txt"), row("b.txt", "b-1.txt")]).expect("writes");
    std::fs::write(&path, "a.txt\tay.txt\nb.txt\tbee.txt\nnot-in-the-batch.txt\tx.txt\n").expect("saves");

    let edits = written.merge(&std::fs::read_to_string(&path).expect("reads"), &earlier);

    assert_eq!(edits.len(), 2);
    assert_eq!(edits.get("a.txt"), Some("ay.txt"));
    assert_eq!(edits.get("b.txt"), Some("bee.txt"));
    assert_eq!(
        edits.get("not-in-the-batch.txt"),
        None,
        "only the file's own rows count"
    );
}

#[test]
fn an_old_name_matches_in_either_unicode_form() {
    let dir = TestDir::new("multi-rename-names-nfc");
    let path = dir.join("results").join("names.txt");
    let decomposed = "Z\u{030C}adost.pdf";
    let written = write(&path, &[row(decomposed, decomposed)]).expect("writes");
    // An editor that recomposes the old name on save.
    std::fs::write(&path, "\u{017D}adost.pdf\tZadost.pdf\n").expect("saves");

    let edits = written.merge(&std::fs::read_to_string(&path).expect("reads"), &NameEdits::default());

    assert_eq!(edits.get(decomposed), Some("Zadost.pdf"));
}

/// The Results file lives in the app's cache dir, which "Delete all Cmdr data" removes whole
/// (`data_wipe::plan`): moving it out would let it escape the wipe.
#[test]
fn the_results_file_goes_with_the_cache_dir() {
    let cache = crate::config::app_cache_dir().expect("a test has a cache dir");
    let path = super::names_file::path_for("multi-rename-1").expect("a test has a cache dir");
    assert!(
        path.starts_with(&cache),
        "{} is outside {}",
        path.display(),
        cache.display()
    );
}

/// Issue #546: the Results folder and file are the user's alone, and nothing planted at their
/// names is followed or overwritten.
#[cfg(unix)]
mod private_folder {
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    use super::super::names_file::read;
    use super::*;

    fn private_dir(dir: &TestDir) -> PathBuf {
        let results = dir.join("results");
        std::fs::create_dir(&results).expect("creates");
        std::fs::set_permissions(&results, std::fs::Permissions::from_mode(0o700)).expect("chmods");
        results
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path).expect("stats").permissions().mode() & 0o777
    }

    #[test]
    fn write_makes_a_folder_and_a_file_only_the_user_can_open() {
        let dir = TestDir::new("multi-rename-names-private");
        let path = dir.join("results").join("names.txt");
        let _written = write(&path, &[row("a.txt", "b.txt")]).expect("writes");

        assert_eq!(mode(&dir.join("results")), 0o700);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn write_refuses_a_symlink_at_the_name_and_leaves_its_target_alone() {
        let dir = TestDir::new("multi-rename-names-symlink");
        let results = private_dir(&dir);
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "precious").expect("writes");
        let path = results.join("names.txt");
        symlink(&victim, &path).expect("links");

        assert!(write(&path, &[row("a.txt", "b.txt")]).is_err());
        assert_eq!(std::fs::read_to_string(&victim).expect("reads"), "precious");
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("stats")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn write_refuses_a_folder_others_can_write_into() {
        let dir = TestDir::new("multi-rename-names-loose");
        let results = private_dir(&dir);
        std::fs::set_permissions(&results, std::fs::Permissions::from_mode(0o777)).expect("chmods");
        let path = results.join("names.txt");

        assert!(write(&path, &[row("a.txt", "b.txt")]).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn write_refuses_a_symlinked_folder() {
        let dir = TestDir::new("multi-rename-names-linked-dir");
        let real = private_dir(&dir);
        let linked = dir.join("linked");
        symlink(&real, &linked).expect("links");

        assert!(write(&linked.join("names.txt"), &[row("a.txt", "b.txt")]).is_err());
        assert!(!real.join("names.txt").exists());
    }

    /// A crashed run's files stay in the cache dir (no OS cleans it), so a write sweeps the ones
    /// whose process is gone. A live process's file, a link, and a name that isn't ours stay.
    #[test]
    fn write_sweeps_files_of_processes_that_are_gone() {
        let dir = TestDir::new("multi-rename-names-sweep");
        let results = private_dir(&dir);
        let mut child = std::process::Command::new("true").spawn().expect("spawns");
        let dead_pid = child.id();
        child.wait().expect("waits");
        let dead = results.join(format!("{dead_pid}-multi-rename-1.txt"));
        std::fs::write(&dead, "a.txt\tb.txt\n").expect("writes");
        let live = results.join(format!("{}-multi-rename-9.txt", std::process::id()));
        std::fs::write(&live, "a.txt\tb.txt\n").expect("writes");
        let unparsed = results.join("notes.txt");
        std::fs::write(&unparsed, "mine").expect("writes");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "precious").expect("writes");
        let dead_link = results.join(format!("{dead_pid}-multi-rename-2.txt"));
        symlink(&victim, &dead_link).expect("links");

        let _written = write(&results.join("names.txt"), &[row("a.txt", "b.txt")]).expect("writes");

        assert!(!dead.exists(), "a gone process's file is swept");
        assert!(live.exists(), "a running process's file stays");
        assert!(unparsed.exists(), "a name without a pid stays");
        assert!(std::fs::symlink_metadata(&dead_link).is_ok(), "a link is never touched");
        assert_eq!(std::fs::read_to_string(&victim).expect("reads"), "precious");
    }

    #[test]
    fn a_stale_file_at_the_name_is_replaced() {
        let dir = TestDir::new("multi-rename-names-stale");
        let path = private_dir(&dir).join("names.txt");
        std::fs::write(&path, "old.txt\told.txt\n").expect("writes");

        let _written = write(&path, &[row("a.txt", "b.txt")]).expect("writes");

        assert_eq!(std::fs::read_to_string(&path).expect("reads"), "a.txt\tb.txt\n");
    }

    #[test]
    fn read_refuses_a_symlink_or_a_named_pipe_at_the_name() {
        let dir = TestDir::new("multi-rename-names-read");
        let results = private_dir(&dir);
        let elsewhere = dir.join("elsewhere.txt");
        std::fs::write(&elsewhere, "a.txt\tb.txt\n").expect("writes");
        let link = results.join("link.txt");
        symlink(&elsewhere, &link).expect("links");
        let pipe = results.join("pipe.txt");
        let c_pipe = std::ffi::CString::new(pipe.as_os_str().as_encoded_bytes()).expect("no NUL");
        // SAFETY: (test) `c_pipe` is a valid NUL-terminated path that outlives the call.
        assert_eq!(unsafe { libc::mkfifo(c_pipe.as_ptr(), 0o600) }, 0);

        assert!(read(&link).is_err());
        assert!(read(&pipe).is_err(), "and doesn't hang waiting for a writer");
    }

    #[test]
    fn read_gives_back_what_the_user_saved() {
        let dir = TestDir::new("multi-rename-names-read-back");
        let path = dir.join("results").join("names.txt");
        let _written = write(&path, &[row("a.txt", "b.txt")]).expect("writes");

        assert_eq!(read(&path).expect("reads"), "a.txt\tb.txt\n");
    }
}
