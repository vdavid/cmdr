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
    limit: number
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
    size?: number
    modifiedAt?: number
    iconId: string
}

export interface PrepareResult {
    ready: boolean
    entryCount: number
}

export interface TranslatedQuery {
    namePattern?: string
    patternType: string
    minSize?: number
    maxSize?: number
    modifiedAfter?: number
    modifiedBefore?: number
    isDirectory?: boolean
}

export interface TranslateDisplay {
    namePattern?: string
    patternType?: string
    minSize?: number
    maxSize?: number
    modifiedAfter?: string
    modifiedBefore?: string
    isDirectory?: boolean
}

export interface TranslateResult {
    query: TranslatedQuery
    display: TranslateDisplay
}
