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
 * Result of starting a new directory listing.
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
export type LocationCategory = 'favorite' | 'main_volume' | 'attached_volume' | 'cloud_drive' | 'network'

/**
 * Information about a location (volume, folder, or cloud drive).
 */
export interface VolumeInfo {
    /** Unique identifier for the location */
    id: string
    /** Display name (e.g., "Macintosh HD", "Dropbox") */
    name: string
    /** Path to the location */
    path: string
    /** Category of this location */
    category: LocationCategory
    /** Base64-encoded icon (WebP format), optional */
    icon?: string
    /** Whether this can be ejected */
    isEjectable: boolean
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
    /** Resolved hostname (e.g., "macbook.local"), or undefined if not yet resolved */
    hostname?: string
    /** Resolved IP address, or undefined if not yet resolved */
    ipAddress?: string
    /** SMB port (usually 445) */
    port: number
}
