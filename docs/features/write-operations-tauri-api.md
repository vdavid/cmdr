# Write operations API

This document covers the frontend API for copy, move, and delete operations.

## Quick start

```ts
import {
    copyFiles,
    moveFiles,
    deleteFiles,
    cancelWriteOperation,
    onOperationEvents,
    calculateOperationStats,
    formatBytes,
    formatDuration,
} from '$lib/tauri-commands'

// Start a copy operation
const startTime = Date.now()
const result = await copyFiles(['/path/to/file.txt'], '/destination/folder')

// Subscribe to events for this specific operation
const unlisten = await onOperationEvents(result.operationId, {
    onProgress: (e) => {
        const stats = calculateOperationStats(e, startTime)
        console.log(`${stats.percentComplete.toFixed(0)}% - ${formatBytes(stats.bytesPerSecond)}/s`)
        if (stats.estimatedSecondsRemaining) {
            console.log(`ETA: ${formatDuration(stats.estimatedSecondsRemaining)}`)
        }
    },
    onComplete: (e) => console.log(`Done! Copied ${e.filesProcessed} files`),
    onError: (e) => console.error('Failed:', e.error),
})

// Clean up when done (or on component unmount)
unlisten()
```

## Commands

### `copyFiles(sources, destination, config?)`

Starts a background copy operation.

```ts
const result = await copyFiles(
    ['/Users/me/Documents/file.txt', '/Users/me/Documents/folder'],
    '/Volumes/USB/backup',
    { conflictResolution: 'skip' }
)
// result.operationId: "550e8400-e29b-41d4-a716-446655440000"
// result.operationType: "copy"
```

### `moveFiles(sources, destination, config?)`

Starts a background move operation. Same-filesystem moves are instant (rename). Cross-filesystem moves use copy+delete.

```ts
const result = await moveFiles(
    ['/Users/me/Downloads/archive.zip'],
    '/Users/me/Documents',
)
```

### `deleteFiles(sources, config?)`

Starts a background delete operation. Recursively deletes directories.

```ts
const result = await deleteFiles(['/Users/me/Downloads/old-folder'])
```

### `cancelWriteOperation(operationId)`

Cancels an in-progress operation. The operation will emit a `write-cancelled` event when it stops.

```ts
await cancelWriteOperation(result.operationId)
```

### `resolveWriteConflict(operationId, resolution, applyToAll)`

Resolves a pending conflict when using `conflictResolution: 'stop'` mode.

```ts
// User chose to skip this file
await resolveWriteConflict(operationId, 'skip', false)

// User chose to overwrite all remaining conflicts
await resolveWriteConflict(operationId, 'overwrite', true)
```

### `listActiveOperations()`

Returns summaries of all running operations. Useful for a global progress indicator.

```ts
const operations = await listActiveOperations()
// [{ operationId, operationType, phase, percentComplete, startedAt }]
```

### `getOperationStatus(operationId)`

Gets detailed status of a specific operation.

```ts
const status = await getOperationStatus(operationId)
if (status?.isRunning) {
    console.log(`Processing: ${status.currentFile}`)
}
```

## Configuration

All write commands accept an optional `WriteOperationConfig`:

```ts
interface WriteOperationConfig {
    /** Progress update interval in milliseconds (default: 200) */
    progressIntervalMs?: number

    /** How to handle conflicts */
    conflictResolution?: 'stop' | 'skip' | 'overwrite' | 'rename'

    /** If true, only scan and detect conflicts without executing */
    dryRun?: boolean
}
```

### Conflict resolution modes

| Mode | Behavior |
|------|----------|
| `stop` | Pause and emit `write-conflict` event. Wait for `resolveWriteConflict()` |
| `skip` | Skip conflicting files silently |
| `overwrite` | Replace destination files |
| `rename` | Rename source as "file (1).txt", "file (2).txt", etc. |

## Events

### Subscribing to a single operation

Use `onOperationEvents()` to subscribe to all events for a specific operation:

```ts
const unlisten = await onOperationEvents(operationId, {
    onProgress: (event) => { /* update UI */ },
    onComplete: (event) => { /* show success */ },
    onError: (event) => { /* show error */ },
    onCancelled: (event) => { /* handle cancellation */ },
    onConflict: (event) => { /* show conflict dialog */ },
})
```

The returned `unlisten` function cleans up all subscriptions at once.

### Global event listeners

For monitoring all operations (e.g., a system-wide progress panel):

```ts
import { onWriteProgress, onWriteComplete, onWriteError } from '$lib/tauri-commands'

const unlistenProgress = await onWriteProgress((event) => {
    console.log(`Operation ${event.operationId}: ${event.filesDone}/${event.filesTotal}`)
})
```

### Event types

#### `WriteProgressEvent`

Emitted every 200ms (configurable) during operation.

```ts
interface WriteProgressEvent {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    phase: 'scanning' | 'copying' | 'deleting'
    currentFile: string | null  // Filename only, not full path
    filesDone: number
    filesTotal: number
    bytesDone: number
    bytesTotal: number
}
```

#### `WriteCompleteEvent`

Emitted when operation finishes successfully.

```ts
interface WriteCompleteEvent {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    filesProcessed: number
    bytesProcessed: number
}
```

#### `WriteErrorEvent`

Emitted when operation fails.

```ts
interface WriteErrorEvent {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    error: WriteOperationError
}
```

#### `WriteCancelledEvent`

Emitted when operation is cancelled.

```ts
interface WriteCancelledEvent {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    filesProcessed: number  // Files completed before cancellation
}
```

#### `WriteConflictEvent`

Emitted when `conflictResolution: 'stop'` encounters a conflict.

```ts
interface WriteConflictEvent {
    operationId: string
    sourcePath: string
    destinationPath: string
    destinationIsNewer: boolean
    sizeDifference: number  // Positive = destination is larger
}
```

## Error handling

Errors are discriminated unions with a `type` field:

```ts
type WriteOperationError =
    | { type: 'source_not_found'; path: string }
    | { type: 'destination_exists'; path: string }
    | { type: 'permission_denied'; path: string; message: string }
    | { type: 'insufficient_space'; required: number; available: number; volumeName: string | null }
    | { type: 'same_location'; path: string }
    | { type: 'destination_inside_source'; source: string; destination: string }
    | { type: 'symlink_loop'; path: string }
    | { type: 'cancelled'; message: string }
    | { type: 'io_error'; path: string; message: string }
```

Example error handling:

```ts
onOperationEvents(operationId, {
    onError: (event) => {
        switch (event.error.type) {
            case 'permission_denied':
                showToast(`Permission denied: ${event.error.path}`)
                break
            case 'insufficient_space':
                const needed = formatBytes(event.error.required)
                const available = formatBytes(event.error.available)
                showToast(`Not enough space. Need ${needed}, have ${available}`)
                break
            case 'destination_inside_source':
                showToast("Can't copy a folder into itself")
                break
            default:
                showToast(`Error: ${event.error.type}`)
        }
    },
})
```

## Progress utilities

### `calculateOperationStats(event, startTime)`

Derives useful statistics from progress events:

```ts
interface WriteOperationStats {
    percentComplete: number          // 0-100
    bytesPerSecond: number           // Transfer speed
    estimatedSecondsRemaining: number | null  // ETA
    elapsedSeconds: number           // Time since start
}
```

Usage:

```ts
let startTime: number

async function startCopy() {
    startTime = Date.now()
    const result = await copyFiles(sources, destination)

    await onOperationEvents(result.operationId, {
        onProgress: (e) => {
            const stats = calculateOperationStats(e, startTime)
            progressBar.value = stats.percentComplete
            speedLabel.text = `${formatBytes(stats.bytesPerSecond)}/s`
            if (stats.estimatedSecondsRemaining !== null) {
                etaLabel.text = formatDuration(stats.estimatedSecondsRemaining)
            }
        },
    })
}
```

### `formatBytes(bytes)`

Formats bytes as human-readable string.

```ts
formatBytes(1024)        // "1.0 KB"
formatBytes(1536000)     // "1.5 MB"
formatBytes(2147483648)  // "2.0 GB"
```

### `formatDuration(seconds)`

Formats seconds as human-readable duration.

```ts
formatDuration(45)    // "45s"
formatDuration(90)    // "1m 30s"
formatDuration(3720)  // "1h 2m"
```

## Dry-run mode

Use dry-run to preview an operation and detect conflicts before executing:

```ts
// Start dry-run
const result = await copyFiles(sources, destination, { dryRun: true })

await onOperationEvents(result.operationId, {
    onScanProgress: (e) => {
        console.log(`Scanning: ${e.filesFound} files, ${e.conflictsFound} conflicts`)
    },
    onScanConflict: (conflict) => {
        // Individual conflicts as they're found
        console.log(`Conflict: ${conflict.sourcePath}`)
    },
    onDryRunComplete: (result) => {
        console.log(`Would process ${result.filesTotal} files (${formatBytes(result.bytesTotal)})`)
        console.log(`${result.conflictsTotal} conflicts found`)

        // Show conflict preview (max 200 sampled)
        for (const conflict of result.conflicts) {
            console.log(`  ${conflict.sourcePath} → ${conflict.destinationPath}`)
        }

        // If there were more than 200, result.conflictsSampled is true
        if (result.conflictsSampled) {
            console.log(`  ... and ${result.conflictsTotal - result.conflicts.length} more`)
        }
    },
})
```

### Dry-run types

```ts
interface ScanProgressEvent {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    filesFound: number
    bytesFound: number
    conflictsFound: number
    currentPath: string | null
}

interface ConflictInfo {
    sourcePath: string
    destinationPath: string
    sourceSize: number
    destinationSize: number
    sourceModified: number | null      // Unix timestamp (seconds)
    destinationModified: number | null
    destinationIsNewer: boolean
    isDirectory: boolean
}

interface DryRunResult {
    operationId: string
    operationType: 'copy' | 'move' | 'delete'
    filesTotal: number
    bytesTotal: number
    conflictsTotal: number
    conflicts: ConflictInfo[]  // Max 200 sampled
    conflictsSampled: boolean  // True if conflictsTotal > conflicts.length
}
```

## Example: Full copy dialog

```ts
import { copyFiles, onOperationEvents, cancelWriteOperation, calculateOperationStats, formatBytes, formatDuration } from '$lib/tauri-commands'

let operationId: string | null = null
let unlisten: (() => void) | null = null
let startTime: number

async function startCopy(sources: string[], destination: string) {
    startTime = Date.now()

    const result = await copyFiles(sources, destination, {
        conflictResolution: 'stop',  // Ask user on conflicts
    })
    operationId = result.operationId

    unlisten = await onOperationEvents(operationId, {
        onProgress: (e) => {
            const stats = calculateOperationStats(e, startTime)
            updateUI({
                currentFile: e.currentFile,
                progress: stats.percentComplete,
                speed: `${formatBytes(stats.bytesPerSecond)}/s`,
                eta: stats.estimatedSecondsRemaining
                    ? formatDuration(stats.estimatedSecondsRemaining)
                    : 'Calculating...',
                status: `${e.filesDone} of ${e.filesTotal} files`,
            })
        },
        onComplete: (e) => {
            showSuccess(`Copied ${e.filesProcessed} files`)
            cleanup()
        },
        onError: (e) => {
            showError(e.error)
            cleanup()
        },
        onCancelled: (e) => {
            showInfo(`Cancelled. ${e.filesProcessed} files were copied.`)
            cleanup()
        },
        onConflict: (e) => {
            showConflictDialog(e)  // Let user choose skip/overwrite/rename
        },
    })
}

async function handleCancel() {
    if (operationId) {
        await cancelWriteOperation(operationId)
    }
}

function cleanup() {
    unlisten?.()
    unlisten = null
    operationId = null
}
```
