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
