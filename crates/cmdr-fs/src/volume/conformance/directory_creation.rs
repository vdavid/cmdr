//! What [`Volume::create_directory_all`] promises: an honest `Created`, a
//! refusal that names a file in the way, and a walk that goes through a link to
//! a folder.
//!
//! Its own file because the three belong together: they are the three answers
//! to "a name on the way is already taken", and a backend that gets one wrong
//! usually got it wrong by not asking what took the name.

use std::path::Path;

use crate::volume::{DirectoryCreation, EntryKind, Volume, VolumeError};

/// [`Volume::create_directory_all`]
/// reports a directory that was ALREADY there as
/// [`DirectoryCreation::AlreadyExisted`], never as `Created`.
///
/// `dir` must already exist on `volume`; the assertion checks that first.
///
/// **Why this one is worth a shared assertion.** `Created` is a promise that the
/// directory was empty at that instant, and the transfer driver spends it: on a
/// `Created` answer it skips the per-file destination conflict probe for
/// everything it then writes inside. So a backend that answered `Created` for a
/// directory it merely found turns "would have prompted" into "overwrote", for
/// every file in the copy. Only the dangerous direction is pinned here — a
/// backend that answers `AlreadyExisted` when it did create the leaf is merely
/// slower, which is why the trait says "when in doubt, answer `AlreadyExisted`".
pub async fn assert_create_directory_all_reports_an_existing_dir_honestly(volume: &dyn Volume, dir: &Path) {
    assert!(
        volume.exists(dir).await,
        "fixture precondition: {} must already exist",
        dir.display()
    );

    let outcome = volume.create_directory_all(dir).await;
    assert!(
        matches!(outcome, Ok(DirectoryCreation::AlreadyExisted)),
        "create_directory_all over the existing {} must answer AlreadyExisted; \
         a Created answer tells the transfer driver it may skip every destination conflict probe inside. Got {outcome:?}",
        dir.display(),
    );
}

/// [`Volume::create_directory_all`] refuses a path a FILE blocks with
/// [`VolumeError::NotADirectory`] naming that file, whether the file sits at the
/// path itself or at one of its ancestors, and leaves the file alone.
///
/// `file` must already exist on `volume` and must not be a directory; nothing
/// may exist below it. The assertion checks the first part itself.
///
/// **Why this one is worth a shared assertion.** A `mkdir -p` treats "that name
/// is taken" as "the folder is there, carry on", and most protocols answer a
/// taken name the same way whatever holds it (WebDAV's `MKCOL` says 405, SFTP v3
/// says `SSH_FX_FAILURE`, an `exists()` probe says `true`). So a backend that
/// never asks WHAT holds the name gets both halves wrong with no failing call
/// anywhere: asked for the file's own path it answers `AlreadyExisted`, and the
/// transfer driver goes on to write into a folder that isn't one; asked for a
/// path below it, the failure surfaces one level down as `NotFound`, naming a
/// folder the user asked Cmdr to CREATE as the thing that's missing. Each
/// backend finds out its own way (a stat after the refusal, the probe it already
/// makes), so there is no shared mechanism to trust, only a shared promise.
pub async fn assert_create_directory_all_refuses_a_file_in_the_way(volume: &dyn Volume, file: &Path) {
    let before = volume
        .get_metadata(file)
        .await
        .unwrap_or_else(|e| panic!("fixture precondition: {} must exist, got {e:?}", file.display()));
    assert!(
        !before.is_directory,
        "fixture precondition: {} must be a file, and it is a directory",
        file.display()
    );
    let name = file
        .file_name()
        .expect("fixture precondition: the blocking file must have a final component");

    let below = file.join("inside");
    let deeper = below.join("deeper");
    for asked in [file, below.as_path(), deeper.as_path()] {
        let outcome = volume.create_directory_all(asked).await;
        let Err(VolumeError::NotADirectory(carried)) = outcome else {
            panic!(
                "create_directory_all({}) on {} must refuse with NotADirectory, because the FILE {} is in the way; \
                 got {outcome:?}",
                asked.display(),
                volume.name(),
                file.display(),
            );
        };
        assert_eq!(
            Path::new(&carried).file_name(),
            Some(name),
            "NotADirectory must carry the path of the thing IN THE WAY ({}), and for create_directory_all({}) \
             {} carried {carried:?}. The frontend renders that string as the file to move aside.",
            file.display(),
            asked.display(),
            volume.name(),
        );
    }

    let after = volume.get_metadata(file).await.unwrap_or_else(|e| {
        panic!(
            "a refused create_directory_all must leave {} in place, got {e:?}",
            file.display()
        )
    });
    assert!(
        !after.is_directory && after.size == before.size,
        "a refused create_directory_all must not touch the file in the way, but {} went from {:?} bytes to {:?} \
         (is_directory: {})",
        file.display(),
        before.size,
        after.size,
        after.is_directory,
    );
}

/// [`Volume::create_directory_all`] goes THROUGH a link that leads to a folder:
/// the link's own path is a folder that's already there, and a path below it is
/// created inside the folder the link points at.
///
/// `link` must already exist on `volume` as a symbolic link to the directory
/// `target`, and `target` must hold nothing named `made-through-the-link`. The
/// assertion checks the link half itself.
///
/// **Why this one is worth a shared assertion.** It pins the other side of
/// [`assert_create_directory_all_refuses_a_file_in_the_way`]: the question that
/// refusal asks is "does this name LEAD to a folder?", and a backend that
/// answers it with an `lstat` refuses every destination reached through a link,
/// which on a server is an ordinary place to be (`/home` on a NAS, `/sdcard` on
/// a phone). This is what `mkdir -p` does, and a path the user typed or
/// navigated through is theirs to write under. ❌ Don't carry the merge
/// engine's "not a directory in its own right" rule over here: that one guards a
/// merge from landing a folder's CONTENTS in a link's target nobody picked,
/// which is a different situation from creating the folder somebody asked for.
///
/// Backends with no links (MTP, WebDAV) and the read-only ones have nothing to
/// run this against.
pub async fn assert_create_directory_all_goes_through_a_link_to_a_folder(
    volume: &dyn Volume,
    link: &Path,
    target: &Path,
) {
    let kind = volume
        .entry_kind(link)
        .await
        .unwrap_or_else(|e| panic!("fixture precondition: {} must exist, got {e:?}", link.display()));
    assert_eq!(
        kind,
        EntryKind::Symlink,
        "fixture precondition: {} must be a symbolic link",
        link.display()
    );

    let outcome = volume.create_directory_all(link).await;
    assert!(
        matches!(outcome, Ok(DirectoryCreation::AlreadyExisted)),
        "create_directory_all({}) on {} must answer AlreadyExisted: the link leads to the folder {}. Got {outcome:?}",
        link.display(),
        volume.name(),
        target.display(),
    );

    let asked = link.join("made-through-the-link").join("deeper");
    let outcome = volume.create_directory_all(&asked).await;
    assert!(
        matches!(outcome, Ok(DirectoryCreation::Created)),
        "create_directory_all({}) on {} must create the folder through the link. Got {outcome:?}",
        asked.display(),
        volume.name(),
    );
    let landed = target.join("made-through-the-link").join("deeper");
    assert!(
        matches!(volume.is_directory(&landed).await, Ok(true)),
        "the folder asked for through the link must exist inside its target, at {}",
        landed.display(),
    );
}
