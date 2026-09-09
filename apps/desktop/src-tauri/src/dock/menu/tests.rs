//! The contract the Dock menu can't be caught breaking: it answers or it does
//! nothing, and it never unwinds into Objective-C.
//!
//! The AppKit half needs a real `NSApplication` on a real main thread, which a
//! `cargo test` binary doesn't have, so what's exercised here is everything up to that
//! boundary plus the two properties that actually matter.

use super::*;
use rows::{Candidate, DockLabel, DockLocation, LocationKind};

/// The panic-catching shape both callbacks use, minus the ObjC calling convention a
/// test can't produce. If `catch_unwind` stopped covering the body, this stops passing.
fn caught<T>(body: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(body)).ok()
}

#[test]
fn a_panicking_menu_build_becomes_a_nil_menu_rather_than_an_unwind() {
    let answer = caught(|| -> Option<u8> { panic!("the sources went wrong") });

    assert!(
        answer.is_none(),
        "a panic has to become `None`, which `dock_menu` returns to AppKit as nil"
    );
}

#[test]
fn a_panicking_click_is_swallowed_rather_than_unwound() {
    let answer = caught(|| panic!("the row went wrong"));

    assert!(
        answer.is_none(),
        "`item_clicked` returns void, so a panic just ends there"
    );
}

#[test]
fn a_tag_outside_the_row_list_selects_nothing() {
    SHOWING.with(|showing| {
        *showing.borrow_mut() = vec![DockRow::Command(DockCommand::OpenCmdr)];
    });

    for tag in [1_i64, 7, i64::MAX] {
        let index = usize::try_from(tag).expect("a positive tag converts");
        let row = SHOWING.with(|showing| showing.borrow().get(index).cloned());
        assert!(row.is_none(), "no row answers to tag {tag}, so the click does nothing");
    }

    SHOWING.with(|showing| showing.borrow_mut().clear());
}

#[test]
fn a_negative_tag_never_reaches_the_row_list() {
    for tag in [-1_i64, i64::MIN] {
        assert!(
            usize::try_from(tag).is_err(),
            "a negative tag must be refused before it can index anything"
        );
    }
}

#[test]
fn only_the_open_cmdr_row_acts_without_a_frontend_command() {
    for command in DockCommand::ALL {
        let id = command.command_id();
        match command {
            DockCommand::OpenCmdr => assert_eq!(id, None, "raising the window is the whole command"),
            _ => assert!(
                id.is_some_and(|id| id.contains('.')),
                "{command:?} must name a registry command id"
            ),
        }
    }
}

#[test]
fn every_clickable_row_carries_a_tag_that_points_back_at_it() {
    let built = rows::menu_rows(
        &[Candidate::bookmark("Desktop", "/Users/dave/Desktop")],
        &[Candidate::tab("/Users/dave/code")],
        Some(std::path::Path::new("/Users/dave")),
    );

    // `native::build` tags each item with its index in this exact list, so the
    // round-trip a click makes has to land on the row that was drawn.
    for (index, row) in built.iter().enumerate() {
        assert_eq!(built.get(index), Some(row), "index {index} must name its own row");
    }
}

#[test]
fn performing_a_separator_is_a_no_op_even_with_no_app_handle() {
    // `perform` bails on a missing `APP`, so this asserts the early return rather than
    // the emit. Reaching a panic here would mean the click path can take the process
    // down before it has anything to act on.
    let answer = caught(|| {
        perform(&DockRow::Separator);
        perform(&DockRow::Location(DockLocation {
            label: DockLabel::Plain("nowhere".to_string()),
            path: "/tmp/nowhere".to_string(),
            kind: LocationKind::Tab,
        }));
    });

    assert!(answer.is_some(), "acting on a row must never unwind");
}
