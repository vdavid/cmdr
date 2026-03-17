// Search IPC commands — typed wrappers for the backend search engine.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ParsedScope, PrepareResult, SearchQuery, SearchResult, TranslateResult } from './ipc-types'

/**
 * Starts loading the search index in the background.
 * Returns immediately with current readiness state.
 * Emits "search-index-ready" when load completes.
 */
export async function prepareSearchIndex(): Promise<PrepareResult> {
    return invoke<PrepareResult>('prepare_search_index')
}

/** Searches the in-memory index. Returns empty results if index isn't loaded yet. */
export async function searchFiles(query: SearchQuery): Promise<SearchResult> {
    return invoke<SearchResult>('search_files', { query })
}

/** Signals that the search dialog closed. Starts the idle timer for index eviction. */
export async function releaseSearchIndex(): Promise<void> {
    return invoke('release_search_index')
}

/** Translates a natural language query into structured search filters using the configured LLM. */
export async function translateSearchQuery(naturalQuery: string): Promise<TranslateResult> {
    return invoke<TranslateResult>('translate_search_query', { naturalQuery })
}

/** Parses a scope string into structured include/exclude data. */
export async function parseSearchScope(scope: string): Promise<ParsedScope> {
    return invoke<ParsedScope>('parse_search_scope', { scope })
}

/** Listens for the search index ready event (emitted after prepare completes loading). */
export function onSearchIndexReady(handler: (entryCount: number) => void): Promise<UnlistenFn> {
    return listen<{ entryCount: number }>('search-index-ready', (event) => {
        handler(event.payload.entryCount)
    })
}
