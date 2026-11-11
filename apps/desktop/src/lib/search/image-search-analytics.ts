/**
 * `image_search_used`: one event per image-grid query that settled
 * (`ImageSearchResults.svelte`), so "does anyone search their images?" has a number.
 *
 * Its own event rather than a prop on `search_used`: the grid runs its own debounced
 * query beside the file search, on its own clock, so a run of one can't say what the
 * other did. PII-free by construction: nothing here can see the query, a path, or an
 * OCR snippet; what crosses is a mode, an outcome, a bucketed hit count, and a bool.
 */

import { itemCountBucket, trackEvent } from '#lib/tauri-commands/index.js'

/** What the grid searched by: the dialog's text (CLIP descriptions plus OCR), or a tile's look-alikes. */
export type ImageSearchMode = 'text' | 'similar'

/** How the query ended. `failed` is an IPC that threw; the grid shows it as no results. */
export type ImageSearchOutcome = 'results' | 'no_results' | 'failed'

export interface ImageSearchFacts {
  mode: ImageSearchMode
  outcome: ImageSearchOutcome
  /** Every hit the query found, before the grid's tile cap. */
  hits: number
  /** Whether any hit came from the semantic (CLIP) pass: `false` on an OCR-only install. */
  semantic: boolean
}

/** The outcome a settled query with `hits` hits reports. */
export function imageSearchOutcome(hits: number): ImageSearchOutcome {
  return hits > 0 ? 'results' : 'no_results'
}

export function reportImageSearch(facts: ImageSearchFacts): void {
  void trackEvent('image_search_used', {
    mode: facts.mode,
    outcome: facts.outcome,
    result_bucket: itemCountBucket(facts.hits),
    semantic: facts.semantic,
  })
}
