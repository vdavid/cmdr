//! `Volume`'s xattr methods on an OS path: the `xattr` crate on the blocking pool.

use cmdr_fs::volume::{VolumeError, XattrAnswer, XattrReadMode};
use std::io;
use std::path::PathBuf;
use tokio::task::spawn_blocking;

pub(super) async fn read_many(paths: Vec<PathBuf>, name: String, mode: XattrReadMode) -> Vec<XattrAnswer> {
    spawn_blocking(move || {
        paths
            .iter()
            .map(|path| match xattr::get(path, &name) {
                Err(e) if e.kind() == io::ErrorKind::NotFound && mode == XattrReadMode::AssumePathsExist => Ok(None),
                answer => answer.map_err(|e| VolumeError::from_io_at(&e, path)),
            })
            .collect()
    })
    .await
    .expect("spawn_blocking xattr read closure doesn't panic and the task is uncancelable")
}

pub(super) async fn write(path: PathBuf, name: String, value: Vec<u8>) -> Result<(), VolumeError> {
    spawn_blocking(move || xattr::set(&path, &name, &value).map_err(|e| VolumeError::from_io_at(&e, &path)))
        .await
        .expect("spawn_blocking xattr write closure doesn't panic and the task is uncancelable")
}

pub(super) async fn remove(path: PathBuf, name: String) -> Result<(), VolumeError> {
    spawn_blocking(move || {
        // Probed first because removing an absent one answers a platform's own
        // `ENOATTR`, and the contract calls that a success.
        match xattr::get(&path, &name) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => xattr::remove(&path, &name),
            Err(e) => Err(e),
        }
        .map_err(|e| VolumeError::from_io_at(&e, &path))
    })
    .await
    .expect("spawn_blocking xattr removal closure doesn't panic and the task is uncancelable")
}
