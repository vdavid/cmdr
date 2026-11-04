use super::*;

/// The `._` file sits beside its file, in the same folder, at the share root
/// too; a sidecar has none of its own, and the share's top names nothing.
#[test]
fn the_sidecar_sits_beside_its_file() {
    assert_eq!(sidecar_path("a/b/.cmdr-tmp-x").as_deref(), Some("a/b/._.cmdr-tmp-x"));
    assert_eq!(sidecar_path("photo.jpg").as_deref(), Some("._photo.jpg"));
    assert_eq!(sidecar_path("a/Folder").as_deref(), Some("a/._Folder"));
    assert_eq!(sidecar_path("a/._photo.jpg"), None);
    assert_eq!(sidecar_path(""), None);
    assert_eq!(sidecar_path("a/"), None);
}

/// Only AppleDouble's magic counts: AppleSingle (`00 05 16 00`), a short file,
/// and a person's own text all stay.
#[test]
fn only_the_magic_makes_a_file_apple_double() {
    assert!(is_apple_double(&[0x00, 0x05, 0x16, 0x07, 0x00, 0x02]));
    assert!(is_apple_double(&APPLE_DOUBLE_MAGIC));
    assert!(!is_apple_double(&[0x00, 0x05, 0x16, 0x00]));
    assert!(!is_apple_double(&[0x00, 0x05, 0x16]));
    assert!(!is_apple_double(b"my notes"));
    assert!(!is_apple_double(&[]));
}

/// No delete probes until the share shows a `._` name or takes a resource fork.
#[test]
fn a_share_shows_sidecars_once_it_names_one_or_takes_a_fork() {
    let sidecars = Sidecars::default();
    sidecars.note_name("photo.jpg");
    sidecars.note_written_xattr("com.apple.metadata:_kMDItemUserTags");
    assert!(!sidecars.seen());
    sidecars.note_name("._photo.jpg");
    assert!(sidecars.seen());

    let forked = Sidecars::default();
    forked.note_written_xattr(RESOURCE_FORK_XATTR);
    assert!(forked.seen());
}

/// A taken sidecar answers its later delete once, and the oldest goes first
/// past the cap.
#[test]
fn a_taken_sidecar_answers_its_later_delete_once() {
    let sidecars = Sidecars::default();
    sidecars.remember_taken("a/._x");
    assert!(sidecars.take_back("a/._x"));
    assert!(!sidecars.take_back("a/._x"), "only once");
    assert!(!sidecars.take_back("a/._y"));

    for n in 0..=TAKEN_CAP {
        sidecars.remember_taken(&format!("._{n}"));
    }
    assert!(!sidecars.take_back("._0"), "the oldest is forgotten past the cap");
    assert!(sidecars.take_back("._1"));
    assert!(sidecars.take_back(&format!("._{TAKEN_CAP}")));
}
