//! The date scenarios with `InMemoryVolume` as the "server", so the engine's
//! half of "a copy keeps the source's date" (the checkpoint wrapper, staging,
//! the final rename) is pinned in the unit lane, onto local disk and off it,
//! whatever state the network backends are in.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::{InMemoryVolume, Volume};

use super::network_dates_test_support::{
    a_copy_off_the_server_keeps_the_source_date, a_copy_onto_the_server_keeps_the_source_date,
    copied_folders_off_the_server_keep_their_dates, copied_folders_onto_the_server_keep_their_dates,
};

fn double() -> Arc<dyn Volume> {
    Arc::new(InMemoryVolume::new("Dated double"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_from_local_disk_keeps_the_source_date() {
    let remote = double();
    remote
        .create_directory(std::path::Path::new("/dated"))
        .await
        .expect("make the destination folder");
    a_copy_onto_the_server_keeps_the_source_date(remote, "/dated".into(), Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_onto_local_disk_keeps_the_source_date() {
    let remote = double();
    remote
        .create_directory(std::path::Path::new("/dated"))
        .await
        .expect("make the source folder");
    a_copy_off_the_server_keeps_the_source_date(remote, "/dated".into(), Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folders_copied_from_local_disk_keep_their_dates() {
    let remote = double();
    remote
        .create_directory(std::path::Path::new("/dated"))
        .await
        .expect("make the destination folder");
    copied_folders_onto_the_server_keep_their_dates(remote, "/dated".into(), Duration::ZERO).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folders_copied_onto_local_disk_keep_their_dates() {
    let remote = double();
    remote
        .create_directory(std::path::Path::new("/dated"))
        .await
        .expect("make the source folder");
    copied_folders_off_the_server_keep_their_dates(remote, "/dated".into()).await;
}
