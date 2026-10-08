import { setListingIncludeHidden } from '$lib/tauri-commands'
import type { ResortResult } from '../types'
import type { PaneRowState } from './pane-row-state'
import type { collectSortState } from './sorting-handlers'

export interface HiddenFilesResyncInput {
  listingId: string
  includeHidden: boolean
  rowState: PaneRowState
  getSortState: () => ReturnType<typeof collectSortState>
  applyResult: (result: ResortResult) => undefined | (() => void)
}

/** Visibility and sorting share the pane's serialized row-space transition gate. */
export function createHiddenFilesResync(getPaneListingId: () => string) {
  let disposed = false
  async function resync(input: HiddenFilesResyncInput): Promise<void> {
    if (disposed) return
    await input.rowState.changeView({
      includeHidden: input.includeHidden,
      isCurrent: () => !disposed && getPaneListingId() === input.listingId,
      request: (token) => {
        const state = input.getSortState()
        return setListingIncludeHidden(
          token.listingId,
          input.includeHidden,
          token.sequence,
          state.cursorFilename ?? null,
          state.backendSelectedIndices ?? null,
          state.allSelected ?? null,
        )
      },
      install: input.applyResult,
    })
  }
  return {
    resync,
    dispose: () => {
      disposed = true
    },
  }
}
