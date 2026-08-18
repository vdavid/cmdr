//! Which destinations a group may be built with.
//!
//! The executor fences an archive-changeset route against a bound transfer, which is the
//! right backstop and the wrong layer on its own: it fires AFTER the user approved, so a
//! user-started copy-into-zip would work while an approved one refused. The constraint
//! belongs here, where a refusal costs the agent a retry and costs the user nothing.

use super::super::*;
use crate::location::Location;

fn at(path: &str) -> Location {
    Location {
        volume_id: "root".to_string(),
        path: path.to_string(),
    }
}

#[test]
fn an_ordinary_folder_is_a_writable_destination() {
    assert!(WritableDestination::new(at("/Users/someone/Documents/Invoices")).is_some());
}

/// The compress target IS an archive, and that is the point of compress. Only a path that
/// continues INSIDE one is refused.
#[test]
fn the_archive_file_itself_is_a_writable_destination() {
    assert!(WritableDestination::new(at("/Users/someone/Desktop/backup.zip")).is_some());
}

#[test]
fn a_path_inside_an_archive_cannot_be_a_destination() {
    for inside in [
        "/Users/someone/Desktop/backup.zip/invoices",
        "/Users/someone/Desktop/backup.zip/nested/deeper",
        "/Users/someone/Desktop/archive.tar.gz/inside",
    ] {
        assert!(
            WritableDestination::new(at(inside)).is_none(),
            "{inside} continues inside an archive, so no group may write there"
        );
    }
}

/// A folder that merely has an archive-ish name somewhere ABOVE it is still refused, because
/// the routing splits on the first archive component whatever follows it. The check and the
/// routing must agree, or the fence fires on a group the proposal layer allowed.
#[test]
fn the_constraint_matches_where_the_routing_splits() {
    assert!(WritableDestination::new(at("/Users/someone/backup.zip/a/b/c")).is_none());
}
