# Volume create-file can clobber an existing file

**Severity:** high **Lens:** A — Data safety **Confidence:** high

## Location

`apps/desktop/src-tauri/src/commands/file_system/write_ops.rs:138-157`
`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix.rs:279-296`
`apps/desktop/src-tauri/src/file_system/volume/backends/smb.rs:1304-1319`
`apps/desktop/src/lib/file-operations/mkfile/NewFileDialog.svelte:65-80`
`apps/desktop/src/lib/file-operations/mkfile/NewFileDialog.svelte:116-124`

## What

The `create_file_core` command assumes `Volume::create_file` is a no-overwrite operation and maps
`VolumeError::AlreadyExists` into a user-facing error. The local POSIX volume implementation uses `std::fs::write`,
which truncates an existing file, and the SMB implementation delegates to `tree.write_file` with no visible
exclusive-create guard. The Svelte dialog does a cache-based pre-check, but that check is advisory and raceable; the
backend path must be authoritative.

## Why it matters

A user can create `budget.xlsx` in a directory while Cmdr's listing is stale, or while another process creates the same
file between validation and confirmation. On local indexed volumes, the backend will truncate the existing file to zero
bytes instead of returning "already exists"; on SMB, the same risk depends on `tree.write_file` semantics, but this call
site does not enforce non-clobber behavior.

## Evidence

The command expects `Volume::create_file` to surface `AlreadyExists`:

```rust
138	    // Try to use Volume abstraction
139	    if let Some(volume) = get_volume_manager().get(&volume_id) {
140	        let new_path = PathBuf::from(&expanded_path).join(name);
141	        let new_path_clone = new_path.clone();
142	        let parent_path_owned = parent_path.to_string();
143	        let name_owned = name.to_string();
144
145	        tokio::time::timeout(Duration::from_secs(5), volume.create_file(&new_path_clone, b""))
146	            .await
147	            .map_err(|_| IpcError::timeout())?
148	            .map_err(|e| match e {
149	                crate::file_system::VolumeError::AlreadyExists(_) => {
150	                    IpcError::from_err(format!("'{}' already exists", name_owned))
151	                }
152	                crate::file_system::VolumeError::PermissionDenied(_) => IpcError::from_err(format!(
153	                    "Permission denied: cannot create '{}' in '{}'",
154	                    name_owned, parent_path_owned
155	                )),
156	                _ => IpcError::from_err(format!("Couldn't create file: {}", e)),
157	            })?;
```

The local volume implementation uses a truncating write:

```rust
279	    fn create_file<'a>(
280	        &'a self,
281	        path: &'a Path,
282	        content: &'a [u8],
283	    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
284	        let abs_path = self.resolve(path);
285	        if git::is_virtual(&abs_path) {
286	            return Box::pin(async { Err(VolumeError::NotSupported) });
287	        }
288	        let content = content.to_vec();
289	        Box::pin(async move {
290	            spawn_blocking(move || {
291	                std::fs::write(&abs_path, content)?;
292	                Ok(())
293	            })
294	            .await
295	            .unwrap()
296	        })
```

The SMB implementation also lacks an exclusive-create check at this layer:

```rust
1309	        Box::pin(async move {
1310	            let smb_path = self.to_smb_path(path);
1311	            let data = content.to_vec();
1312
1313	            debug!("SmbVolume::create_file: share={}, path={:?}", self.share_name, smb_path);
1314
1315	            {
1316	                let (tree, mut conn) = self.clone_session().await?;
1317	                let result = tree.write_file(&mut conn, &smb_path, &data).await;
1318	                self.handle_smb_result("create_file", result)?;
1319	            }
```

The frontend validation is listing-cache based and explicitly falls back to the backend when lookup fails:

```svelte
65	        isChecking = true
66	        try {
67	            const index = await findFileIndex(listingId, trimmed, showHiddenFiles)
68	            if (index !== null) {
69	                const entry = await getFileAt(listingId, index, showHiddenFiles)
70	                if (entry?.isDirectory) {
71	                    errorMessage = 'There is already a folder by this name in this folder.'
72	                } else {
73	                    errorMessage = 'There is already a file by this name in this folder.'
74	                }
75	            } else {
76	                errorMessage = ''
77	            }
78	        } catch {
79	            // If lookup fails (listing gone), clear error and let the backend handle it
80	            errorMessage = ''
```

## Suggested fix

Make the `Volume::create_file` contract explicitly non-clobbering and enforce it in every backend. For
`LocalPosixVolume`, use `OpenOptions::new().write(true).create_new(true).open(...)` and write the initial bytes only
after the exclusive open succeeds. For SMB, use the protocol's exclusive create disposition if the `smb2` wrapper
exposes it; otherwise add an atomic no-overwrite helper to that crate instead of relying on a preflight `exists` check.
Add backend tests that create an existing file, call `create_file`, assert `VolumeError::AlreadyExists`, and verify the
original bytes are unchanged.

## Notes

This is not covered by the documented write-operation overwrite policy in
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`; it is the separate "New File" action path.
