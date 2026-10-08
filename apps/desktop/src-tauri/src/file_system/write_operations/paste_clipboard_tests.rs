//! Tests for `write_payload_to_dir`: the decoupled write core (an already-read
//! `ClipboardPayload` + a `&Path` dir, no NSPasteboard / `MainThreadMarker` /
//! `AppHandle`). Drives it against a `TempDir` with a real registered "root"
//! volume, the same path production hits.
//!
//! Pins: base name `pasted.<ext>`, ` (N)` dedup on collision (the ONE
//! `numbered_name` convention), byte-verbatim writes for the passthrough flavors,
//! the `Nothing` → `Ok(None)` no-op, and a read-only dir failing closed.

use super::{write_payload_replying, write_payload_to_dir};
use crate::clipboard::{ClipboardPasteOutcome, ClipboardPayload, PasteClipboardReply, PastedKind};

use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Registers a real local-FS "root" volume so `write_payload_to_dir` with
/// `volume_id = None` (→ "root") exercises the timed `Volume::create_file` path.
/// Idempotent via `register_if_absent`. Mirrors `create/tests.rs`.
fn ensure_root_volume() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use std::sync::Arc;
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
}

async fn write_text(dir: &Path, text: &str) -> Option<crate::clipboard::PastedClipboardFile> {
    write_payload_to_dir(None, dir, ClipboardPayload::Text(text.to_string()))
        .await
        .expect("write should succeed")
}

#[tokio::test]
async fn text_writes_pasted_txt_with_verbatim_bytes() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let file = write_text(tmp.path(), "hello clipboard").await.expect("Some file");
    assert_eq!(file.name, "pasted.txt");
    assert_eq!(file.kind, PastedKind::Text);
    assert_eq!(fs::read(tmp.path().join("pasted.txt")).unwrap(), b"hello clipboard");
}

#[tokio::test]
async fn markdown_text_writes_pasted_md() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let file = write_text(tmp.path(), "# A heading\n\nwith prose")
        .await
        .expect("Some file");
    assert_eq!(file.name, "pasted.md", "the sniffer promotes a heading to .md");
    assert_eq!(file.kind, PastedKind::Text);
    assert!(tmp.path().join("pasted.md").exists());
}

#[tokio::test]
async fn png_payload_writes_pasted_png_verbatim() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nfake-but-verbatim".to_vec();
    let file = write_payload_to_dir(None, tmp.path(), ClipboardPayload::Png(bytes.clone()))
        .await
        .unwrap()
        .expect("Some file");
    assert_eq!(file.name, "pasted.png");
    assert_eq!(file.kind, PastedKind::Image);
    assert_eq!(
        fs::read(tmp.path().join("pasted.png")).unwrap(),
        bytes,
        "PNG bytes written verbatim"
    );
}

/// Routing paste through the managed `CreateFile` op means it journals for free
/// (David-approved bypass routing): a paste records exactly one `CreateFile` operation,
/// `done` and `rollbackable` (a net-new file).
#[tokio::test]
async fn paste_journals_a_create_file_operation() {
    use crate::operation_log::TestJournalGuard;
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{open_read_connection, operation_log_db_path, recent_operations};
    use crate::operation_log::types::{ExecutionStatus, OpKind, RollbackState};
    use crate::operation_log::writer::OperationLogWriter;
    use std::sync::Arc;

    ensure_root_volume();
    let jdir = TempDir::new().unwrap();
    let jdb = operation_log_db_path(jdir.path());
    // Serializes journal-slot tests under plain `cargo test`; clears on drop.
    let _journal = TestJournalGuard::install(Arc::new(WriterJournal::new(
        OperationLogWriter::spawn(&jdb).expect("spawn writer"),
    )));

    let tmp = TempDir::new().unwrap();
    let file = write_text(tmp.path(), "journaled paste").await.expect("Some file");
    assert_eq!(file.name, "pasted.txt");

    let conn = open_read_connection(&jdb).expect("read conn");
    let ops = recent_operations(&conn, 10).expect("recent ops");
    let create = ops
        .iter()
        .find(|o| o.kind == OpKind::CreateFile)
        .expect("paste records a CreateFile op");
    assert_eq!(create.execution_status, ExecutionStatus::Done);
    assert_eq!(
        create.rollback_state,
        RollbackState::Rollbackable,
        "a net-new pasted file is rollbackable"
    );
}

#[tokio::test]
async fn jpeg_payload_writes_pasted_jpg_verbatim_no_recompression() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let bytes = b"\xff\xd8\xff\xe0jpeg-bytes-as-is".to_vec();
    let file = write_payload_to_dir(None, tmp.path(), ClipboardPayload::Jpeg(bytes.clone()))
        .await
        .unwrap()
        .expect("Some file");
    assert_eq!(file.name, "pasted.jpg");
    assert_eq!(file.kind, PastedKind::Image);
    assert_eq!(
        fs::read(tmp.path().join("pasted.jpg")).unwrap(),
        bytes,
        "JPEG bytes written verbatim (no recompression)"
    );
}

#[tokio::test]
async fn pdf_payload_writes_pasted_pdf_verbatim() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let bytes = b"%PDF-1.7\n...raw pdf...".to_vec();
    let file = write_payload_to_dir(None, tmp.path(), ClipboardPayload::Pdf(bytes.clone()))
        .await
        .unwrap()
        .expect("Some file");
    assert_eq!(file.name, "pasted.pdf");
    assert_eq!(file.kind, PastedKind::Pdf);
    assert_eq!(fs::read(tmp.path().join("pasted.pdf")).unwrap(), bytes);
}

#[tokio::test]
async fn nothing_payload_writes_no_file_and_returns_none() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    let result = write_payload_to_dir(None, tmp.path(), ClipboardPayload::Nothing)
        .await
        .unwrap();
    assert!(result.is_none(), "Nothing is a no-op returning Ok(None)");
    // Coverage: the dir stays empty (the no-op did nothing, not something wrong).
    assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0, "no file created");
}

#[tokio::test]
async fn collisions_use_the_numbered_name_dedup_scheme() {
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();

    let first = write_text(tmp.path(), "one").await.expect("Some");
    let second = write_text(tmp.path(), "two").await.expect("Some");
    let third = write_text(tmp.path(), "three").await.expect("Some");

    assert_eq!(first.name, "pasted.txt");
    assert_eq!(second.name, "pasted (1).txt", "second reuses the ` (N)` convention");
    assert_eq!(third.name, "pasted (2).txt");

    // Each distinct file exists with its own content (no clobber).
    assert_eq!(fs::read(tmp.path().join("pasted.txt")).unwrap(), b"one");
    assert_eq!(fs::read(tmp.path().join("pasted (1).txt")).unwrap(), b"two");
    assert_eq!(fs::read(tmp.path().join("pasted (2).txt")).unwrap(), b"three");
}

#[tokio::test]
async fn read_only_directory_fails_closed_without_writing() {
    use std::os::unix::fs::PermissionsExt;
    ensure_root_volume();
    let tmp = TempDir::new().unwrap();
    // Make the directory read + execute only (no write) so create_file can't land.
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = write_payload_to_dir(None, tmp.path(), ClipboardPayload::Text("blocked".to_string())).await;

    // Restore write perms before the TempDir drops so cleanup can remove it.
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        result.is_err(),
        "a read-only destination must fail, not silently drop the paste"
    );
    assert_eq!(
        fs::read_dir(tmp.path()).unwrap().count(),
        0,
        "no partial file left behind on failure"
    );
}

/// A share that holds new files for `delay`, with an empty `/docs`, registered
/// under `volume_id`.
async fn register_slowly_creating_volume(volume_id: &str, delay: std::time::Duration) {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{InMemoryVolume, Volume};
    use crate::test_support::SlowVolume;
    use std::sync::Arc;

    let inner = InMemoryVolume::new("Slow NAS");
    inner.create_directory(Path::new("/docs")).await.unwrap();
    get_volume_manager().register_if_absent(volume_id, Arc::new(SlowVolume::creating_slowly(inner, delay)));
}

/// A paste onto a busy share sat under a bare timeout that DROPPED the write
/// mid-flight and answered `TimedOut`. Past the reply deadline it's "still
/// running", and the settle names the file it made.
#[tokio::test(start_paused = true)]
async fn a_slow_paste_replies_still_running_then_settles_with_the_file() {
    let volume_id = "smb-slow-paste-test";
    register_slowly_creating_volume(volume_id, std::time::Duration::from_secs(40)).await;
    let (tx, rx) = tokio::sync::oneshot::channel();

    let reply = write_payload_replying(
        Some(volume_id.to_string()),
        "/docs".into(),
        ClipboardPayload::Text("hello".to_string()),
        move |settled| {
            let _ = tx.send(settled);
        },
    )
    .await;

    let Ok(PasteClipboardReply::StillRunning { pending_id }) = reply else {
        panic!("a 40 s paste is still running at the deadline, not refused: {reply:?}");
    };
    let settled = rx.await.expect("the paste settles");
    assert_eq!(settled.pending_id, pending_id);
    assert!(
        matches!(&settled.outcome, ClipboardPasteOutcome::Landed { file: Some(file) } if file.name == "pasted.txt"),
        "{settled:?}"
    );
}

/// A paste that ends in time is the reply itself, file and all.
#[tokio::test(start_paused = true)]
async fn a_quick_paste_replies_done_with_the_file() {
    let volume_id = "smb-quick-paste-test";
    register_slowly_creating_volume(volume_id, std::time::Duration::from_millis(200)).await;

    let reply = write_payload_replying(
        Some(volume_id.to_string()),
        "/docs".into(),
        ClipboardPayload::Text("hi".to_string()),
        |_| {},
    )
    .await;

    assert!(
        matches!(&reply, Ok(PasteClipboardReply::Done { file: Some(file) }) if file.name == "pasted.txt"),
        "{reply:?}"
    );
}
