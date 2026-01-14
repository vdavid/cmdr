# Write actions (copy, move, delete)

File operations with streaming progress reporting and cancellation support.

## Overview

Write operations run in background tasks and emit progress events at configurable intervals (default: 200ms). All
operations support:

- Batch processing (multiple source files)
- Cancellation at any time
- Progress tracking (files and bytes)

## Event flow diagram

```
Frontend                          Rust Backend
   |                                   |
   |-- copyFiles(sources, dest) ------>|
   |<-- { operationId, type: copy }    | (immediate return)
   |                                   |
   |                                   | (background task starts)
   |                                   |
   |<---- write-progress event --------| (every 200ms)
   |     { operationId, phase,         |
   |       currentFile, filesDone,     |
   |       filesTotal, bytesDone,      |
   |       bytesTotal }                |
   |                                   |
   |<---- write-complete event --------|
   |     { operationId, type,          |
   |       filesProcessed,             |
   |       bytesProcessed }            |
   |                                   |
```

### Error handling

```
Frontend                          Rust Backend
   |                                   |
   |<---- write-error event -----------|
   |     { operationId, type, error }  |
   |                                   |
```

### Cancellation

```
Frontend                          Rust Backend
   |                                   |
   |-- cancelWriteOperation(id) ------>|
   |                                   |
   |<---- write-cancelled event -------|
   |     { operationId, type,          |
   |       filesProcessed }            |
   |                                   |
```

## Operation phases

Each operation has distinct phases for accurate progress reporting:

| Phase     | Description                                |
| --------- | ------------------------------------------ |
| scanning  | Counting files and calculating total bytes |
| copying   | Copying files (copy and cross-FS move)     |
| deleting  | Deleting files                             |

## Operation details

### Copy

1. **Scan phase**: Walk source tree, count files and total bytes
2. **Copy phase**: Copy files one by one with progress
3. Uses APFS clonefile automatically when available (handled by `std::fs::copy`)

Copy-on-write on APFS: When copying files on the same APFS volume, `std::fs::copy` uses the `clonefile` system call,
which is nearly instant and uses no additional disk space until files are modified.

### Move

1. Check if source and destination are on the same filesystem (via `metadata.dev()`)
2. **Same filesystem**: Use `rename()` system call - instant, no progress needed
3. **Different filesystem**: Fall back to copy + delete with progress

For an 8 GB folder with 1 million files on the same filesystem, move completes in microseconds.

### Delete

1. **Scan phase**: Walk source tree, count files
2. **Delete phase**: Delete files one by one (O(n) unlink calls)
3. Remove empty directories bottom-up

## Tauri commands

```typescript
// Start a copy operation (async, returns immediately)
copyFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult>

// Start a move operation (async, returns immediately)
moveFiles(
  sources: string[],
  destination: string,
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult>

// Start a delete operation (async, returns immediately)
deleteFiles(
  sources: string[],
  config?: WriteOperationConfig
): Promise<WriteOperationStartResult>

// Cancel an in-progress operation
cancelWriteOperation(operationId: string): void
```

## Configuration

| Option             | Type    | Default | Description                   |
| ------------------ | ------- | ------- | ----------------------------- |
| progressIntervalMs | number  | 200     | Progress event interval in ms |
| overwrite          | boolean | false   | Overwrite existing files      |

Example:

```typescript
await copyFiles(['/path/to/source'], '/path/to/dest', {
	progressIntervalMs: 100,
	overwrite: true
});
```

## Error types

| Type                     | Cause                                      |
| ------------------------ | ------------------------------------------ |
| source_not_found         | Source path doesn't exist                  |
| destination_exists       | Destination exists and overwrite=false     |
| permission_denied        | No permission to read source or write dest |
| insufficient_space       | Not enough space on destination filesystem |
| same_location            | Source and destination are the same        |
| destination_inside_source| Destination is inside source tree          |
| cancelled                | User cancelled the operation               |
| io_error                 | Generic I/O error                          |

Error payloads are tagged unions for easy frontend handling:

```json
{
  "type": "permission_denied",
  "path": "/protected/file.txt",
  "message": "Access denied"
}
```

## Progress event structure

```typescript
interface WriteProgressEvent {
	operationId: string;
	operationType: 'copy' | 'move' | 'delete';
	phase: 'scanning' | 'copying' | 'deleting';
	currentFile: string | null; // Filename being processed
	filesDone: number;
	filesTotal: number;
	bytesDone: number;
	bytesTotal: number;
}
```

## Implementation

### Backend

- **Module**: `apps/desktop/src-tauri/src/file_system/write_operations.rs`
- **Commands**: `apps/desktop/src-tauri/src/commands/file_system.rs`
- **Tests**: `apps/desktop/src-tauri/src/file_system/write_operations_test.rs`

Key implementation details:

- Uses `tokio::spawn` + `spawn_blocking` for non-blocking I/O
- Progress state stored in global `WRITE_OPERATION_STATE` cache
- Cancellation via `AtomicBool` flag checked during iteration
- Same-filesystem detection via `std::os::unix::fs::MetadataExt::dev()`

### Frontend

Events can be listened to using Tauri's event system:

```typescript
import { listen } from '@tauri-apps/api/event';

const unlisten = await listen('write-progress', (event) => {
	const progress = event.payload as WriteProgressEvent;
	updateProgressUI(progress);
});
```

## Performance characteristics

| Scenario                        | Behavior                               |
| ------------------------------- | -------------------------------------- |
| Move on same filesystem         | Instant (rename syscall)               |
| Copy on same APFS volume        | Near-instant (clonefile)               |
| Copy to different filesystem    | Byte-by-byte copy with progress        |
| Delete large directory          | O(n) where n = file count              |
| Many small files                | Progress visible every 200ms           |
| User cancellation               | Stops at next file boundary            |
