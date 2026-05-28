# MTP upload buffers the entire file and bypasses cancellation progress

**Severity:** high **Lens:** G — Resource hygiene **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/volume/backends/mtp.rs:797-826`
`apps/desktop/src-tauri/src/mtp/connection/file_ops.rs:488-556`
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_strategy.rs:72-93`

## What

`MtpVolume::write_from_stream` receives the transfer progress/cancellation callback but names it `_on_progress` and
never calls it. It also drains the entire source stream into a `Vec<Bytes>` before starting the MTP upload. The shared
volume transfer strategy says destination `write_from_stream` implementations are responsible for per-chunk progress and
cancellation, so the MTP backend violates the expected transfer contract.

## Why it matters

Copying a 40 GB video archive or phone backup to an MTP device can allocate the whole file in memory before the actual
device upload begins, causing swap pressure or an out-of-memory crash. During that pre-buffering phase, and during the
subsequent upload from the prebuilt vector, a user pressing Cancel will not be observed through the normal per-chunk
callback path.

## Evidence

The MTP backend ignores `_on_progress` and collects all chunks:

```rust
797	    fn write_from_stream<'a>(
798	        &'a self,
799	        dest: &'a Path,
800	        size: u64,
801	        mut stream: Box<dyn VolumeReadStream>,
802	        _on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
803	    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
804	        Box::pin(async move {
805	            let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();
806	            let filename = dest
807	                .file_name()
808	                .and_then(|n| n.to_str())
809	                .ok_or_else(|| VolumeError::IoError {
810	                    message: "Invalid filename".into(),
811	                    raw_os_error: None,
812	                })?
813	                .to_string();
814
815	            // Stream chunks directly with .await (no need to pre-collect; we're
816	            // fully async now, no nested block_on risk).
817	            let mut chunks: Vec<bytes::Bytes> = Vec::new();
818	            while let Some(result) = stream.next_chunk().await {
819	                let data = result?;
820	                chunks.push(bytes::Bytes::from(data));
821	            }
```

The upload API takes ownership of that full vector and turns it into an iterator stream:

```rust
488	    pub async fn upload_from_chunks(
489	        &self,
490	        device_id: &str,
491	        storage_id: u32,
492	        dest_folder: &str,
493	        filename: &str,
494	        size: u64,
495	        chunks: Vec<bytes::Bytes>,
496	    ) -> Result<u64, MtpConnectionError> {
```

```rust
544	        // Convert chunks to stream format expected by mtp-rs
545	        let chunk_results: Vec<Result<bytes::Bytes, std::io::Error>> = chunks.into_iter().map(Ok).collect();
546	        let data_stream = futures_util::stream::iter(chunk_results);
547
548	        let new_handle = tokio::time::timeout(
549	            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
550	            storage.upload(parent_opt, object_info, data_stream),
551	        )
552	        .await
553	        .map_err(|_| MtpConnectionError::Timeout {
554	            device_id: device_id.to_string(),
555	        })?
556	        .map_err(|e| map_mtp_error(e, device_id))?;
```

The shared transfer path relies on the destination backend to enforce progress and cancellation:

```rust
72	/// Streams one file from source to destination via `open_read_stream` /
73	/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
74	/// the destination's `write_from_stream` implementation, which calls
75	/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
76	/// `ControlFlow::Break(())`.
77	async fn stream_pipe_file(
```

## Suggested fix

Change the MTP upload path to stream directly from `VolumeReadStream` into `storage.upload` through a custom
`Stream<Item = Result<Bytes, std::io::Error>>` adapter. The adapter should update `bytes_written`, call
`on_progress(bytes_written, size)` after each chunk, and stop with a cancellation error when the callback returns
`ControlFlow::Break(())`. If the `mtp-rs` API requires a concrete stream value, build that stream with `async_stream` or
`futures_util::stream::unfold`; do not pre-collect file contents.

## Notes

The misleading comment at `mtp.rs:815-816` says there is "no need to pre-collect", but the code immediately
pre-collects.
