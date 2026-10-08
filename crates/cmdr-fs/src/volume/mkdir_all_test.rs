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

/// A server whose directories and files are sets of remote paths.
///
/// It answers the way the real ones do: a taken name is `AlreadyExists`
/// whatever holds it, and a create under a file is `NotFound`, which is what
/// OpenSSH (`ENOTDIR`) and an RFC 4918 server (409) both come out as.
/// [`answering_bad_request_under_a_file`](Self::answering_bad_request_under_a_file)
/// turns it into Apache.
struct FakeServer {
    existing: Mutex<HashSet<String>>,
    files: HashSet<String>,
    made: Mutex<Vec<String>>,
    asked: Mutex<Vec<String>>,
    refuses: Option<Refusal>,
    bad_request_under_a_file: bool,
}

impl FakeServer {
    fn new(existing: &[&str]) -> Self {
        Self {
            existing: Mutex::new(existing.iter().map(|d| (*d).to_string()).collect()),
            files: HashSet::new(),
            made: Mutex::new(Vec::new()),
            asked: Mutex::new(Vec::new()),
            refuses: None,
            bad_request_under_a_file: false,
        }
    }

    fn with_file(mut self, remote: &str) -> Self {
        self.files.insert(remote.to_string());
        self
    }

    /// Apache `mod_dav`: every request addressed under a file, a create and a
    /// stat alike, answers 400, which no status table can classify.
    fn answering_bad_request_under_a_file(mut self) -> Self {
        self.bad_request_under_a_file = true;
        self
    }

    fn is_under_a_file(&self, remote: &str) -> bool {
        Path::new(remote)
            .ancestors()
            .skip(1)
            .any(|above| self.files.contains(above.to_string_lossy().as_ref()))
    }

    fn refusing(mut self, remote: &str, how: fn(String) -> VolumeError) -> Self {
        self.refuses = Some((remote.to_string(), how));
        self
    }

    fn requests(&self) -> Vec<String> {
        self.made.lock_ignore_poison().clone()
    }

    /// The names the walk asked `leads_to_a_directory` about, in order.
    fn questions(&self) -> Vec<String> {
        self.asked.lock_ignore_poison().clone()
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
            if self.bad_request_under_a_file && self.is_under_a_file(remote) {
                return Err(bad_request());
            }
            let mut existing = self.existing.lock_ignore_poison();
            if existing.contains(remote) || self.files.contains(remote) {
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

    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo> {
        Box::pin(async move {
            self.asked.lock_ignore_poison().push(remote.to_string());
            if self.bad_request_under_a_file && self.is_under_a_file(remote) {
                return Err(bad_request());
            }
            Ok(if self.existing.lock_ignore_poison().contains(remote) {
                LeadsTo::Directory
            } else if self.files.contains(remote) {
                LeadsTo::NotADirectory
            } else {
                LeadsTo::Nothing
            })
        })
    }
}

/// What Apache answers to anything addressed under a file.
fn bad_request() -> VolumeError {
    VolumeError::IoError {
        message: "HTTP 400 Bad Request".to_string(),
        raw_os_error: None,
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
    assert!(server.questions().is_empty(), "a folder it made needs no second look");
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
async fn a_taken_leaf_costs_one_look_at_what_took_it() {
    let server = FakeServer::new(&["/parent", "/parent/album"]);

    create_directory_all(&server, Path::new("/parent/album"))
        .await
        .expect("the walk");

    assert_eq!(server.requests(), vec!["/parent/album"]);
    assert_eq!(server.questions(), vec!["/parent/album"]);
}

#[tokio::test]
async fn a_file_where_the_folder_should_be_is_refused_and_named() {
    // A taken name reads the same whatever holds it, so `AlreadyExisted` here
    // would send the transfer driver on to write into a folder that isn't one.
    let server = FakeServer::new(&["/parent"]).with_file("/parent/notes");

    let outcome = create_directory_all(&server, Path::new("/parent/notes"))
        .await
        .map(|made| made.leaf);

    assert!(
        matches!(&outcome, Err(VolumeError::NotADirectory(path)) if path == "/parent/notes"),
        "got {outcome:?}"
    );
}

#[tokio::test]
async fn a_file_where_an_ancestor_should_be_is_named_and_the_folder_below_it_is_not_reported_missing() {
    // `/a/b` is a FILE, so the create below it fails, and the file is what the
    // user has to hear about. `NotFound` would name a folder they asked Cmdr to
    // CREATE as the thing that's missing.
    for asked in ["/a/b/c", "/a/b/c/d"] {
        let server = FakeServer::new(&["/a"]).with_file("/a/b");

        let outcome = create_directory_all(&server, Path::new(asked))
            .await
            .map(|made| made.leaf);

        assert!(
            matches!(&outcome, Err(VolumeError::NotADirectory(path)) if path == "/a/b"),
            "create_directory_all({asked}) got {outcome:?}"
        );
    }
}

#[tokio::test]
async fn a_server_that_cannot_address_anything_under_a_file_still_has_the_file_named() {
    // Apache never gets as far as "parent missing": the create answers 400, and
    // so does asking what is at any name under the file. The way up is the only
    // way to the file.
    for asked in ["/a/b/c", "/a/b/c/d"] {
        let server = FakeServer::new(&["/a"])
            .with_file("/a/b")
            .answering_bad_request_under_a_file();

        let outcome = create_directory_all(&server, Path::new(asked))
            .await
            .map(|made| made.leaf);

        assert!(
            matches!(&outcome, Err(VolumeError::NotADirectory(path)) if path == "/a/b"),
            "create_directory_all({asked}) got {outcome:?}"
        );
    }
}

#[tokio::test]
async fn an_unclassified_refusal_with_only_folders_above_it_keeps_its_own_answer() {
    // The look may only make a report MORE accurate: nothing is in the way
    // here, so whatever the server meant by its refusal is what gets reported.
    let server = FakeServer::new(&["/a"]).refusing("/a/new", |_| bad_request());

    let outcome = create_directory_all(&server, Path::new("/a/new"))
        .await
        .map(|made| made.leaf);

    assert!(matches!(outcome, Err(VolumeError::IoError { .. })), "got {outcome:?}");
    assert_eq!(server.questions(), vec!["/a/new", "/a"]);
}

#[tokio::test]
async fn a_walk_that_works_never_asks_what_the_ancestors_it_found_are() {
    // ❗ An ancestor that isn't a folder always fails the level below it, so the
    // question waits for that failure. A deep destination pays per level for the
    // creates alone.
    let server = FakeServer::new(&["/a", "/a/b"]);

    let made = create_directory_all(&server, Path::new("/a/b/c/d"))
        .await
        .expect("the walk");

    assert!(matches!(made.leaf, DirectoryCreation::Created));
    assert_eq!(made.shallowest_created, Some(PathBuf::from("/a/b/c")));
    assert!(server.questions().is_empty(), "asked about {:?}", server.questions());
}

#[tokio::test]
async fn a_refusal_below_a_real_folder_keeps_its_own_answer() {
    // The look at the level above may only make a report MORE accurate. `/a` is
    // a folder, so the permission refusal below it is the truth.
    let server = FakeServer::new(&["/a"]).refusing("/a/b", |path| VolumeError::PermissionDenied {
        path,
        raw_os_error: None,
    });

    let outcome = create_directory_all(&server, Path::new("/a/b/c"))
        .await
        .map(|made| made.leaf);

    assert!(
        matches!(outcome, Err(VolumeError::PermissionDenied { .. })),
        "got {outcome:?}"
    );
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
    let server = FakeServer::new(&["/parent"]).refusing("/parent/new", |path| VolumeError::PermissionDenied {
        path,
        raw_os_error: None,
    });

    let outcome = create_directory_all(&server, Path::new("/parent/new")).await;

    assert!(matches!(outcome, Err(VolumeError::PermissionDenied { .. })));
    assert_eq!(server.requests(), vec!["/parent/new"], "it never walked");
}
