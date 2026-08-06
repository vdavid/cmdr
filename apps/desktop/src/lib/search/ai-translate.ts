/**
 * Search's `translateAi` callback: the translate IPC, plus every write the answer makes
 * into Search state.
 *
 * The AI returns one shape and it lands in five places (the Pattern chip and its label,
 * size, date, type, scope, case sensitivity, the system-dir toggle), so this is where the
 * mapping lives rather than in the dialog wrapper. What comes BACK is only what QueryDialog
 * needs to render: the caveat, and the names of the chips that changed (for the highlight
 * flash).
 *
 * ❌ This does NOT write `lastAiPrompt` / `lastAiCaveat`: QueryDialog owns both
 * (`query-ui/CLAUDE.md`). And the typed IPC error is deliberately left to throw, so
 * QueryDialog can show the specific toast (quota, key rejected, timeout, empty answer)
 * instead of the run failing silently.
 */

import { SvelteSet } from 'svelte/reactivity'
import { applySizeFromAi, applyDateFromAi, applyTypeFromAi } from '$lib/query-ui/apply-ai-filters'
import { typeFilterToIsDirectory } from '$lib/query-ui/query-filter-state.svelte'
import type { AiTranslateResult } from '$lib/query-ui/query-dialog-config'
import { translateSearchQuery, type TranslateResult } from '$lib/tauri-commands'
import {
  searchQueryState,
  recordAiTranslation,
  setCaseSensitive,
  setExcludeSystemDirs,
  setScope,
} from './search-state.svelte'

/** Recovers the structured pattern kind ('glob' | 'regex' | null) from the AI display string. */
export function patternKindFromDisplay(patternType: string | null | undefined): 'glob' | 'regex' | null {
  if (patternType === 'regex') return 'regex'
  if (patternType === 'glob') return 'glob'
  return null
}

/** Folds the AI's `includePaths` + `excludeDirNames` into one scope expression. Returns true if set. */
export function applyScopeFromAi(includePaths: string[] | null, excludeDirNames: string[] | null): boolean {
  if (!includePaths?.length && !excludeDirNames?.length) return false
  const parts: string[] = []
  if (includePaths) parts.push(...includePaths)
  if (excludeDirNames) parts.push(...excludeDirNames.map((d) => `!${d}`))
  setScope(parts.join(', '))
  return true
}

/** Writes the produced pattern (+ label), case sensitivity, and the system-dir toggle. */
function applyAiPatternAndToggles(result: TranslateResult, changed: SvelteSet<string>): void {
  const { display, query } = result
  // Record the produced pattern in its own slot (the Pattern chip). The bar keeps the prompt.
  recordAiTranslation({
    pattern: display.namePattern ?? null,
    kind: patternKindFromDisplay(display.patternType),
    label: result.label ?? null,
  })
  if (display.namePattern != null) changed.add('pattern')
  if (query.caseSensitive != null) {
    setCaseSensitive(query.caseSensitive)
    changed.add('caseSensitive')
  }
  // The AI only ever turns OFF the default "hide boring folders" exclusion.
  if (query.excludeSystemDirs === false) {
    setExcludeSystemDirs(false)
    changed.add('excludeSystemDirs')
  }
  if (applyScopeFromAi(query.includePaths ?? null, query.excludeDirNames ?? null)) changed.add('scope')
}

/** Writes the shared Size / Modified / Type filters via the cross-consumer helpers. */
function applyAiSharedFilters(display: TranslateResult['display'], changed: SvelteSet<string>): void {
  // Reset size + date to `any` before applying the AI's bounds. `applySizeFromAi` /
  // `applyDateFromAi` no-op when the AI returns no bound, so without this a previous run's
  // size/date filter would silently leak into a run that didn't return one. Selection does
  // the same; the contract lives in `apply-ai-filters.ts`. The user's own manual filter edit
  // between runs is wiped too, which is the right call (running AI again means "give me the
  // AI's filter set", not a merge with a stale manual tweak).
  searchQueryState.setSizeFilter('any')
  searchQueryState.setDateFilter('any')
  if (applySizeFromAi(searchQueryState, display.minSize ?? null, display.maxSize ?? null)) changed.add('size')
  if (applyDateFromAi(searchQueryState, display.modifiedAfter ?? null, display.modifiedBefore ?? null))
    changed.add('date')
  // Type: leave-alone-if-null. The AI got the current type as context in `translateAi`;
  // it returns `isDirectory` only when it wants to change it, so a null leaves the user's
  // choice intact. Deliberately NOT reset-first like size/date (see `apply-ai-filters.ts`).
  if (applyTypeFromAi(searchQueryState, display.isDirectory ?? null)) changed.add('type')
}

/**
 * Paints a translate result onto the Search state and returns the names of the chips that
 * changed (for the QueryDialog highlight flash). Split into pattern-write vs filter-write
 * halves to keep each under the cognitive-complexity ceiling.
 */
export function applyAiTranslationToState(result: TranslateResult): string[] {
  const changed = new SvelteSet<string>()
  applyAiPatternAndToggles(result, changed)
  applyAiSharedFilters(result.display, changed)
  return Array.from(changed)
}

/**
 * Translates a natural-language prompt and applies the AI's filter writes: the Pattern
 * chip + label, size, date, scope, case sensitivity, and "hide boring folders". Returns
 * the caveat + highlighted-field list for QueryDialog to surface in the AI strip and
 * flash effect.
 */
export async function translateAi(prompt: string): Promise<AiTranslateResult | null> {
  // Hand the AI the user's current type as context so it can keep or change it.
  const currentType = typeFilterToIsDirectory(searchQueryState.getTypeFilter())
  const result = await translateSearchQuery(prompt, currentType)
  return {
    caveat: result.caveat,
    highlightedFields: applyAiTranslationToState(result),
  }
}
