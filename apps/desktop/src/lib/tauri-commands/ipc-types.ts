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

/**
 * Throws a typed IPC error value as an actual Error object, satisfying
 * `@typescript-eslint/only-throw-error`. When the error value has a `.message`
 * string property (e.g. IpcError), the Error message is set to that string and
 * the original properties are copied onto the Error so `isIpcError()` and similar
 * checks still work. Plain strings become `new Error(string)`. Everything else is
 * JSON-stringified into the message.
 *
 * Use this in typed-bindings error paths:
 *   if (res.status === 'error') throwIpcError(res.error)
 */
export function throwIpcError(error: unknown): never {
  if (error instanceof Error) throw error
  if (typeof error === 'string') throw new Error(error)
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof (error as Record<string, unknown>)['message'] === 'string'
  ) {
    const msg = (error as Record<string, unknown>)['message'] as string
    throw Object.assign(new Error(msg), error)
  }
  throw new Error(JSON.stringify(error))
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
  /** Scope paths that couldn't be searched because their volume has no index (an unindexed NAS share, an ejected drive). Empty on a fully-covered search. */
  uncoveredScopes?: string[]
  /** Scope paths that routed to an indexed volume but weren't found in its index (a typo, a since-deleted folder). Empty when every scope path resolved. */
  unresolvedScopes?: string[]
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
  caveat?: string
}

export interface ParsedScope {
  includePaths: string[]
  excludePatterns: string[]
}
