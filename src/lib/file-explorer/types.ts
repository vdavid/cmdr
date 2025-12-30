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
