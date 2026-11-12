/**
 * "Left pane, Documents", spoken through the pane's live region (`#lib/ui/LiveAnnouncer.svelte`) when
 * the pane takes focus and when the focused pane settles on another folder.
 *
 * ❗ VoiceOver never speaks a listbox's own name, neither at launch nor on `Tab`, even with
 * distinct names and the active descendant held back up to 1.2 s (macOS 27.0, VoiceOver 10,
 * 2026-10-15 spike). So which pane and folder you're in has to be announced.
 *
 * Row first: the message waits for the list's cursor row to land as the active descendant
 * (`cursorRowLanded`), so the polite message queues behind the row instead of racing it. When
 * no row lands (an empty folder at a drive's root, a pane showing no list), it goes out after
 * `NO_ROW_FALLBACK_MS`. Whether VoiceOver still cuts one off is a listening question, not a
 * timing knob to tune blind.
 */

import { untrack } from 'svelte'
import { tString } from '#lib/intl/messages.svelte.js'
import { emptyFolderSpokenText } from '../views/empty-folder-text'

/** How long a pending announcement waits for a cursor row to land before it goes out anyway. */
const NO_ROW_FALLBACK_MS = 400

export interface PaneLocationInput {
  side: 'left' | 'right'
  currentPath: string
  /** The pane sits on its volume's root, where the folder's name is the volume's. */
  isVolumeRoot: boolean
  volumeName?: string
  isSearchResults: boolean
}

/** The last path segment, ignoring a trailing slash; empty at `/`. */
function lastSegment(path: string): string {
  return path.replace(/\/+$/, '').split('/').at(-1) ?? ''
}

/**
 * "{Left|Right} pane, {folder}": the announcement, and the pane's listbox name. ❌ No "active"
 * word: the unfocused pane keeps its name, and it would go stale there.
 */
export function paneLocationLabel(input: PaneLocationInput): string {
  const folder = input.isSearchResults
    ? tString('fileExplorer.pane.searchResultsLocation')
    : (input.isVolumeRoot ? input.volumeName : undefined) || lastSegment(input.currentPath) || input.currentPath
  return tString('fileExplorer.pane.location', { side: input.side, folder })
}

/**
 * What an arrival says: the location, plus what the empty state shows when the pane shows no
 * rows ("…, empty folder", or the hidden items and how to see them, `../views/empty-folder-text.ts`).
 * `emptyHiddenCount` is `null` while the pane shows rows.
 */
export function paneArrivalMessage(location: string, emptyHiddenCount: number | null): string {
  if (emptyHiddenCount === null) return location
  if (emptyHiddenCount <= 0) return tString('fileExplorer.pane.locationEmpty', { location })
  return tString('fileExplorer.pane.locationHiddenItems', {
    location,
    emptyState: emptyFolderSpokenText(emptyHiddenCount),
  })
}

export interface PaneLocationAnnouncerDeps {
  /** "Left pane, Documents": which folder the pane shows. A change of it is an arrival. */
  label: () => string
  /**
   * What an arrival says, when it's more than `label` (`paneArrivalMessage`: "Left pane, Downloads, empty folder").
   * Read when the message goes out, so it describes the settled listing; its own changes
   * announce nothing (`pane-status-announcer.svelte.ts` speaks about changes in place).
   */
  message?: () => string
  isFocused: () => boolean
  /** The pane shows what `label` names: no listing still loading into it. */
  isSettled: () => boolean
  announce: (message: string) => void
}

export interface PaneLocationAnnouncer {
  /** The pane's list just landed its cursor row as the active descendant: a pending message may go. */
  cursorRowLanded: () => void
}

/** Call during component init (it installs an effect). */
export function createPaneLocationAnnouncer(deps: PaneLocationAnnouncerDeps): PaneLocationAnnouncer {
  let wasFocused = false
  let lastLabel: string | undefined
  /** The pane took focus or changed folder, and nobody has heard about it yet. */
  let owed = false
  let pending: string | null = null
  let fallback: ReturnType<typeof setTimeout> | undefined

  function flush(): void {
    clearTimeout(fallback)
    if (pending === null) return
    const label = pending
    pending = null
    deps.announce(deps.message ? untrack(deps.message) : label)
  }

  function drop(): void {
    clearTimeout(fallback)
    pending = null
    owed = false
  }

  $effect(() => {
    const focused = deps.isFocused()
    const label = deps.label()
    const settled = deps.isSettled()
    untrack(() => {
      if (!focused) {
        wasFocused = false
        drop()
        return
      }
      // Only a folder change counts, never a listing batch in the same folder.
      if (!wasFocused || label !== lastLabel) owed = true
      wasFocused = true
      lastLabel = label
      if (!owed || !settled) return
      owed = false
      pending = label
      clearTimeout(fallback)
      fallback = setTimeout(flush, NO_ROW_FALLBACK_MS)
    })
  })
  $effect(() => () => {
    clearTimeout(fallback)
  })

  return { cursorRowLanded: flush }
}
