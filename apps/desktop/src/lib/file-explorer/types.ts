import type { TagRef } from '$lib/ipc/bindings'

export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  isSymlink: boolean
  size?: number
  physicalSize?: number
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
  recursiveSize?: number
  recursivePhysicalSize?: number
  recursiveFileCount?: number
  recursiveDirCount?: number
  /** True when the subtree contains symlinks (whose content is omitted from the recursive size). */
  recursiveHasSymlinks?: boolean
  /**
   * True while the indexer still has unprocessed writes affecting this directory
   * or a descendant (a big delete/copy in flight), so its recursive size is
   * mid-update. Drives the per-row "size updating" hourglass. Carried on
   * `DirStats` and copied here by `updateIndexSizesInPlace` / `createParentEntry`
   * — NOT populated by the initial `get_file_range` render (the Rust `FileEntry`
   * deliberately doesn't carry it), so a folder navigated into mid-storm lights
   * up on the first throttled refresh rather than first paint.
   */
  recursiveSizePending?: boolean
  /**
   * Whether `recursiveSize` is an exact total (`true`) or a lower bound
   * (`false`, some subtree was never listed). Derived backend-side from the
   * subtree's coverage. Drives the `≥` lower-bound vs `—` unknown vs exact
   * size rendering in the Size column. `undefined` when not indexed yet;
   * consumers treat absent as exact. Carried on `DirStats` and copied here by
   * `updateIndexSizesInPlace` / `createParentEntry`, and set by the backend
   * `FileEntry` enrichment on first paint.
   */
  recursiveSizeComplete?: boolean
  /**
   * Whether the exact `recursiveSize` is accurate-but-stale (computed at an
   * older volume epoch than now). Only meaningful when `recursiveSizeComplete`
   * is `true`; drives the muted "stale" treatment. Absent is treated as fresh.
   */
  recursiveSizeStale?: boolean
  /**
   * When set on a virtual entry, the frontend navigates to this path instead
   * of treating the entry as a normal directory listing. Currently set on
   * `worktrees/` and `submodules/` entries inside the git portal. Lives on
   * the base `FileEntry` schema so every consumer carries it for free.
   */
  redirectToPath?: string
  /**
   * Loose Size-column override for virtual git entries, for example:
   * `+12 / -3`, `5 files`, `12 items`, `on main`, a short SHA. When set,
   * the Size cell renders this string verbatim instead of formatted
   * bytes from `size`. Cross-category Size sorting is meaningless on
   * purpose; each cell is self-explaining via tooltip + aria-label.
   */
  displaySize?: string
  /**
   * Long-form tooltip for the Size cell when `displaySize` is set.
   * Example: "12 commits ahead, 3 commits behind `origin/main`".
   * Doubles as the aria-label for screen readers.
   */
  displaySizeTooltip?: string
  /**
   * Parent directory path. Optional on FileEntry because normal directory
   * listings derive it implicitly from the containing folder, but search-results
   * snapshots carry it per row so the optional Path column in FullList can
   * shrink-wrap and the path-pills renderer has data to display. Always set
   * when `SearchResultEntry` is adapted into a FileEntry for the search-results
   * pane; absent for entries fetched from the backend listing cache.
   */
  parentPath?: string
  /**
   * macOS Finder tags (`com.apple.metadata:_kMDItemUserTags`). Empty in the
   * core listing; filled by the deferred, visible-range-first `enrich_tags`
   * pass and the post-load background sweep. Optional here so synthetic
   * entries (the `..` row, search-results adapters) don't have to set it;
   * `TagDots` treats absent as none.
   */
  tags?: TagRef[]
}

/** Cloud sync status for files in Dropbox/iCloud/etc. folders */
export type SyncStatus = 'synced' | 'online_only' | 'uploading' | 'downloading' | 'unknown'

/**
 * Status of a streaming directory listing.
 * Serialized as a tagged object by Rust (e.g. `{ status: "loading" }`).
 */
export type ListingStatus =
  | { status: 'loading' }
  | { status: 'ready' }
  | { status: 'cancelled' }
  | { status: 'error'; message: string }

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

// Streaming-listing event payload types (`ListingProgressEvent`,
// `ListingReadCompleteEvent`, `ListingCompleteEvent`, `ListingErrorEvent`,
// `ListingCancelledEvent`, `ListingOpeningEvent`) are now generated by
// tauri-specta. Import them from `$lib/tauri-commands`.

/** Action kind for errors that require a specific user action (mirrors Rust `ErrorActionKind`). */
export type ErrorActionKind = 'open_privacy_settings'

/**
 * Rendered, displayable error copy for `ErrorPane`. The backend ships a typed,
 * word-free `ListingError`; `lib/errors/listing-error.ts::renderListingError`
 * composes this shape (picking the words from the FE factories, escaping runtime
 * params inside them). `explanation` / `suggestion` are trusted markdown rendered
 * through `renderErrorMarkdown` → `snarkdown`.
 */
export interface FriendlyError {
  category: 'transient' | 'needs_action' | 'serious'
  title: string
  /** Trusted markdown (runtime params already escaped by the FE factories). */
  explanation: string
  suggestion: string
  rawDetail: string
  retryHint: boolean
  actionKind?: ErrorActionKind | null
}

/**
 * A single change in a directory diff.
 */
export interface DiffChange {
  type: 'add' | 'remove' | 'modify'
  /** The affected file entry */
  entry: FileEntry
  /** Position in the sorted listing: old listing for `remove`, new listing for `add`/`modify`. */
  index: number
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

/** Sent when the watched directory itself is deleted. */
export interface DirectoryDeletedEvent {
  listingId: string
  path: string
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
 * SMB connection quality. `direct` = smb2 session active, `os_mount` = OS mount
 * fallback, `disconnected` = SmbVolume exists but its smb2 session is broken
 * (the reconnect manager runs the recovery cycle). Non-SMB volumes carry no
 * value at all.
 */
export type SmbConnectionState = 'direct' | 'os_mount' | 'disconnected'

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
  /** Whether this volume is a mounted disk image (.dmg): no indexing affordances, no space bars. */
  isDiskImage?: boolean
  /** Filesystem type from statfs (for example, "apfs", "smbfs", "exfat") */
  fsType?: string
  /** Whether this volume supports macOS trash. `undefined` means unknown (treat as `true`). */
  supportsTrash?: boolean
  /** SMB connection state. Only set for volumes with an active SmbVolume in the backend. */
  smbConnectionState?: SmbConnectionState
  /** Negotiated USB link speed. Only set for MTP/mobile volumes. */
  usbSpeed?: UsbSpeed
}

/**
 * Negotiated USB link speed (slowest of host port, cable, and device).
 * Mirrors the Rust `UsbSpeed` enum from `bindings.ts`.
 */
export type UsbSpeed = 'low' | 'full' | 'high' | 'super' | 'super_plus'

/** Display label and theoretical max for a `UsbSpeed`. */
export interface UsbSpeedDisplay {
  /** The raw tier identifier, useful for CSS class names (`usb-speed-indicator-{tier}`). */
  tier: UsbSpeed
  /** Generation name, e.g. "USB 3.2 Gen 1". */
  label: string
  /** Theoretical maximum throughput in MB/s (1 MB/s = 10^6 B/s, matching marketing). */
  maxMBps: number
}

/**
 * Map a `UsbSpeed` to its display label + theoretical max MB/s.
 * Values follow USB-IF marketing: divide raw line rate by 8 (decimal MB).
 */
export function describeUsbSpeed(speed: UsbSpeed): UsbSpeedDisplay {
  switch (speed) {
    case 'low':
      return { tier: 'low', label: 'USB 1.0 low-speed', maxMBps: 0.2 }
    case 'full':
      return { tier: 'full', label: 'USB 1.1 full-speed', maxMBps: 1.5 }
    case 'high':
      return { tier: 'high', label: 'USB 2.0', maxMBps: 60 }
    case 'super':
      return { tier: 'super', label: 'USB 3.2 Gen 1', maxMBps: 625 }
    case 'super_plus':
      return { tier: 'super_plus', label: 'USB 3.2 Gen 2', maxMBps: 1250 }
  }
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
  newCursorIndex: number | null
  /** New indices of previously selected files after re-sorting. */
  newSelectedIndices: number[] | null
}

/** Statistics about a directory listing. */
export interface ListingStats {
  /** Total number of files (not directories) */
  totalFiles: number
  /** Total number of directories */
  totalDirs: number
  /** Total logical size in bytes (files + directory recursive sizes) */
  totalSize: number
  /** Total physical (on-disk) size in bytes */
  totalPhysicalSize: number
  /** Number of selected files (if selected_indices provided) */
  selectedFiles: number | null
  /** Number of selected directories (if selected_indices provided) */
  selectedDirs: number | null
  /** Total logical size of selected entries in bytes (if selected_indices provided) */
  selectedSize: number | null
  /** Total physical size of selected entries in bytes (if selected_indices provided) */
  selectedPhysicalSize: number | null
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
  /** How this host was added: mDNS discovery or manual user entry */
  source?: 'discovered' | 'manual'
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
  comment: string | null
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
  | { type: 'missing_dependency'; message: string; installCommand: string | null }

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
// Mount types
// ============================================================================

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
export type WriteOperationType = 'copy' | 'move' | 'delete' | 'trash'

/** Transfer operations (copy or move): subset of write operations that share UI. */
export type TransferOperationType = 'copy' | 'move' | 'delete' | 'trash'

/** Phase of a write operation. */
export type WriteOperationPhase = 'scanning' | 'copying' | 'deleting' | 'trashing' | 'rolling_back' | 'flushing'

/** How to handle conflicts when destination files already exist. */
export type ConflictResolution = 'stop' | 'skip' | 'overwrite' | 'rename' | 'overwrite_smaller' | 'overwrite_older'

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
  /** Source filenames already known to conflict at the destination (from the pre-flight
   *  `scanVolumeForConflicts` call). When `conflictResolution` is `'skip'`, the backend
   *  bulk-skips these upfront so the progress bar reflects them immediately. Ignored
   *  for other resolution modes. */
  preKnownConflicts?: string[]
}

/** Result of starting a write operation. */
export interface WriteOperationStartResult {
  /** Unique operation ID for tracking and cancellation */
  operationId: string
  /** Type of operation started */
  operationType: WriteOperationType
}

// The write-operations event payload types (`WriteProgressEvent`,
// `WriteCompleteEvent`, `WriteErrorEvent`, `WriteSourceItemDoneEvent`,
// `WriteCancelledEvent`, `WriteSettledEvent`, `WriteConflictEvent`,
// `ScanProgressEvent`, `ConflictInfo`, `DryRunResult`, `OperationStatus`,
// `OperationSummary`) plus the scan-preview event payloads
// (`ScanPreviewProgressEvent` / `Complete` / `Error` / `Cancelled`) are now
// generated by tauri-specta. Import them from `$lib/tauri-commands`.

/** A file that exceeds the destination filesystem's per-file size limit. */
export interface OversizedFile {
  name: string
  size: number
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
  | { type: 'device_disconnected'; path: string }
  | { type: 'read_only_device'; path: string; deviceName: string | null }
  | { type: 'file_locked'; path: string }
  | { type: 'trash_not_supported'; path: string }
  | { type: 'connection_interrupted'; path: string }
  | { type: 'read_error'; path: string; message: string }
  | { type: 'write_error'; path: string; message: string }
  | { type: 'name_too_long'; path: string }
  | { type: 'invalid_name'; path: string; message: string }
  | { type: 'delete_pending'; path: string }
  | {
      type: 'files_too_large_for_filesystem'
      /** The destination filesystem kind (snake_case tag, e.g. 'fat32'). */
      filesystem: string
      maxSize: number
      files: OversizedFile[]
      totalCount: number
    }
  | { type: 'io_error'; path: string; message: string }

// ============================================================================
// Scan preview types (for Copy dialog live stats)
// ============================================================================

/** Result of starting a scan preview. */
export interface ScanPreviewStartResult {
  previewId: string
}

/** Cached scan-preview totals (returned by `checkScanPreviewStatus`). */
export interface ScanPreviewTotals {
  filesTotal: number
  dirsTotal: number
  bytesTotal: number
  /** `du`-equivalent source footprint (hardlinks counted once). */
  dedupBytesTotal: number
}
