export interface FileEntry {
    name: string
    path: string
    isDirectory: boolean
    isSymlink: boolean
    size?: number
    modifiedAt?: number
    createdAt?: number
    /** When the file was added to its current directory (macOS only) */
    addedAt?: number
    /** When the file was last opened (macOS only) */
    openedAt?: number
    permissions: number
    owner: string
    group: string
    iconId: string
    /** Whether extended metadata (addedAt, openedAt) has been loaded */
    extendedMetadataLoaded: boolean
}

/** Cloud sync status for files in Dropbox/iCloud/etc. folders */
export type SyncStatus = 'synced' | 'online_only' | 'uploading' | 'downloading' | 'unknown'

/**
 * Result of starting a new directory listing (synchronous).
 * The listing caches entries on the backend for on-demand fetching.
 */
export interface ListingStartResult {
    /** Unique listing ID for subsequent API calls */
    listingId: string
    /** Total number of visible entries in the directory */
    totalCount: number
    /** Maximum filename width in pixels (for Brief mode columns). None if font metrics not available. */
    maxFilenameWidth?: number
}

/**
 * Status of a streaming directory listing.
 */
export type ListingStatus = 'loading' | 'ready' | 'cancelled' | { error: string }

/**
 * Result of starting a streaming directory listing (async).
 * Returns immediately with listing ID and loading status.
 */
export interface StreamingListingStartResult {
    /** Unique listing ID for subsequent API calls */
    listingId: string
    /** Initial status (always "loading") */
    status: ListingStatus
}

/**
 * Progress event payload emitted during streaming directory listing.
 */
export interface ListingProgressEvent {
    listingId: string
    loadedCount: number
}

/**
 * Read-complete event payload emitted when read_dir finishes (before sorting/caching).
 */
export interface ListingReadCompleteEvent {
    listingId: string
    totalCount: number
}

/**
 * Completion event payload emitted when streaming directory listing finishes.
 */
export interface ListingCompleteEvent {
    listingId: string
    totalCount: number
    maxFilenameWidth?: number
    /** Root path of the volume this listing belongs to */
    volumeRoot: string
}

/**
 * Error event payload emitted when streaming directory listing fails.
 */
export interface ListingErrorEvent {
    listingId: string
    message: string
}

/**
 * Cancelled event payload emitted when streaming directory listing is cancelled.
 */
export interface ListingCancelledEvent {
    listingId: string
}

/**
 * Opening event payload emitted just before read_dir starts.
 * This is the slow part for network folders (SMB connection, directory handle creation).
 */
export interface ListingOpeningEvent {
    listingId: string
}

/**
 * A single change in a directory diff.
 */
export interface DiffChange {
    type: 'add' | 'remove' | 'modify'
    /** The affected file entry */
    entry: FileEntry
}

/**
 * Directory diff event sent from backend watcher.
 * Contains changes since last update, with monotonic sequence for ordering.
 */
export interface DirectoryDiff {
    /** Listing ID this diff belongs to */
    listingId: string
    /** Monotonic sequence number for ordering */
    sequence: number
    /** List of changes */
    changes: DiffChange[]
}

/**
 * Category of a location item.
 */
export type LocationCategory =
    | 'favorite'
    | 'main_volume'
    | 'attached_volume'
    | 'cloud_drive'
    | 'network'
    | 'mobile_device'

/**
 * Information about a location (volume, folder, or cloud drive).
 */
export interface VolumeInfo {
    /** Unique identifier for the location */
    id: string
    /** Display name (like "Macintosh HD", "Dropbox") */
    name: string
    /** Path to the location */
    path: string
    /** Category of this location */
    category: LocationCategory
    /** Base64-encoded icon (WebP format), optional */
    icon?: string
    /** Whether this can be ejected */
    isEjectable: boolean
    /** Whether this volume is read-only (for example, PTP cameras) */
    isReadOnly?: boolean
}

// ============================================================================
// Sorting types
// ============================================================================

/** Column to sort files by. Must match Rust enum. */
export type SortColumn = 'name' | 'extension' | 'size' | 'modified' | 'created'

/** Sort order. Must match Rust enum. */
export type SortOrder = 'ascending' | 'descending'

/** Default sort order for each column (first click uses this). */
export const defaultSortOrders: Record<SortColumn, SortOrder> = {
    name: 'ascending',
    extension: 'ascending',
    size: 'descending',
    modified: 'descending',
    created: 'descending',
}

/** Default sort column when opening a new directory. */
export const DEFAULT_SORT_BY: SortColumn = 'name'

/** Result of re-sorting a listing. */
export interface ResortResult {
    /** New index of the cursor file after re-sorting, if found. */
    newCursorIndex?: number
    /** New indices of previously selected files after re-sorting. */
    newSelectedIndices?: number[]
}

/** Statistics about a directory listing. */
export interface ListingStats {
    /** Total number of files (not directories) */
    totalFiles: number
    /** Total number of directories */
    totalDirs: number
    /** Total size of all files in bytes */
    totalFileSize: number
    /** Number of selected files (if selected_indices provided) */
    selectedFiles?: number
    /** Number of selected directories (if selected_indices provided) */
    selectedDirs?: number
    /** Total size of selected files in bytes (if selected_indices provided) */
    selectedFileSize?: number
}

// ============================================================================
// Network discovery types
// ============================================================================

/** State of network host discovery. */
export type DiscoveryState = 'idle' | 'searching' | 'active'

/** A discovered network host advertising SMB services. */
export interface NetworkHost {
    /** Unique identifier for the host (derived from service name) */
    id: string
    /** Display name (the advertised service name) */
    name: string
    /** Resolved hostname (like "macbook.local"), or undefined if not yet resolved */
    hostname?: string
    /** Resolved IP address, or undefined if not yet resolved */
    ipAddress?: string
    /** SMB port (usually 445) */
    port: number
}

// ============================================================================
// SMB share types
// ============================================================================

/** Information about a discovered SMB share. */
export interface ShareInfo {
    /** Name of the share (for example, "Documents", "Media") */
    name: string
    /** Whether this is a disk share (true) or other type like printer/IPC */
    isDisk: boolean
    /** Optional description/comment for the share */
    comment?: string
}

/** Authentication mode detected for a host. */
export type AuthMode = 'guest_allowed' | 'creds_required' | 'unknown'

/** Result of a share listing operation. */
export interface ShareListResult {
    /** Shares found on the host (already filtered to disk shares only) */
    shares: ShareInfo[]
    /** Authentication mode detected */
    authMode: AuthMode
    /** Whether this result came from cache */
    fromCache: boolean
}

/** Error types for share listing operations. */
export type ShareListError =
    | { type: 'host_unreachable'; message: string }
    | { type: 'timeout'; message: string }
    | { type: 'auth_required'; message: string }
    | { type: 'signing_required'; message: string }
    | { type: 'auth_failed'; message: string }
    | { type: 'protocol_error'; message: string }
    | { type: 'resolution_failed'; message: string }

// ============================================================================
// Known shares store types
// ============================================================================

/** Connection mode used for the last successful connection. */
export type ConnectionMode = 'guest' | 'credentials'

/** Authentication options available for a share. */
export type AuthOptions = 'guest_only' | 'credentials_only' | 'guest_or_credentials'

/** Information about a known network share (previously connected). */
export interface KnownNetworkShare {
    /** Hostname or IP of the server */
    serverName: string
    /** Name of the specific share */
    shareName: string
    /** Protocol type (currently only "smb") */
    protocol: string
    /** When we last successfully connected (ISO 8601) */
    lastConnectedAt: string
    /** How we connected last time */
    lastConnectionMode: ConnectionMode
    /** Auth options detected last time */
    lastKnownAuthOptions: AuthOptions
    /** Username used (null for guest) */
    username: string | null
}

// ============================================================================
// Keychain types
// ============================================================================

/** Credentials for SMB authentication. */
export interface SmbCredentials {
    /** Username for authentication */
    username: string
    /** Password for authentication */
    password: string
}

/** Error types for Keychain operations. */
export type KeychainError =
    | { type: 'not_found'; message: string }
    | { type: 'access_denied'; message: string }
    | { type: 'other'; message: string }

// ============================================================================
// Mount types
// ============================================================================

/** Result of a successful mount operation. */
export interface MountResult {
    /** Path where the share was mounted (like "/Volumes/Documents") */
    mountPath: string
    /** Whether the share was already mounted (we didn't mount it ourselves) */
    alreadyMounted: boolean
}

/** Error types for mount operations. */
export type MountError =
    | { type: 'host_unreachable'; message: string }
    | { type: 'share_not_found'; message: string }
    | { type: 'auth_required'; message: string }
    | { type: 'auth_failed'; message: string }
    | { type: 'permission_denied'; message: string }
    | { type: 'timeout'; message: string }
    | { type: 'cancelled'; message: string }
    | { type: 'protocol_error'; message: string }
    | { type: 'mount_path_conflict'; message: string }

// ============================================================================
// Write operation types
// ============================================================================

/** Type of write operation. */
export type WriteOperationType = 'copy' | 'move' | 'delete'

/** Phase of a write operation. */
export type WriteOperationPhase = 'scanning' | 'copying' | 'deleting'

/** How to handle conflicts when destination files already exist. */
export type ConflictResolution = 'stop' | 'skip' | 'overwrite' | 'rename'

/** Configuration for write operations. */
export interface WriteOperationConfig {
    /** Progress update interval in milliseconds (default: 200) */
    progressIntervalMs?: number
    /** Whether to overwrite existing files (deprecated, use conflictResolution) */
    overwrite?: boolean
    /** How to handle conflicts */
    conflictResolution?: ConflictResolution
    /** If true, only scan and detect conflicts without executing the operation */
    dryRun?: boolean
    /** Column to sort files by during copy (default: name) */
    sortColumn?: SortColumn
    /** Sort order for copy operation (default: ascending) */
    sortOrder?: SortOrder
    /** Preview scan ID to reuse cached scan results (from start_scan_preview) */
    previewId?: string | null
    /** Maximum number of conflicts to include in DryRunResult (default: 100) */
    maxConflictsToShow?: number
}

/** Result of starting a write operation. */
export interface WriteOperationStartResult {
    /** Unique operation ID for tracking and cancellation */
    operationId: string
    /** Type of operation started */
    operationType: WriteOperationType
}

/** Progress event payload for write operations. */
export interface WriteProgressEvent {
    operationId: string
    operationType: WriteOperationType
    phase: WriteOperationPhase
    /** Current file being processed (filename only, not full path) */
    currentFile: string | null
    /** Number of files processed */
    filesDone: number
    /** Total number of files */
    filesTotal: number
    /** Bytes processed so far */
    bytesDone: number
    /** Total bytes to process */
    bytesTotal: number
}

/** Completion event payload for write operations. */
export interface WriteCompleteEvent {
    operationId: string
    operationType: WriteOperationType
    filesProcessed: number
    bytesProcessed: number
}

/** Error event payload for write operations. */
export interface WriteErrorEvent {
    operationId: string
    operationType: WriteOperationType
    error: WriteOperationError
}

/** Cancelled event payload for write operations. */
export interface WriteCancelledEvent {
    operationId: string
    operationType: WriteOperationType
    /** Number of files processed before cancellation */
    filesProcessed: number
    /** Whether partial files were rolled back (deleted) */
    rolledBack: boolean
}

/** Conflict event payload (emitted when stop mode encounters a conflict). */
export interface WriteConflictEvent {
    operationId: string
    sourcePath: string
    destinationPath: string
    /** Source file size in bytes */
    sourceSize: number
    /** Destination file size in bytes */
    destinationSize: number
    /** Source modification time (Unix timestamp in seconds), if available */
    sourceModified: number | null
    /** Destination modification time (Unix timestamp in seconds), if available */
    destinationModified: number | null
    /** Whether destination is newer than source */
    destinationIsNewer: boolean
    /** Size difference (positive = destination is larger) */
    sizeDifference: number
}

/** Error types for write operations (discriminated union). */
export type WriteOperationError =
    | { type: 'source_not_found'; path: string }
    | { type: 'destination_exists'; path: string }
    | { type: 'permission_denied'; path: string; message: string }
    | { type: 'insufficient_space'; required: number; available: number; volumeName: string | null }
    | { type: 'same_location'; path: string }
    | { type: 'destination_inside_source'; source: string; destination: string }
    | { type: 'symlink_loop'; path: string }
    | { type: 'cancelled'; message: string }
    | { type: 'io_error'; path: string; message: string }

/** Progress event during scanning phase (emitted in dry-run mode). */
export interface ScanProgressEvent {
    operationId: string
    operationType: WriteOperationType
    /** Number of files found so far */
    filesFound: number
    /** Total bytes found so far */
    bytesFound: number
    /** Number of conflicts detected so far */
    conflictsFound: number
    /** Current path being scanned (for activity indication) */
    currentPath: string | null
}

/** Detailed information about a single conflict. */
export interface ConflictInfo {
    sourcePath: string
    destinationPath: string
    /** Source file size in bytes */
    sourceSize: number
    /** Destination file size in bytes */
    destinationSize: number
    /** Source modification time (Unix timestamp in seconds) */
    sourceModified: number | null
    /** Destination modification time (Unix timestamp in seconds) */
    destinationModified: number | null
    /** Whether destination is newer than source */
    destinationIsNewer: boolean
    /** Whether source is a directory */
    isDirectory: boolean
}

/** Result of a dry-run operation. */
export interface DryRunResult {
    operationId: string
    operationType: WriteOperationType
    /** Total number of files that would be processed */
    filesTotal: number
    /** Total bytes that would be processed */
    bytesTotal: number
    /** Total number of conflicts detected */
    conflictsTotal: number
    /** Sampled conflicts (max 200 for large sets) */
    conflicts: ConflictInfo[]
    /** Whether the conflicts list is a sample (true if conflictsTotal > conflicts.length) */
    conflictsSampled: boolean
}

/** Current status of an operation for query APIs. */
export interface OperationStatus {
    operationId: string
    operationType: WriteOperationType
    phase: WriteOperationPhase
    /** Whether the operation is still running */
    isRunning: boolean
    /** Current file being processed (filename only) */
    currentFile: string | null
    /** Number of files processed */
    filesDone: number
    /** Total number of files (0 if unknown/scanning) */
    filesTotal: number
    /** Bytes processed so far */
    bytesDone: number
    /** Total bytes to process (0 if unknown/scanning) */
    bytesTotal: number
    /** Operation start time (Unix timestamp in milliseconds) */
    startedAt: number
}

/** Summary of an active operation for list view. */
export interface OperationSummary {
    operationId: string
    operationType: WriteOperationType
    phase: WriteOperationPhase
    /** Percentage complete (0-100) */
    percentComplete: number
    /** Operation start time (Unix timestamp in milliseconds) */
    startedAt: number
}

// ============================================================================
// Scan preview types (for Copy dialog live stats)
// ============================================================================

/** Result of starting a scan preview. */
export interface ScanPreviewStartResult {
    previewId: string
}

/** Progress event for scan preview. */
export interface ScanPreviewProgressEvent {
    previewId: string
    filesFound: number
    dirsFound: number
    bytesFound: number
    currentPath: string | null
}

/** Completion event for scan preview. */
export interface ScanPreviewCompleteEvent {
    previewId: string
    filesTotal: number
    dirsTotal: number
    bytesTotal: number
}

/** Error event for scan preview. */
export interface ScanPreviewErrorEvent {
    previewId: string
    message: string
}

/** Cancelled event for scan preview. */
export interface ScanPreviewCancelledEvent {
    previewId: string
}
