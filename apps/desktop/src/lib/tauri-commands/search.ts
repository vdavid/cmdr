// Search IPC commands — typed wrappers for the backend search engine.

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { commands } from '$lib/ipc/bindings'
import type { ParsedScope, PrepareResult, SearchResult } from './ipc-types'
import { throwIpcError } from './ipc-types'
import type { SearchQuery, TranslateResult } from '$lib/ipc/bindings'

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

/** Signals that the search dialog closed. Starts the idle timer for index eviction. */
export async function releaseSearchIndex(): Promise<void> {
  const res = await commands.releaseSearchIndex()
  if (res.status === 'error') throwIpcError(res.error)
}

/** Translates a natural language query into structured search filters using the configured LLM. */
export async function translateSearchQuery(naturalQuery: string): Promise<TranslateResult> {
  const res = await commands.translateSearchQuery(naturalQuery)
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

/** Listens for the search index ready event (emitted after prepare completes loading). */
export function onSearchIndexReady(handler: (entryCount: number) => void): Promise<UnlistenFn> {
  return listen<{ entryCount: number }>('search-index-ready', (event) => {
    handler(event.payload.entryCount)
  })
}
