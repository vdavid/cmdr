//! `Volume`'s xattr methods on the store: the reference answer to what
//! `conformance::assert_xattrs_keep_their_promises` asks.

use super::InMemoryVolume;
use crate::ignore_poison::RwLockIgnorePoison;
use crate::metadata::Xattr;
use crate::volume::{VolumeError, XattrAnswer, XattrReadMode};
use std::path::{Path, PathBuf};

impl InMemoryVolume {
    pub(super) fn read_xattr_many_impl(&self, paths: &[PathBuf], name: &str, mode: XattrReadMode) -> Vec<XattrAnswer> {
        let entries = self.entries.read_ignore_poison();
        paths
            .iter()
            .map(|path| match entries.get(&self.normalize(path)) {
                Some(entry) => Ok(entry
                    .extra
                    .xattrs
                    .iter()
                    .find(|xattr| xattr.name == *name)
                    .map(|xattr| xattr.value.clone())),
                None if mode == XattrReadMode::AssumePathsExist => Ok(None),
                None => Err(VolumeError::NotFound(path.display().to_string())),
            })
            .collect()
    }

    pub(super) fn write_xattr_impl(&self, path: &Path, name: &str, value: &[u8]) -> Result<(), VolumeError> {
        let mut entries = self.entries.write_ignore_poison();
        let entry = entries
            .get_mut(&self.normalize(path))
            .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))?;
        entry.extra.xattrs.retain(|xattr| xattr.name != *name);
        entry.extra.xattrs.push(Xattr {
            name: name.into(),
            value: value.to_vec(),
        });
        Ok(())
    }

    pub(super) fn remove_xattr_impl(&self, path: &Path, name: &str) -> Result<(), VolumeError> {
        let mut entries = self.entries.write_ignore_poison();
        let entry = entries
            .get_mut(&self.normalize(path))
            .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))?;
        entry.extra.xattrs.retain(|xattr| xattr.name != *name);
        Ok(())
    }
}
