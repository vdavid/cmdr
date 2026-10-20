//! Creates a new file holding given bytes under the first free numbered name in
//! a directory, as one managed instant op: the engine half of "paste clipboard
//! content as a file" (the clipboard half is `clipboard/paste_file.rs`).
//!
//! Naming/dedup reuses the ONE ` (N)` convention via `unique_name::numbered_name`
//! (the same helper `find_unique_name` uses), and writes atomically through
//! `Volume::create_file` (`O_CREAT|O_EXCL`): the loop tries `<stem>.<ext>`,
//! `<stem> (1).<ext>`, … retrying on the TYPED `VolumeError::AlreadyExists`, so
//! there's no pre-scan-then-write TOCTOU window and it works on any writable
//! volume (local or network).

use std::path::{Path, PathBuf};

use cmdr_fs::volume::VolumeError;
use cmdr_operation_log::types::{EntryType, Initiator, OpKind};

use super::mutation_error::MutationError;
use super::routed_writes::{route_serving, routed_mutation_refusal};
use super::types::WriteOperationType;
use super::unique_name::numbered_name;

impl super::Operations {
    /// Writes `bytes` into `dir` as `<stem>.<ext>` (numbered on collision) and
    /// returns the name it took. `dir` is already tilde-expanded by the caller.
    ///
    /// The write loops candidate names from `unique_name::numbered_name`, calling
    /// `Volume::create_file` (atomic O_EXCL create+write) and retrying on the typed
    /// `AlreadyExists`. Emits the create synthetic listing diff on success so the
    /// new file lands in the pane and the cursor-land plumbing works like mkfile.
    pub async fn create_numbered_file(
        &self,
        volume_id: Option<String>,
        dir: &Path,
        stem: &str,
        ext: &str,
        bytes: &[u8],
        initiator: Initiator,
    ) -> Result<String, MutationError> {
        let volume_id_str = volume_id.clone().unwrap_or_else(|| "root".to_string());
        // A route (inside a `.zip`, or a repo's `.git/<category>/` trees) serves `dir`
        // read-only; the drive underneath would take a real file there.
        let first_candidate = dir.join(numbered_name(stem, Some(ext), 0));
        if route_serving(self.host(), &volume_id_str, &first_candidate)
            .await
            .is_some()
        {
            return Err(routed_mutation_refusal(&first_candidate));
        }
        // Plain `get`: the route gate above sent an in-zip or `.git` folder away.
        let Some(volume) = self.host().volumes().get(&volume_id_str) else {
            return Err(
                super::mutation_error::unregistered_volume_refusal(self.host().volumes(), volume_id_str, dir).await,
            );
        };

        // Route through the managed `CreateFile` instant op (David-approved bypass routing): the
        // write is a real mutation, so it registers a brief `Running` record, marks
        // the volume busy, and — via the journal bracket — records a one-item
        // `CreateFile` operation exactly like mkfile, without a bespoke recorder. It
        // journals under the REAL volume id (`"root"` for the local drive), so a write
        // onto SMB / MTP is captured too.
        let descriptor = super::create::instant_descriptor(WriteOperationType::CreateFile, volume_id.as_deref(), stem);
        let op_id = descriptor.operation_id.clone();
        // The op's own scope on the engine, which journals it: an instant op has no
        // state to carry one.
        let scope = self.op_scope();
        super::journal::open_volume_op(
            &scope,
            op_id.as_str(),
            OpKind::CreateFile,
            initiator,
            &volume_id_str,
            None,
            1,
        );

        // Atomic O_EXCL create+write, retrying the shared `<stem> (N).<ext>` names on
        // the typed AlreadyExists (no pre-scan TOCTOU; works on any writable volume).
        // The whole write runs INSIDE the managed op so its `Running` record brackets
        // the retries.
        let write = async {
            let mut counter: u32 = 0;
            loop {
                let name = numbered_name(stem, Some(ext), counter);
                let path = dir.join(&name);
                // Register the destination with the downloads watcher's ignore set
                // before the syscall (no-op outside ~/Downloads).
                self.host().announce(&path);
                match volume.create_file(&path, bytes).await {
                    Ok(()) => break Ok::<(PathBuf, String), MutationError>((path, name)),
                    Err(VolumeError::AlreadyExists(_)) => {
                        counter = counter
                            .checked_add(1)
                            .ok_or(MutationError::AlreadyExists { name: name.clone() })?;
                    }
                    // Every other refusal is the volume's own, and `VolumeError`
                    // carries the path and the reason the frontend already words.
                    Err(error) => return Err(MutationError::Volume { error }),
                }
            }
        };
        let result = self.manager().run_instant(descriptor, write).await;

        super::journal::journal_instant_create(
            &scope,
            OpKind::CreateFile,
            EntryType::File,
            &volume_id_str,
            result.as_ref().ok().map(|(path, _)| path.as_path()),
        );
        let (new_path, final_name) = result?;

        // Synthetic listing diff so the new file appears and the FE cursor-lands
        // (mirrors create_file). Local-FS volumes only.
        if super::create::should_emit_synthetic_diff(self.host(), volume_id.as_deref()) {
            super::create::patch_listings_after_create(self.host(), volume_id.as_deref(), &new_path, dir);
        }

        Ok(final_name)
    }
}
