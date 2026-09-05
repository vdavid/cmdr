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
            // allowed-pluralize-noun: "continues" is the verb here, not a plural noun.
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

/// A repo's virtual `.git` trees take no writes either, and for the same reason:
/// there is nothing on disk to write to. Refusing here costs the agent a retry;
/// refusing after approval costs the user a decision they made for nothing.
#[test]
fn a_path_inside_a_repos_history_cannot_be_a_destination() {
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);
    for inside in [
        "/Users/someone/code/cmdr/.git/branches",
        "/Users/someone/code/cmdr/.git/branches/main/src",
        "/Users/someone/code/cmdr/.git/stash/0",
    ] {
        assert!(
            WritableDestination::new(at(inside)).is_none(),
            "{inside} is the portal's, so no group may write there"
        );
    }

    // The real entries under `.git/` are ordinary local files and stay writable.
    assert!(WritableDestination::new(at("/Users/someone/code/cmdr/.git")).is_some());
    assert!(WritableDestination::new(at("/Users/someone/code/cmdr/.git/hooks")).is_some());
}
