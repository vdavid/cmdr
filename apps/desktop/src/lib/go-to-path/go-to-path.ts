/**
 * Frontend handler for the "Go to path" command (⌘G) and the pure helpers the
 * dialog shares with it.
 *
 * The backend (`resolve_go_to_path`) owns all path reasoning and returns a
 * tagged `GoToPathResolution`. This handler is a thin presenter: it switches on
 * the typed `kind` discriminator (never on a message string) and calls the
 * shared navigation primitives. On any successful jump it records the RESOLVED
 * target into recents, never the raw typed input.
 */

import { commands, type GoToPathResolution, type RecentPathEntry } from '$lib/ipc/bindings'
import { addToast } from '$lib/ui/toast'
import { getEffectiveShortcuts } from '$lib/shortcuts'
import { getAppLogger } from '$lib/logging/logger'
import {
  navigateToDirInPane,
  navigateToFileInPane,
  resolveLocationOrToast,
} from '$lib/file-explorer/navigation/navigate-and-select'
import { getFocusedPanePath } from '$lib/file-explorer/pane/focused-pane-reads'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'
import { GO_TO_PATH_ANCESTOR_TOAST_ID } from './go-to-path-ids'
import { addRecentPath } from './recent-paths-state.svelte'

export { GO_TO_PATH_ANCESTOR_TOAST_ID }

const log = getAppLogger('go-to-path')

/**
 * Resolve `input` against the focused pane and jump there.
 *
 * Contract (per `kind`):
 * - `directory` → navigate the focused pane into the dir.
 * - `file` → navigate to the parent dir and select the file (don't open it).
 * - `nearestAncestor` → navigate to the nearest existing ancestor, then fire an
 *   INFO toast whose back-shortcut is snapshotted at toast-creation.
 * - `invalid` → no-op (empty/unresolvable input; the dialog gates this anyway).
 *
 * On `directory` / `file` / `nearestAncestor` success the resolved target is
 * recorded into recents. The helper is a no-op when `explorer` is `undefined`
 * (HMR or pre-mount).
 *
 * Returns the resolution so callers (the dialog) can react (close on success,
 * stay open on `invalid`).
 */
export async function goToPath(
  explorer: ExplorerAPI | undefined,
  input: string,
): Promise<GoToPathResolution | undefined> {
  if (!explorer) {
    log.debug('goToPath: no explorer; skipping (HMR or pre-mount)')
    return undefined
  }

  const baseDir = getFocusedPanePath()
  const result = await commands.resolveGoToPath(input, baseDir)
  if (result.status === 'error') {
    log.warn('goToPath: resolve failed for {input}: {error}', { input, error: result.error.message })
    return undefined
  }

  const resolution = result.data
  const pane = explorer.getFocusedPane()

  // Exhaustive switch on the typed enum. Never branch on a message string —
  // the `kind` discriminator is the contract. Each jump resolves its target dir
  // to a `Location` (volume id + path) on the FE at jump time — `resolve_go_to_path`
  // stays pure (two consumers, incl. a per-keystroke preview), so the volume
  // `statfs` happens here, once, only when the user actually jumps.
  switch (resolution.kind) {
    case 'directory': {
      const location = await resolveLocationOrToast(resolution.path)
      if (!location) return resolution
      await navigateToDirInPane(explorer, pane, location)
      await recordRecent(resolution.path)
      return resolution
    }
    case 'file': {
      const location = await resolveLocationOrToast(resolution.parentDir)
      if (!location) return resolution
      await navigateToFileInPane(explorer, pane, location, resolution.fileName)
      await recordRecent(resolution.path)
      return resolution
    }
    case 'nearestAncestor': {
      const location = await resolveLocationOrToast(resolution.ancestorDir)
      if (!location) return resolution
      await navigateToDirInPane(explorer, pane, location)
      // Snapshot the back-shortcut at toast-creation time so a later rebind
      // doesn't rewrite a visible toast (matches the downloads snapshot rule).
      const backShortcut = getEffectiveShortcuts('nav.back')[0] ?? ''
      addToast(GoToPathAncestorToastContent, {
        id: GO_TO_PATH_ANCESTOR_TOAST_ID,
        level: 'info',
        toastGroup: 'go-to-path',
        props: {
          requested: resolution.requested,
          landed: resolution.ancestorDir,
          backShortcut,
        },
      })
      await recordRecent(resolution.ancestorDir)
      return resolution
    }
    case 'invalid':
      log.debug('goToPath: invalid input {input}: {reason}', { input, reason: resolution.reason })
      return resolution
  }
}

/**
 * Records a resolved target into recents. Routes through the `$state` mirror so
 * the dialog's in-memory list stays in sync (the mirror writes the backend and
 * re-reads the authoritative order). Best-effort.
 */
async function recordRecent(path: string): Promise<void> {
  const entry: RecentPathEntry = {
    id: crypto.randomUUID(),
    timestamp: Date.now(),
    path,
  }
  await addRecentPath(entry)
}

/**
 * Maps a digit key to a recents index, but ONLY when the textbox is empty.
 *
 * The empty-box guard is unambiguous because no valid path starts with a digit
 * (paths start with `/`, `~`, or `.`), so once any character is in the box,
 * digits are ordinary input. `'1'..'9'` map to 0..8, `'0'` maps to 9 (the
 * tenth recent). A digit with no corresponding recent, a non-digit key, a
 * non-empty box, or a modifier held returns `null` (no jump).
 */
export function digitToRecentIndex(
  inputValue: string,
  key: string,
  recentsCount: number,
  modifierHeld = false,
): number | null {
  if (inputValue !== '') return null
  if (modifierHeld) return null
  if (key.length !== 1 || key < '0' || key > '9') return null

  // '1'..'9' → 0..8, '0' → 9.
  const index = key === '0' ? 9 : Number(key) - 1
  if (index >= recentsCount) return null
  return index
}

/**
 * Whether to prefill the dialog's textbox from the clipboard: only when the
 * clipboard string resolves to something that actually exists on disk (an
 * existing directory or file). A nearest-ancestor or invalid resolution means
 * the clipboard isn't a real path, so the box opens empty.
 */
export function shouldPrefillClipboard(resolution: GoToPathResolution): boolean {
  return resolution.kind === 'directory' || resolution.kind === 'file'
}
