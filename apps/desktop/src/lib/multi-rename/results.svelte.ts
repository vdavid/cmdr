/**
 * Results (⌥⏎), Total Commander's "edit names": the shown preview goes out to a text file in
 * the user's editor, and coming back to the window reads it back. The file, the reading, and
 * the typed names are the backend session's (`src-tauri/src/multi_rename/names_file.rs`);
 * this holds whether a file is out, and why reading it back didn't go.
 */

import type { MultiRenameError } from '$lib/tauri-commands'
import { openFileInEditor } from '$lib/text-editor/open-file-in-editor'
import type { MultiRenameState } from './multi-rename-state.svelte'

export interface ResultsState {
  /** A names file is open in the editor: the window's next focus reads it back. */
  readonly open: boolean
  /** Why reading the file back didn't go, until the next try. */
  readonly error: MultiRenameError | null
  /** Writes the names and opens them in the editor. */
  start: () => Promise<void>
  /** Reads the file back while one is out; a no-op otherwise. */
  readBack: () => Promise<void>
  /** Drops the typed names and stops reading the file. */
  discard: () => Promise<void>
}

export function createResults(tool: Pick<MultiRenameState, 'writeNames' | 'readNames' | 'clearNames'>): ResultsState {
  let open = $state(false)
  let error = $state<MultiRenameError | null>(null)

  return {
    get open() {
      return open
    },
    get error() {
      return error
    },
    async start() {
      error = null
      // A failed write is `applyError`'s to word.
      const path = await tool.writeNames()
      if (path === null) return
      // A launch that didn't start is the editor helper's toast to say.
      if (await openFileInEditor(path)) open = true
    },
    async readBack() {
      if (!open) return
      error = await tool.readNames()
      if (error) open = false
    },
    async discard() {
      open = false
      error = null
      await tool.clearNames()
    },
  }
}
