// Shared IPC types for timeout-aware backend communication.
//
// ❌ There is deliberately no generic "IPC error" type here any more. Every
// command family ships its OWN typed error enum, which the frontend renders from
// through the message catalog (`$lib/ipc/typed-failure.ts` carries one across a
// throw). A shared `{ message, timedOut }` shape is how a typed refusal used to
// become an untranslated English sentence in front of a person.

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
 * Throws a wire error value as an actual `Error`, satisfying
 * `@typescript-eslint/only-throw-error`, with the original properties copied
 * onto it so a catch site can still read them.
 *
 * `Error.message` is a best-effort DIAGNOSTIC for logs and generic consumers.
 * ❌ Nothing a user reads comes from it: a command family whose refusal reaches
 * a human throws a `TypedFailure` subclass instead and words it from the catalog
 * (`$lib/ipc/typed-failure.ts`).
 *
 * A tagged wire error (`{ type: 'timedOut', … }`, the shape every typed command
 * family ships) becomes `Error("timedOut")` rather than a blob of JSON, so a log
 * line names the REASON. Plain strings become `new Error(string)`; anything else
 * falls back to JSON.
 *
 * Use this in typed-bindings error paths:
 *   if (res.status === 'error') throwIpcError(res.error)
 */
export function throwIpcError(error: unknown): never {
  if (error instanceof Error) throw error
  if (typeof error === 'string') throw new Error(error)
  if (typeof error === 'object' && error !== null) {
    const fields = error as Record<string, unknown>
    for (const key of ['message', 'type', 'kind'] as const) {
      if (typeof fields[key] === 'string') throw Object.assign(new Error(fields[key]), error)
    }
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
  /** The one volume this search covered, as the backend's routing resolved it. Lets a caller act on a coverage gap against the right drive instead of re-deriving it from the path. */
  targetVolumeId?: string
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
  /**
   * Whether a background load is in flight, so a `search-index-ready` naming this volume is
   * coming. `false` alongside `ready: false` is the terminal "there is no index to load here":
   * the dialog stops waiting and runs the search, which answers with its coverage gap named.
   */
  loading: boolean
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
