//! A private local scratch directory whose `Drop` removes it (and everything
//! inside) however the operation ends — success, error, or cancel. The
//! archive-edit flows that stage bytes locally share it: the remote-PARENT
//! pull-apply-upload-swap ([`super::archive_remote_edit`]) and the remote-SOURCE
//! pull-into-zip ([`super::archive_edit`]). The local temp never outlives the op.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// A uniquely-named directory under the system temp dir, cleaned on `Drop`.
pub(super) struct ScratchDir(PathBuf);

impl ScratchDir {
    /// Creates `<temp>/<prefix>-<uuid>`. The uuid keeps concurrent ops (and a
    /// leftover from a crashed prior op) from colliding, so the caller need not
    /// coordinate names.
    pub(super) fn new(prefix: &str) -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::ScratchDir;

    #[test]
    fn drop_removes_the_directory_and_its_contents() {
        let path = {
            let scratch = ScratchDir::new("cmdr-scratch-dir-test").expect("create scratch");
            let dir = scratch.path().to_path_buf();
            std::fs::write(dir.join("inside.txt"), b"bytes").expect("write into scratch");
            assert!(dir.exists(), "the scratch dir exists while the guard is alive");
            dir
            // `scratch` drops here.
        };
        assert!(
            !path.exists(),
            "the scratch dir (and its contents) is removed when the guard drops"
        );
    }
}
