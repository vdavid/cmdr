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
    let path = dir.join("names.txt");
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
    let path = dir.join("names.txt");
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
    let path = dir.join("names.txt");
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
    let path = dir.join("names.txt");
    let decomposed = "Z\u{030C}adost.pdf";
    let written = write(&path, &[row(decomposed, decomposed)]).expect("writes");
    // An editor that recomposes the old name on save.
    std::fs::write(&path, "\u{017D}adost.pdf\tZadost.pdf\n").expect("saves");

    let edits = written.merge(&std::fs::read_to_string(&path).expect("reads"), &NameEdits::default());

    assert_eq!(edits.get(decomposed), Some("Zadost.pdf"));
}
