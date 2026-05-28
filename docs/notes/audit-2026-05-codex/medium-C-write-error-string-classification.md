# Write-operation errors are classified by message text

**Severity:** medium **Lens:** C — Error handling discipline **Confidence:** high

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/types.rs:507-567`

## What

`classify_io_error` first uses errno where available, but then falls back to lowercasing the free-form error message and
matching substrings such as `disconnect`, `read-only`, `connection`, `timeout`, and `invalid name`. The repository rules
explicitly ban string-matching error or state classification because messages change across OS versions, locales, and
dependency versions. This classifier feeds user-visible write-operation error variants, so a misclassification can
produce the wrong recovery guidance.

## Why it matters

On a localized macOS install, or through a wrapped SMB/MTP/library error with different wording, Cmdr can classify a
disconnected device as a generic I/O error, or classify an unrelated error containing "connection" as a network
interruption. In the write path, that changes which toast, retry guidance, and operation status the user sees while a
destructive or partially completed operation is being recovered.

## Evidence

The fallback path branches on message substrings:

```rust
530	    let msg = e.to_string();
531	    let lower = msg.to_lowercase();
532
533	    // Message-based heuristics as fallback for errors without raw OS codes
534	    // (synthetic/wrapped errors from libraries)
535	    if lower.contains("disconnect") || lower.contains("no such device") {
536	        return WriteOperationError::DeviceDisconnected { path };
537	    }
538	    if lower.contains("read-only") || lower.contains("read only") {
539	        return WriteOperationError::ReadOnlyDevice {
540	            path,
541	            device_name: None,
542	        };
543	    }
544	    if lower.contains("connection") || lower.contains("timed out") || lower.contains("timeout") {
545	        return WriteOperationError::ConnectionInterrupted { path };
546	    }
547	    if lower.contains("name too long") || lower.contains("file name too long") {
548	        return WriteOperationError::NameTooLong { path };
549	    }
550	    if lower.contains("invalid") && lower.contains("name") {
551	        return WriteOperationError::InvalidName { path, message: msg };
552	    }
```

The same function also special-cases `PermissionDenied` by message text:

```rust
554	    // ErrorKind-based fallback, with one kind-specific heuristic
555	    match e.kind() {
556	        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path },
557	        std::io::ErrorKind::PermissionDenied => {
558	            // macOS immutable flag manifests as PermissionDenied + "operation not permitted"
559	            if lower.contains("immutable") || lower.contains("operation not permitted") {
560	                return WriteOperationError::FileLocked { path };
561	            }
562	            WriteOperationError::PermissionDenied { path, message: msg }
563	        }
```

## Suggested fix

Preserve typed errors until the write-operation boundary instead of collapsing backend failures into plain
`std::io::Error`. Local filesystem paths should rely on errno and `ErrorKind`; SMB, MTP, and synthetic errors should map
their own typed variants into `WriteOperationError` before formatting. If one OS-specific text fallback is truly
unavoidable, isolate it behind a narrow helper with an explicit allow comment, locale assumptions, and snapshot tests
that pin the exact upstream messages.

## Notes

I did not file the Linux USB-permission string matching in the MTP connection layer because
`apps/desktop/src-tauri/src/mtp/CLAUDE.md` documents that as a deliberate platform trade-off. I did not find similar
documentation for this write-operation classifier.
