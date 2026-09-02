//! The `mkdir -p` walk, with a fake server counting its requests.
//!
//! ❗ The `Created` / `AlreadyExisted` cells are data-safety cells: the transfer
//! driver skips its per-file conflict probe on `Created`, so a wrong one is a
//! silent overwrite of every file in a copy.

use std::collections::HashSet;
use std::sync::Mutex;

use super::*;
use crate::ignore_poison::IgnorePoison;

/// How a fixed remote path refuses, for the arms that must stop the walk.
type Refusal = (String, fn(String) -> VolumeError);

/// A server whose directories are a set of remote paths.
struct FakeServer {
    existing: Mutex<HashSet<String>>,
    made: Mutex<Vec<String>>,
    refuses: Option<Refusal>,
}

impl FakeServer {
    fn new(existing: &[&str]) -> Self {
        Self {
            existing: Mutex::new(existing.iter().map(|d| (*d).to_string()).collect()),
            made: Mutex::new(Vec::new()),
            refuses: None,
        }
    }

    fn refusing(mut self, remote: &str, how: fn(String) -> VolumeError) -> Self {
        self.refuses = Some((remote.to_string(), how));
        self
    }

    fn requests(&self) -> Vec<String> {
        self.made.lock_ignore_poison().clone()
    }
}

impl MakesDirectories for FakeServer {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        Ok(path.to_string_lossy().into_owned())
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(async move {
            self.made.lock_ignore_poison().push(remote.to_string());
            if let Some((refused, how)) = &self.refuses
                && refused == remote
            {
                return Err(how(remote.to_string()));
            }
            let mut existing = self.existing.lock_ignore_poison();
            if existing.contains(remote) {
                return Err(VolumeError::AlreadyExists(remote.to_string()));
            }
            let parent = Path::new(remote).parent().map(|p| p.to_string_lossy().into_owned());
            match parent {
                Some(parent) if parent != "/" && !existing.contains(&parent) => {
                    Err(VolumeError::NotFound(remote.to_string()))
                }
                _ => {
                    existing.insert(remote.to_string());
                    Ok(())
                }
            }
        })
    }
}

#[tokio::test]
async fn a_new_folder_under_an_existing_parent_costs_exactly_one_request() {
    // ❗ The common case, and the whole reason the leaf is tried first.
    let server = FakeServer::new(&["/parent"]);

    let made = create_directory_all(&server, Path::new("/parent/new"))
        .await
        .expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::Created));
    assert_eq!(made.shallowest_created, Some(PathBuf::from("/parent/new")));
    assert_eq!(server.requests(), vec!["/parent/new"]);
}

#[tokio::test]
async fn a_directory_that_was_already_there_is_never_reported_as_created() {
    // ❗ THE data-safety cell: `Created` tells the transfer driver it may skip
    // every destination conflict probe inside.
    let server = FakeServer::new(&["/parent", "/parent/album"]);

    let made = create_directory_all(&server, Path::new("/parent/album"))
        .await
        .expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::AlreadyExisted));
    assert_eq!(made.shallowest_created, None, "nothing was made, so nothing to patch");
}

#[tokio::test]
async fn a_missing_ancestor_earns_the_walk_and_the_leaf_still_reports_created() {
    let server = FakeServer::new(&[]);

    let made = create_directory_all(&server, Path::new("/a/b/c"))
        .await
        .expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::Created));
    assert_eq!(
        made.shallowest_created,
        Some(PathBuf::from("/a")),
        "one patch, for the shallowest level: its parent is the only listing a pane could hold"
    );
    assert_eq!(
        server.requests(),
        vec!["/a/b/c", "/a", "/a/b", "/a/b/c"],
        "the leaf is tried once up front, then the levels shallowest first"
    );
}

#[tokio::test]
async fn the_volume_root_always_exists_and_costs_no_request() {
    let server = FakeServer::new(&[]);

    let made = create_directory_all(&server, Path::new("/")).await.expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::AlreadyExisted));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_level_someone_else_created_mid_walk_does_not_stop_it() {
    // A lost race on an ancestor is ordinary; the walk carries on and the leaf's
    // own answer is what the caller acts on.
    let server = FakeServer::new(&["/a"]);

    let made = create_directory_all(&server, Path::new("/a/b/c"))
        .await
        .expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::Created));
    assert_eq!(made.shallowest_created, Some(PathBuf::from("/a/b")));
}

#[tokio::test]
async fn a_refusal_that_is_not_about_a_missing_ancestor_stops_the_walk() {
    // ❗ A read-only export or a quota fails the same way at every level, so
    // walking would only spend round trips to arrive at the same answer.
    let server = FakeServer::new(&["/parent"]).refusing("/parent/new", VolumeError::PermissionDenied);

    let outcome = create_directory_all(&server, Path::new("/parent/new")).await;

    assert!(matches!(outcome, Err(VolumeError::PermissionDenied(_))));
    assert_eq!(server.requests(), vec!["/parent/new"], "it never walked");
}
