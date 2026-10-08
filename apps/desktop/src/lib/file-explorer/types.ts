import type { CompressedSizeEstimate, DeviceReadiness, GitEntryMeta, TagRef } from '$lib/ipc/bindings'

export type { DeviceReadiness } from '$lib/ipc/bindings'

// Two vocabularies live beside their owners and are re-exported here, so every
// consumer keeps importing from this one seam.
export type * from './network/types'
export type * from '../file-operations/write-operation-types'

export interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  isSymlink: boolean
  /**
   * True when this is a file whose extension marks it as a supported archive
   * (a `.zip` today), computed backend-side at listing time (extension-only, no
   * per-file byte read). Drives the Enter fork: an archive file navigates INTO
   * itself like a folder rather than opening in the OS default app. `isDirectory`
   * stays `false` — an archive is a file that browses like a folder. Optional
   * here so synthetic entries (the `..` row, search-results adapters) can omit it.
   */
  isArchive?: boolean
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
   * What a virtual git entry's Size cell states, as a fact rather than a
   * sentence: an ahead/behind pair, a count, a pinned commit. `wordGitMeta`
   * in `views/full-list-utils.ts` turns it into the cell text plus the tooltip
   * that doubles as the aria-label, so it reads in the user's own language
   * with that language's plural rules. Cross-category Size sorting is
   * meaningless on purpose.
   */
  gitMeta?: GitEntryMeta
  /**
   * The file's bytes sit in cold storage (S3 Glacier Flexible Retrieval or Deep
   * Archive) and can't be read until someone restores them. The row shows an
   * "archived" glyph; a read answers `coldStorage`. Set by the backend listing;
   * optional so synthetic entries can omit it.
   */
  inColdStorage?: boolean
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
  /**
   * True when this entry should be hidden unless "show hidden files" is on:
   * a dotfile, a macOS `UF_HIDDEN` entry, or a name listed in a volume root's
   * `/.hidden` (see `cmdr-fs::FileEntry::is_hidden`). Drives the dimmed name
   * treatment in `views/FullList.svelte` / `BriefList.svelte`. Optional here
   * so synthetic entries (the `..` row, search-results adapters) don't have
   * to set it; absent renders like `false` (never dimmed).
   */
  isHidden?: boolean
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
 * word-free `ListingError`; `lib/error-messages/listing-error.ts::renderListingError`
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
 *
 * `move` is a row that kept existing but changed its sorted position (its mtime
 * or size changed under a sort that reads it). It carries the fresh entry, so it
 * subsumes `modify`, and it's what lets the cursor and the selection follow the
 * row they were on instead of staying on an index that now holds someone else.
 */
export interface DiffChange {
  type: 'add' | 'remove' | 'modify' | 'move'
  /** The affected file entry */
  entry: FileEntry
  /** Position in the sorted listing: old listing for `remove`, new listing for the rest. */
  index: number
  /** Where the row sat before it moved. Set exactly on `move`. */
  previousIndex?: number | null
}

/**
 * Directory diff event sent from backend watcher.
 * Contains changes since last update, with monotonic sequence for ordering.
 */
export interface DirectoryDiff {
  /** Listing ID this diff belongs to */
  listingId: string
  /** Each transition keeps its own old/new index spaces. */
  batches: DirectoryDiffBatch[]
}

export interface DirectoryDiffBatch {
  fromSequence: number
  sequence: number
  totalCount: number
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
 * How live a remote volume's session is. Mirrors Rust's `ConnectionState`.
 *
 * `direct` = a live session Cmdr owns (smb2, SFTP, WebDAV, S3, a dialed phone),
 * `os_mount` = SMB's kernel-mount fallback, `disconnected` = the session dropped
 * and the backoff loop owns recovery, `needs_sign_in` = the backend stopped
 * retrying because a credential is missing, `needs_host_key_approval` = SFTP's
 * server key isn't the trusted one, `saved` = a pinned place that isn't connected
 * and has nothing in flight. A local disk, a favorite, and the hub row carry no
 * value at all.
 *
 * ❌ Never test this with `!= null` — that used to mean "is this SMB" and no
 * longer answers anything. Use the named predicates in
 * `navigation/connection-state.ts`, and `pane/volume-capabilities.ts` for which
 * KIND of volume this is.
 */
export type ConnectionState =
  | 'direct'
  | 'os_mount'
  | 'disconnected'
  | 'needs_sign_in'
  | 'needs_host_key_approval'
  | 'saved'

/**
 * What the registered backend for a volume can do, straight from Rust's
 * `Volume::capabilities()`. Mirrors the `VolumeCapabilities` type in
 * `bindings.ts`; the canonical answer for each field lives on the `Volume`
 * trait, so ❌ never re-derive one of these from an id, an `fsType`, or a
 * category.
 *
 * Absent when no backend is registered for the volume (a favorite, or a volume
 * discovery found before registration). Consumers fold it over the per-kind
 * defaults via `pane/volume-capabilities.ts`.
 */
export interface VolumeBackendCapabilities {
  /** Files and folders can be created, renamed, and deleted here. */
  backendCanWrite: boolean
  /** Files can be read out of here, so this volume can be the SOURCE of a copy or a move. */
  canExport: boolean
  /** A drive index can be turned on for this volume: the index has a way to walk and watch this backend. */
  canBeIndexed: boolean
  /** "Copy share link" can mint a link to a file here (S3's presigned GET). */
  canShareLinks: boolean
  /** Some entries here rename by copying on the server (S3), so a move within the volume scans. */
  renamesCanCopy: boolean
  /** The same place can also be reached through the OS's own mount (SMB), so a live session is the "direct" one of two. */
  hasOsMountFallback: boolean
}

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
  /** Unmodified A–Z key that opens this favorite while the favorites menu is visible. */
  favoriteShortcut?: string | null
  /** Base64-encoded icon (WebP format), optional */
  icon?: string
  /** Whether this can be ejected */
  isEjectable: boolean
  /** Whether this volume is read-only (for example, PTP cameras) */
  mountIsReadOnly?: boolean
  /** Whether this volume is a mounted disk image (.dmg): no indexing affordances, no space bars. */
  isDiskImage?: boolean
  /**
   * Whether a cloud provider's own filesystem serves this mount (pCloud's `pcloudfs`, CloudMounter, …),
   * so every entry read costs a round trip to that provider's daemon: grouped under CLOUD, and no
   * indexing affordances. ❗ `false` for a `~/Library/CloudStorage` folder, which is an ordinary
   * directory on the data volume and stays indexable.
   */
  isCloudMount?: boolean
  /** Filesystem type from statfs (for example, "apfs", "smbfs", "exfat") */
  fsType?: string
  /** Whether this volume supports macOS trash. `undefined` means unknown (treat as `true`). */
  supportsTrash?: boolean
  /**
   * How live this volume's session is. Set for every volume a connecting backend
   * serves (SMB, SFTP, WebDAV, S3, ADB) plus a saved-but-unconnected server.
   * ❌ Never an "is this SMB" test: `navigation/connection-state.ts` has the
   * predicates, `pane/volume-capabilities.ts` has the kind.
   */
  connectionState?: ConnectionState | null
  /**
   * Whether this place belongs in the volume SWITCHER: the user's own cap on how many saved things crowd their disks.
   * Set only on a server place, which is the only row the cap applies to; absent on a local disk, a favorite, and a
   * mounted SMB share, all of which show unconditionally.
   *
   * ❗ The listing publishes every saved place whatever this says, because a
   * volume id with no row is one the app denies exists.
   * `navigation/volume-grouping.ts` is where the cap is applied.
   */
  pinned?: boolean | null
  /**
   * Where opening this place lands when that isn't `path`: a server place's saved start folder, as an app path.
   * Absent or `null` lands at `path`. ❗ Minted in Rust (`server_volumes.rs`), ❌ never derived here, so the switcher
   * and the pane read one spelling of it.
   */
  landingPath?: string | null
  /** An SMB share's own name, for a tab at its root: its mount dir may be `public-1`. Minted in Rust. */
  rootLabel?: string | null
  /** The account a mounted SMB share is signed in as NOW (`GUEST` for guest), off the mount table. */
  mountAccount?: string | null
  /**
   * Whether the DEVICE behind this row is reachable, a different question from how live a session is. Set by the
   * device providers only: a phone waiting for its "Allow USB debugging?" tap is present, and must never start a
   * reconnect backoff.
   */
  deviceReadiness?: DeviceReadiness | null
  /** Negotiated USB link speed. Only set for MTP/mobile volumes. */
  usbSpeed?: UsbSpeed
  /** What the registered backend can do. Absent when no backend is registered for this id. */
  capabilities?: VolumeBackendCapabilities | null
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
  sequence: number
  totalCount: number
  /** New index of the cursor file after re-sorting, if found. */
  newCursorIndex: number | null
  /** New indices of previously selected files after re-sorting. */
  newSelectedIndices: number[] | null
}

// ============================================================================
// List-view callback payloads
// ============================================================================

/**
 * A row click/context-menu selection, threaded from `BriefList` / `FullList` /
 * `SearchResultsView`'s `onSelect` prop down through `PanePointer.handleSelect`.
 * `shiftKey` extends the range from the cursor; `metaKey` toggles the row (Shift
 * wins when both are held). Both are omitted, not `false`, for a plain click or
 * a context-menu select.
 */
export interface SelectPayload {
  index: number
  shiftKey?: boolean
  metaKey?: boolean
}

/**
 * The virtualized window's visible rows, threaded from `BriefList` / `FullList`
 * up through `SearchResultsView`'s pass-through `onVisibleRangeChange` prop to
 * the pane, which uses it for MCP state sync.
 */
export interface VisibleRangePayload {
  start: number
  end: number
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
// Scan preview types (for Copy dialog live stats)
// ============================================================================

/** Result of starting a scan preview. */
export interface ScanPreviewStartResult {
  previewId: string
}

/** Cached scan-preview totals (returned by `checkScanPreviewStatus`). Mirrors the
 *  generated `ScanPreviewTotals` binding. */
export interface ScanPreviewTotals {
  filesTotal: number
  dirsTotal: number
  bytesTotal: number
  /** `du`-equivalent source footprint (hardlinks counted once). */
  dedupBytesTotal: number
  /** Estimated compressed size (Compress mode, local sources only); `null`
   *  otherwise. Mirrors `ScanPreviewCompleteEvent.estimatedCompressedBytes`. */
  estimatedCompressedBytes?: CompressedSizeEstimate | null
}
