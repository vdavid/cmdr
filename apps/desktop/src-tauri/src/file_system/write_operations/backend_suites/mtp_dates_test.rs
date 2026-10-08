//! "A copy keeps the source's date" between local disk and a virtual MTP
//! device, through the app's whole pipeline. The device stamps an upload's
//! `DateModified` onto its backing file and reports each file's mtime back, the
//! way Android does.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;

use super::network_dates_test_support::{
    a_copy_off_the_server_keeps_the_source_date, a_copy_onto_the_server_keeps_the_source_date,
};
use crate::mtp::test_support::{connect_virtual_device, device_lock, teardown, volume_for};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_onto_an_mtp_device_keeps_the_source_date() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, Some("/Documents")).await);

    a_copy_onto_the_server_keeps_the_source_date(volume, PathBuf::from("/Documents"), Duration::ZERO).await;

    teardown(device).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_off_an_mtp_device_keeps_the_source_date() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;
    let volume: Arc<dyn Volume> = Arc::new(volume_for(&device, Some("/Documents")).await);

    a_copy_off_the_server_keeps_the_source_date(volume, PathBuf::from("/Documents"), Duration::ZERO).await;

    teardown(device).await;
}
