// Shared IPC types for timeout-aware backend communication.

/**
 * Wraps a backend result with a flag indicating whether the operation timed out.
 * Used by commands returning collections or Option so the frontend can distinguish
 * "genuinely empty/none" from "timed out before completing."
 */
export interface TimedOut<T> {
    data: T
    timedOut: boolean
}

/**
 * Structured IPC error from the backend.
 * Commands returning `Result<T, IpcError>` send this on failure.
 * The `timedOut` flag lets the frontend distinguish timeout errors from real failures
 * without fragile string matching.
 */
export interface IpcError {
    message: string
    timedOut: boolean
}

/** Type guard: checks if an unknown error value is a structured IpcError. */
export function isIpcError(error: unknown): error is IpcError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'message' in error &&
        'timedOut' in error &&
        typeof (error as IpcError).message === 'string' &&
        typeof (error as IpcError).timedOut === 'boolean'
    )
}

/** Extracts a human-readable message from a caught IPC error (IpcError, Error, or string). */
export function getIpcErrorMessage(error: unknown): string {
    if (isIpcError(error)) return error.message
    if (error instanceof Error) return error.message
    return String(error)
}

// ============================================================================
// Search types
// ============================================================================

export type PatternType = 'glob' | 'regex'

export interface SearchQuery {
    namePattern?: string
    patternType: PatternType
    minSize?: number
    maxSize?: number
    modifiedAfter?: number
    modifiedBefore?: number
    isDirectory?: boolean
    includePaths?: string[]
    excludeDirNames?: string[]
    limit: number
    caseSensitive?: boolean
    excludeSystemDirs?: boolean
}

export interface SearchResult {
    entries: SearchResultEntry[]
    totalCount: number
}

export interface SearchResultEntry {
    name: string
    path: string
    parentPath: string
    isDirectory: boolean
    size: number | null
    modifiedAt: number | null
    iconId: string
}

export interface PrepareResult {
    ready: boolean
    entryCount: number
}

export interface TranslatedQuery {
    namePattern: string | null
    patternType: string
    minSize: number | null
    maxSize: number | null
    modifiedAfter: number | null
    modifiedBefore: number | null
    isDirectory: boolean | null
    includePaths?: string[]
    excludeDirNames?: string[]
    caseSensitive?: boolean
    excludeSystemDirs?: boolean
}

export interface TranslateDisplay {
    namePattern: string | null
    patternType: string | null
    minSize: number | null
    maxSize: number | null
    modifiedAfter: string | null
    modifiedBefore: string | null
    isDirectory: boolean | null
    caseSensitive: boolean | null
    includePaths?: string[]
    excludeDirNames?: string[]
}

export interface TranslateResult {
    query: TranslatedQuery
    display: TranslateDisplay
    preflightSummary?: string
    caveat?: string
}

export interface PreflightContext {
    totalCount: number
    sampleEntries: PreflightEntry[]
}

export interface PreflightEntry {
    name: string
    size: number | null
    modifiedAt: number | null
    isDirectory: boolean
}

export interface ParsedScope {
    includePaths: string[]
    excludePatterns: string[]
}
