//! A big file on a real Samba share, opened through the app's viewer over smb2 (requires
//! the Docker SMB containers). The share-double version of the same paths is the crate's
//! `volume_source_test.rs`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::get_volume_manager;

use super::SeekTarget;
use super::app_viewer as viewer;

/// Every synthetic row is 15 digits plus a newline.
const ROW_BYTES: u64 = 16;

/// The byte at `offset` of the synthetic file: row `n` is `format!("{n:015}\n")`.
fn synthetic_byte(offset: u64) -> u8 {
    let row = offset / ROW_BYTES;
    let column = offset % ROW_BYTES;
    if column == ROW_BYTES - 1 {
        return b'\n';
    }
    let digit = (row / 10u64.pow((ROW_BYTES - 2 - column) as u32)) % 10;
    b'0' + digit as u8
}

fn row_number(text: &str) -> u64 {
    text.parse().expect("a synthetic row is 15 digits")
}

/// End to end against a real Samba share (requires Docker SMB containers): a file past
/// the background-index limit opens through smb2 and serves rows at the start, middle,
/// and end, with the real `SmbVolume`, the real `block_on` bridge, and the reader closed
/// on the runtime when the session goes.
#[test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
fn smb_integration_viewer_jumps_in_a_big_file_on_a_share() {
    use cmdr_smb::volume::testing::{ensure_clean, make_docker_volume, share_path, test_dir_name};

    let volume = tauri::async_runtime::block_on(make_docker_volume());
    let dir = test_dir_name();
    tauri::async_runtime::block_on(async {
        ensure_clean(&volume, &dir).await;
        volume.create_directory(Path::new(&dir)).await.unwrap();
    });
    let size = 40 * 1024 * 1024;
    let bytes: Vec<u8> = (0..size).map(synthetic_byte).collect();
    let relative = format!("{dir}/big.log");
    tauri::async_runtime::block_on(volume.create_file(Path::new(&relative), &bytes)).unwrap();
    let volume = Arc::new(volume);
    let id = format!("viewer-vsrc-smb-{dir}");
    get_volume_manager().register(&id, Arc::clone(&volume) as Arc<dyn Volume>);

    let opened = viewer()
        .open_session(&share_path(&relative), &id)
        .expect("opens through smb2");
    assert_eq!(opened.total_bytes, size);
    let last_row = size / ROW_BYTES - 1;
    for (fraction, near_row) in [(0.5, last_row / 2), (0.999, last_row * 999 / 1000), (0.0, 0)] {
        let chunk = viewer()
            .get_lines(
                &opened.session_id,
                SeekTarget::Fraction(fraction),
                500,
                &AtomicBool::new(false),
            )
            .expect("a jump reads");
        let first = row_number(&chunk.rows[0].text);
        assert!(
            first.abs_diff(near_row) <= 1,
            "a jump to {fraction} landed on row {first}"
        );
    }
    viewer().close_session(&opened.session_id).unwrap();

    get_volume_manager().unregister(&id);
    tauri::async_runtime::block_on(ensure_clean(&volume, &dir));
}
