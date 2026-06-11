/**
 * Selection's module-level `QueryFilterState` singleton.
 *
 * Mirrors `lib/search/search-state.svelte.ts`, but thinner: Selection has no
 * "extras" (no scope, no exclude-system-dirs, no index flags, no Pattern chip),
 * so it holds only the cross-consumer core factory, with no façade fan-out.
 *
 * Why a module singleton (not a component-scoped `const`): the dialog mounts on
 * open and unmounts on close. A per-component instance would be reborn empty on
 * every reopen, losing the user's mode, term, and filters. The shared query-ui
 * contract is "state survives unmount by design"; the singleton is how Selection
 * honors it. Persistence is GLOBAL, not per-folder: reopening in folder B keeps
 * the filters set in folder A and re-runs them against B's snapshot. `⌘N` (the
 * dialog's clear hook → `clearSelectionState`) is the only reset.
 */

import { createQueryFilterState } from '$lib/query-ui/query-filter-state.svelte'

/**
 * The single cross-consumer state instance for the Selection dialog. Exposed so
 * `SelectionDialog.svelte` can hand it straight to `QueryDialog`'s `state` prop
 * and to its matcher / AI-apply helpers.
 */
export const selectionQueryState = createQueryFilterState({ defaultMode: 'filename' })

/**
 * Clears all Selection dialog state to defaults. Triggered by the user via `⌘N`
 * ("new selection") inside the dialog.
 */
export function clearSelectionState(): void {
  selectionQueryState.clearCore()
}
