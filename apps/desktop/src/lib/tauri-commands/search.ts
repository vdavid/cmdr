// Search IPC commands: typed wrappers for the backend search engine.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '$lib/ipc/bindings'
import type { ParsedScope, PrepareResult, SearchResult } from './ipc-types'
import { throwIpcError } from './ipc-types'
import type {
  HistoryEntry,
  LiveSearchStart,
  SearchCancelledEvent,
  SearchCompleteEvent,
  SearchErrorEvent,
  SearchProgressEvent,
  SearchQuery,
  TranslateResult,
} from '$lib/ipc/bindings'

/**
 * Starts loading the search index in the background.
 * Returns immediately with current readiness state.
 * Emits "search-index-ready" when load completes.
 */
export async function prepareSearchIndex(): Promise<PrepareResult> {
  const res = await commands.prepareSearchIndex()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Searches the in-memory index. Returns empty results if index isn't loaded yet. */
export async function searchFiles(query: SearchQuery): Promise<SearchResult> {
  const res = await commands.searchFiles(query)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Starts a search that answers over TIME: the index's half first, then whatever the
 * index can't answer for, walked live.
 *
 * Resolves as soon as routing has picked its one volume; everything else arrives as
 * `search-progress` / `search-complete` / `search-cancelled` / `search-error`, each
 * stamped with `runId`. The CALLER mints the id (as it does a listing id), so no event
 * can arrive against one the frontend hasn't seen. Starting a run supersedes the
 * previous one: its events stop, its walk carries on.
 */
export async function searchFilesStreaming(query: SearchQuery, runId: string): Promise<LiveSearchStart> {
  const res = await commands.searchFilesStreaming(query, runId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Stops a live search and the walk behind it. Resolves to whether there was one. */
export async function cancelSearch(runId: string): Promise<boolean> {
  const res = await commands.cancelSearch(runId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** A live search's batches, and where the run has got to. */
export function onSearchProgress(handler: (event: SearchProgressEvent) => void): Promise<UnlistenFn> {
  return events.searchProgress.listen((event) => {
    handler(event.payload)
  })
}

/** A live search finished on its own terms. ❌ Not the same as "the answer is complete". */
export function onSearchComplete(handler: (event: SearchCompleteEvent) => void): Promise<UnlistenFn> {
  return events.searchComplete.listen((event) => {
    handler(event.payload)
  })
}

/** Somebody stopped a live search. Its results stay on screen; they're all real. */
export function onSearchCancelled(handler: (event: SearchCancelledEvent) => void): Promise<UnlistenFn> {
  return events.searchCancelled.listen((event) => {
    handler(event.payload)
  })
}

/** A live search couldn't run at all. Typed reason plus the sentence to show. */
export function onSearchError(handler: (event: SearchErrorEvent) => void): Promise<UnlistenFn> {
  return events.searchError.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Signals that the search dialog closed. Starts the idle timer for index eviction
 * and stops every live search but `keepRunId`.
 *
 * A walk outlives the dialog only through "Open in pane", where its results are on
 * screen in a pane and still growing — that is the one run to name here. Everything
 * else is a query nobody is reading any more.
 */
export async function releaseSearchIndex(keepRunId: string | null = null): Promise<void> {
  const res = await commands.releaseSearchIndex(keepRunId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Translates a natural language query into structured search filters using the configured LLM.
 *
 * `currentType` is the dialog's `Files | Folders | Both` toggle as context (`true` = folders,
 * `false` = files, `null` = both). The AI may set the type or leave it; when it returns nothing,
 * the caller keeps the user's choice (see `applyTypeFromAi`).
 */
export async function translateSearchQuery(
  naturalQuery: string,
  currentType: boolean | null = null,
): Promise<TranslateResult> {
  const res = await commands.translateSearchQuery(naturalQuery, currentType)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Parses a scope string into structured include/exclude data. */
export async function parseSearchScope(scope: string): Promise<ParsedScope> {
  return commands.parseSearchScope(scope)
}

/** Returns the list of system/build/cache directory names excluded by default. */
export async function getSystemDirExcludes(): Promise<string[]> {
  return commands.getSystemDirExcludes()
}

/**
 * Listens for a volume's search arena landing (emitted after a background load completes).
 * The event NAMES its volume: a search covers one volume, so readiness is only ever true of
 * a particular one.
 */
export function onSearchIndexReady(handler: (volumeId: string, entryCount: number) => void): Promise<UnlistenFn> {
  return events.searchIndexReady.listen((event) => {
    handler(event.payload.volumeId, event.payload.entryCount)
  })
}

/** Returns the persisted recent-searches entries (newest first). `limit = null` returns all. */
export async function getRecentSearches(limit: number | null = null): Promise<HistoryEntry[]> {
  return commands.getRecentSearches(limit)
}

/** Adds an entry to the recent-searches store. The backend dedupes by canonical key and caps. */
export async function addRecentSearch(entry: HistoryEntry, maxCount: number | null = null): Promise<void> {
  const res = await commands.addRecentSearch(entry, maxCount)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Removes a single recent-search entry by id. No-op if the id isn't present. */
export async function removeRecentSearch(id: string): Promise<void> {
  const res = await commands.removeRecentSearch(id)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Clears every recent-search entry. */
export async function clearRecentSearches(): Promise<void> {
  const res = await commands.clearRecentSearches()
  if (res.status === 'error') throwIpcError(res.error)
}

/** Live-applies a new `search.recentSearches.maxCount` cap. */
export async function applyRecentSearchesMaxCount(maxCount: number): Promise<void> {
  const res = await commands.applyRecentSearchesMaxCount(maxCount)
  if (res.status === 'error') throwIpcError(res.error)
}
