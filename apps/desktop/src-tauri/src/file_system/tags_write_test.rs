use super::*;
use crate::file_system::tags_io::{TagWriteError, apply_merge, clear_tags, read_for_merge, toggle_color};
use crate::test_support::TestDir;
use std::path::Path;

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

fn temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("tags_write_{}_{name}", std::process::id()))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Registers the disk as a volume, once per test binary, so these go through the same route
/// every caller takes. What this exercises is the rule and the route; `Volume`'s own xattr
/// contract is `conformance::assert_xattrs_keep_their_promises`.
fn disk() -> Option<&'static str> {
    const ID: &str = "tags-write-test-disk";
    static REGISTERED: std::sync::Once = std::sync::Once::new();
    REGISTERED.call_once(|| {
        let volume =
            cmdr_local::LocalPosixVolume::local_folder("Disk", "/", cmdr_fs::volume::host::VolumeHost::detached());
        crate::file_system::volume::manager::get_volume_manager().register(ID, std::sync::Arc::new(volume));
    });
    Some(ID)
}

async fn toggle(paths: &[String], color: u8) -> Result<TagToggleOutcome, TagWriteError> {
    toggle_color(disk(), paths.to_vec(), color).await
}

/// Puts `tags` straight on the file: a fixture, not the path under test.
fn seed(file: &Path, tags: &[TagRef]) {
    let bytes = encode_tags_plist(tags).expect("an in-memory plist write can't fail");
    xattr::set(file, TAGS_XATTR, &bytes).expect("a temp file takes an xattr");
}

#[tokio::test]
async fn a_merge_writes_binary_plists_and_clearing_writes_the_empty_one() {
    let dir = temp_dir("round");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let path = path_str(&file);
    let write = async |tags: Vec<TagRef>| {
        let (current, skipped) = read_for_merge(disk(), vec![path.clone()]).await;
        assert!(skipped.is_empty());
        apply_merge(current, |_| tags.clone()).await.unwrap();
    };

    // Single color, and the raw xattr really is a binary plist.
    write(vec![tag("Red", 6)]).await;
    assert_eq!(read_tags(&file), vec![tag("Red", 6)]);
    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr present");
    assert!(raw.starts_with(b"bplist00"), "raw xattr must decode as binary plist");

    // Multiple, including a colorless named tag.
    let many = vec![tag("Important", 0), tag("Red", 6), tag("Blue", 4)];
    write(many.clone()).await;
    assert_eq!(read_tags(&file), many);

    // Clearing writes macOS's own empty array rather than deleting: a QNAP keeps what
    // it's told to delete, and macOS itself writes this.
    write(Vec::new()).await;
    assert_eq!(read_tags(&file), Vec::<TagRef>::new());
    assert_eq!(
        xattr::get(&file, TAGS_XATTR).unwrap(),
        Some(encode_tags_plist(&[]).unwrap())
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A file already in the target state isn't written: a write bumps the date on a
/// `streams_xattr` server. The marker is the bytes, since a plain rewrite would replace
/// them with Cmdr's own encoding.
#[tokio::test]
async fn files_already_in_the_target_state_are_not_rewritten() {
    let dir = temp_dir("no_churn");
    let untagged = dir.join("untagged.txt");
    let cleared = dir.join("cleared.txt");
    let red = dir.join("red.txt");
    for file in [&untagged, &cleared, &red] {
        std::fs::write(file, b"x").unwrap();
    }
    // An empty array in a spelling Cmdr's encoder wouldn't produce (XML, which a rewrite
    // would replace), and a red tag the same way.
    let xml = |tags: &[&str]| {
        let mut buf = Vec::new();
        plist::Value::Array(tags.iter().map(|tag| plist::Value::String((*tag).into())).collect())
            .to_writer_xml(&mut buf)
            .unwrap();
        buf
    };
    xattr::set(&cleared, TAGS_XATTR, &xml(&[])).unwrap();
    xattr::set(
        &red,
        TAGS_XATTR,
        &xml(&["Red
6"]),
    )
    .unwrap();

    // Clearing what holds no tags writes nothing, and creates no attribute.
    clear_tags(disk(), vec![path_str(&untagged), path_str(&cleared)])
        .await
        .unwrap();
    assert_eq!(xattr::get(&untagged, TAGS_XATTR).unwrap(), None, "no attribute appears");
    assert_eq!(
        xattr::get(&cleared, TAGS_XATTR).unwrap(),
        Some(xml(&[])),
        "not rewritten"
    );

    // A toggle that adds red leaves the file that already has it alone.
    let plain = dir.join("plain.txt");
    std::fs::write(&plain, b"x").unwrap();
    toggle(&[path_str(&red), path_str(&plain)], 6).await.unwrap();
    assert_eq!(
        xattr::get(&red, TAGS_XATTR).unwrap(),
        Some(xml(&["Red
6"])),
        "not rewritten"
    );
    assert_eq!(read_tags(&plain), vec![tag("Red", 6)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn clearing_overwrites_a_value_it_cannot_decode() {
    let dir = temp_dir("clear_garbage");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    xattr::set(&file, TAGS_XATTR, b"garbage").unwrap();
    clear_tags(disk(), vec![path_str(&file)]).await.unwrap();
    assert_eq!(
        xattr::get(&file, TAGS_XATTR).unwrap(),
        Some(encode_tags_plist(&[]).unwrap())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A row that's an archive file itself is on the drive, and its tag goes there: asking the
/// route about the row rather than its folder would hand it to the archive inside it.
#[tokio::test]
async fn a_zip_file_row_is_tagged_on_the_drive() {
    let dir = temp_dir("zip_row");
    let file = dir.join("photos.zip");
    std::fs::write(&file, b"not really a zip").unwrap();
    toggle(&[path_str(&file)], 6).await.unwrap();
    assert_eq!(read_tags(&file), vec![tag("Red", 6)]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A path nothing stores tags for is skipped and reported, ❌ never a failed toggle: the
/// same selection may hold rows that can.
#[tokio::test]
async fn a_path_inside_an_archive_is_skipped() {
    let dir = temp_dir("zip_inner");
    let zip = dir.join("photos.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
    writer
        .start_file("inner.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut writer, b"x").unwrap();
    writer.finish().unwrap();

    let outcome = toggle(&[path_str(&zip.join("inner.txt"))], 6).await.unwrap();

    assert!(outcome.updates.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert!(matches!(
        outcome.skipped[0].reason,
        TagReadError::Unreadable(cmdr_fs::volume::VolumeError::NotSupported)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_adds_color_preserving_existing_unrelated_tags() {
    let dir = temp_dir("add");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    seed(&file, &[tag("Important", 0)]);

    let result = toggle(&[path_str(&file)], 6).await.unwrap();
    let expected = vec![tag("Important", 0), tag("Red", 6)];
    assert_eq!(read_tags(&file), expected, "red added, colorless tag preserved");
    assert_eq!(result.updates, vec![(path_str(&file), expected)]);
    assert!(result.skipped.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_removes_only_the_targeted_color() {
    let dir = temp_dir("remove");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    seed(&file, &[tag("Blue", 4), tag("Red", 6)]);

    // File already carries red → toggling red strips it, keeps blue.
    toggle(&[path_str(&file)], 6).await.unwrap();
    assert_eq!(read_tags(&file), vec![tag("Blue", 4)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_multi_file_all_have_removes_from_all() {
    let dir = temp_dir("multi_all");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"x").unwrap();
    std::fs::write(&b, b"x").unwrap();
    seed(&a, &[tag("Red", 6)]);
    seed(&b, &[tag("Green", 2), tag("Red", 6)]);

    // Both already red → remove from both; b keeps green.
    toggle(&[path_str(&a), path_str(&b)], 6).await.unwrap();
    assert_eq!(read_tags(&a), Vec::<TagRef>::new());
    assert_eq!(read_tags(&b), vec![tag("Green", 2)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_multi_file_some_have_adds_to_all_without_duplicating() {
    let dir = temp_dir("multi_some");
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, b"x").unwrap();
    std::fs::write(&b, b"x").unwrap();
    // `a` already has a CUSTOM red tag; `b` has none.
    seed(&a, &[tag("Important", 6)]);

    // Not all have red → add to all. `a` keeps its custom red (no system "Red"
    // duplicate); `b` gains the canonical "Red".
    toggle(&[path_str(&a), path_str(&b)], 6).await.unwrap();
    assert_eq!(read_tags(&a), vec![tag("Important", 6)], "no duplicate same-color tag");
    assert_eq!(read_tags(&b), vec![tag("Red", 6)]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_rejects_colorless_index() {
    let dir = temp_dir("reject");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(toggle(&[path_str(&file)], 0).await.is_err());
    assert!(toggle(&[path_str(&file)], 8).await.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A binary plist array holding one tag string and one integer: Finder never
/// writes this, but another tool can, and the lenient reader drops the integer.
fn plist_with_non_string_element() -> Vec<u8> {
    let mut buf = Vec::new();
    plist::Value::Array(vec![
        plist::Value::String("Work\n0".into()),
        plist::Value::Integer(42.into()),
    ])
    .to_writer_binary(&mut buf)
    .expect("an in-memory plist write can't fail");
    buf
}

/// #541 regression: a value Cmdr can't fully decode must never read as "no tags"
/// on the write path, or the toggle replaces every existing tag with one color.
#[tokio::test]
async fn toggle_skips_a_file_whose_tags_are_malformed_and_leaves_its_bytes_alone() {
    let dir = temp_dir("malformed");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let garbage = b"not a plist at all".to_vec();
    xattr::set(&file, TAGS_XATTR, &garbage).unwrap();

    let outcome = toggle(&[path_str(&file)], 6).await.unwrap();

    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, garbage, "the original xattr bytes must be unchanged");
    assert!(outcome.updates.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].path, path_str(&file));
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Undecodable));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn toggle_skips_a_file_with_a_non_string_tag_element() {
    let dir = temp_dir("non_string");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let original = plist_with_non_string_element();
    xattr::set(&file, TAGS_XATTR, &original).unwrap();

    let outcome = toggle(&[path_str(&file)], 6).await.unwrap();

    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, original, "the non-string element must survive");
    assert_eq!(outcome.skipped.len(), 1);
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Undecodable));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Write permission without read: `getxattr` fails, `setxattr` would succeed.
#[tokio::test]
async fn toggle_skips_a_write_only_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("write_only");
    let file = dir.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    seed(&file, &[tag("Important", 0), tag("Blue", 4)]);
    let original = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr present");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o200)).unwrap();
    if xattr::get(&file, TAGS_XATTR).is_ok() {
        // Running as root (or a filesystem that ignores the mode): no unreadable state to test.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    let outcome = toggle(&[path_str(&file)], 6).await.unwrap();

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let raw = xattr::get(&file, TAGS_XATTR).unwrap().expect("xattr still present");
    assert_eq!(raw, original, "an unreadable file's tags must not be overwritten");
    assert_eq!(outcome.skipped.len(), 1);
    assert!(matches!(outcome.skipped[0].reason, TagReadError::Unreadable(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The skip is per file: the readable rest of the selection still gets the color,
/// and the skipped file doesn't sway the all-have decision.
#[tokio::test]
async fn toggle_tags_the_readable_files_and_skips_only_the_malformed_one() {
    let dir = temp_dir("mixed");
    let good = dir.join("good.txt");
    let bad = dir.join("bad.txt");
    std::fs::write(&good, b"x").unwrap();
    std::fs::write(&bad, b"x").unwrap();
    seed(&good, &[tag("Work", 0)]);
    xattr::set(&bad, TAGS_XATTR, b"garbage").unwrap();

    let outcome = toggle(&[path_str(&good), path_str(&bad)], 6).await.unwrap();

    let expected = vec![tag("Work", 0), tag("Red", 6)];
    assert_eq!(read_tags(&good), expected);
    assert_eq!(outcome.updates, vec![(path_str(&good), expected)]);
    assert_eq!(xattr::get(&bad, TAGS_XATTR).unwrap().as_deref(), Some(&b"garbage"[..]));
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].path, path_str(&bad));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_missing_file_is_skipped_rather_than_read_as_untagged() {
    let dir = temp_dir("missing");
    let outcome = toggle(&[path_str(&dir.join("gone.txt"))], 6).await.unwrap();
    assert!(outcome.updates.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert!(!dir.join("gone.txt").exists(), "a toggle never creates a file");
    let _ = std::fs::remove_dir_all(&dir);
}

/// D11 regression: tagging must NOT clobber a folder's custom-icon FinderInfo.
/// Zeroing `com.apple.FinderInfo` would wipe `kHasCustomIcon` (0x0400 at offset 8)
/// and break both the custom icon and Cmdr's own `custom_folder_icon`.
#[tokio::test]
async fn tagging_preserves_finder_info_custom_icon_flag() {
    let dir = temp_dir("d11");
    let folder = dir.join("CustomIconFolder");
    std::fs::create_dir(&folder).unwrap();

    // FinderInfo with kHasCustomIcon set (big-endian 0x0400 at byte offset 8).
    let mut finder_info = vec![0u8; 32];
    finder_info[8] = 0x04;
    finder_info[9] = 0x00;
    xattr::set(&folder, "com.apple.FinderInfo", &finder_info).unwrap();

    toggle(&[path_str(&folder)], 6).await.unwrap();

    // The tag landed…
    assert_eq!(read_tags(&folder), vec![tag("Red", 6)]);
    // …and the FinderInfo blob (with the custom-icon flag) survived untouched.
    let fi = xattr::get(&folder, "com.apple.FinderInfo")
        .unwrap()
        .expect("FinderInfo must still be present after tagging");
    assert_eq!(fi.len(), 32, "FinderInfo must not be truncated");
    let flags = u16::from_be_bytes([fi[8], fi[9]]);
    assert!(flags & 0x0400 != 0, "kHasCustomIcon must survive a tag write");

    let _ = std::fs::remove_dir_all(&dir);
}
