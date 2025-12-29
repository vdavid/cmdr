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
}

/** Cloud sync status for files in Dropbox/iCloud/etc. folders */
export type SyncStatus = 'synced' | 'online_only' | 'uploading' | 'downloading' | 'unknown'

/**
 * Result of starting a new directory listing session.
 * The session caches entries on the backend for fast subsequent chunk fetches.
 */
export interface SessionStartResult {
    /** Unique session ID for subsequent next/end calls */
    sessionId: string
    /** Total number of entries in the directory */
    totalCount: number
    /** First chunk of entries */
    entries: FileEntry[]
    /** Whether there are more entries to fetch */
    hasMore: boolean
}

/**
 * Result of fetching the next chunk in a session.
 */
export interface ChunkNextResult {
    /** Chunk of entries */
    entries: FileEntry[]
    /** Whether there are more entries to fetch */
    hasMore: boolean
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
    /** Session ID this diff belongs to */
    sessionId: string
    /** Monotonic sequence number for ordering */
    sequence: number
    /** List of changes */
    changes: DiffChange[]
}
