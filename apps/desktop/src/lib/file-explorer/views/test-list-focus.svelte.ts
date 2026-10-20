/**
 * The focus-model cases both file lists owe a screen reader, run once per view from its
 * a11y test file (each owns the module stubs its view needs):
 *
 *     describe('BriefList focus model', () => {
 *       listFocusCases((options) => mountBriefList(options))
 *     })
 *
 * A `.svelte.ts` so a case can hold live props in `$state` (a navigation, a cursor move).
 */

import { expect, it, vi } from 'vitest'
import { tick, unmount } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import { getFileRange } from '#lib/tauri-commands/index.js'
import { computeAccessibleName } from 'dom-accessibility-api'
import { assertFocusModel } from '#lib/test-focus-model.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { registerFocusHome } from '#lib/ui/focus-restore.js'
import { dirEntry, fileEntry } from './test-full-list'
import type { FileEntry } from '../types'
import type { RenameSessionId, RenameState } from '../rename/rename-state.svelte'

/** The props every case may change after mount; both views take them. */
interface LiveListProps {
  listingId: string
  cursorIndex: number
}

export interface MountedFocusableList {
  component: { focusList: () => boolean }
  target: HTMLElement
  rows: () => HTMLElement[]
  rowNames: () => string[]
  settle: (until: () => boolean, reason: string) => Promise<void>
}

/** Fixed props a case may set at mount; both views take them. */
interface ListProps {
  hasParent: boolean
  parentPath: string
  totalCount: number
  selectedIndices: Set<number>
  listLabel: string
  onCursorRowLanded: () => void
  renameState: RenameState | null
  onRenameCancel: (sessionId: RenameSessionId) => void
}

export type MountFocusableList = (options: {
  entries: FileEntry[]
  liveProps?: Partial<LiveListProps>
  props?: Partial<ListProps>
}) => Promise<MountedFocusableList>

const ENTRIES = [fileEntry({ name: 'a.txt' }), fileEntry({ name: 'b.txt' }), fileEntry({ name: 'c.txt' })]

function listboxOf(list: MountedFocusableList): HTMLElement {
  const listbox = list.target.querySelector<HTMLElement>('[role="listbox"]')
  if (!listbox) throw new Error('the list rendered no listbox')
  return listbox
}

function activeDescendant(listbox: HTMLElement): string | null {
  return listbox.getAttribute('aria-activedescendant')
}

function cursorOption(listbox: HTMLElement): HTMLElement {
  const option = document.getElementById(activeDescendant(listbox) ?? '')
  if (!option) throw new Error('the listbox names no rendered cursor row')
  return option
}

async function focusAndLand(list: MountedFocusableList): Promise<HTMLElement> {
  const listbox = listboxOf(list)
  expect(list.component.focusList()).toBe(true)
  await list.settle(() => activeDescendant(listbox) !== null, 'the cursor row to land as the active descendant')
  return listbox
}

export function listFocusCases(mountList: MountFocusableList): void {
  it('renders row ids that stay unique with two lists mounted', async () => {
    const first = await mountList({ entries: ENTRIES })
    const second = await mountList({ entries: ENTRIES })
    const ids = [...first.rows(), ...second.rows()].map((row) => row.id)
    expect(ids).toHaveLength(ENTRIES.length * 2)
    expect(ids.filter((id, at) => ids.indexOf(id) !== at)).toEqual([])
  })

  it('focuses its listbox and names the cursor row one frame later, never at once', async () => {
    const list = await mountList({ entries: ENTRIES })
    const listbox = listboxOf(list)
    expect(activeDescendant(listbox)).toBeNull()

    expect(list.component.focusList()).toBe(true)
    expect(document.activeElement).toBe(listbox)
    await tick()
    expect(activeDescendant(listbox)).toBeNull()

    await list.settle(() => activeDescendant(listbox) !== null, 'the cursor row to land as the active descendant')
    assertFocusModel(list.target)
    expect(document.getElementById(activeDescendant(listbox) ?? '')?.dataset.filename).toBe('a.txt')
  })

  it('names no row once focus leaves it', async () => {
    const list = await mountList({ entries: ENTRIES })
    const listbox = await focusAndLand(list)
    listbox.blur()
    await tick()
    expect(activeDescendant(listbox)).toBeNull()
  })

  it('names the new cursor row at once when the cursor moves', async () => {
    const live = $state<Partial<LiveListProps>>({})
    const list = await mountList({ entries: ENTRIES, liveProps: live })
    const listbox = await focusAndLand(list)
    const seen: (string | null)[] = []
    new MutationObserver(() => seen.push(activeDescendant(listbox))).observe(listbox, {
      attributeFilter: ['aria-activedescendant'],
    })

    live.cursorIndex = 1
    await tick()
    await Promise.resolve()

    expect(document.getElementById(activeDescendant(listbox) ?? '')?.dataset.filename).toBe('b.txt')
    expect(seen).not.toContain(null)
  })

  // ❗ Entering a folder renders a new row 0 under the same id; without landing the id again
  // WebKit keeps focus on the dead node and VoiceOver says nothing (heard in the 2026-10-15 VoiceOver spike).
  it('lands the id again when a navigation replaces the cursor row under it', async () => {
    const live = $state<Partial<LiveListProps>>({})
    const list = await mountList({ entries: ENTRIES, liveProps: live })
    const listbox = await focusAndLand(list)
    const rowZeroId = activeDescendant(listbox)
    const seen: (string | null)[] = []
    new MutationObserver(() => seen.push(activeDescendant(listbox))).observe(listbox, {
      attributeFilter: ['aria-activedescendant'],
    })

    // As many rows as before: `totalCount` stays, and a listing shorter than it never settles.
    const next = ['x.txt', 'y.txt', 'z.txt'].map((name) => fileEntry({ name, path: `/root/sub/${name}` }))
    vi.mocked(getFileRange).mockImplementation((_listingId, start: number, count: number) =>
      Promise.resolve(next.slice(start, start + count)),
    )
    live.listingId = 'listing-2'
    await list.settle(
      () => list.rowNames()[0] === 'x.txt' && activeDescendant(listbox) !== null,
      'the new folder rows and their cursor row',
    )

    expect(activeDescendant(listbox)).toBe(rowZeroId)
    expect(seen).toContain(null)
    assertFocusModel(list.target)
  })

  // ❗ A Brief/Full toggle remounts the list: focus fell to `<body>` and ↓ did nothing until
  // `Tab` (2026-10-15 VoiceOver spike). The new list picks focus up when its pane is focused.
  it('takes focus when it mounts in the focused pane while focus is adrift', async () => {
    const unregister = registerFocusHome({ root: () => undefined, refocus: () => {} })
    try {
      document.body.focus()
      const list = await mountList({ entries: ENTRIES })
      await list.settle(() => document.activeElement === listboxOf(list), 'the new list to take focus')
    } finally {
      unregister()
    }
  })

  // ❗ Backspace on a focused path segment navigates up and unmounts that very segment (it
  // becomes the current folder, a plain span), dropping focus to `<body>` with no list remount
  // to pick it up. The navigation's new rows do.
  it('takes focus when a navigation lands while focus is adrift', async () => {
    const unregister = registerFocusHome({ root: () => undefined, refocus: () => {} })
    const segment = document.createElement('button')
    try {
      const live = $state<Partial<LiveListProps>>({})
      const list = await mountList({ entries: ENTRIES, liveProps: live })
      document.body.append(segment)
      segment.focus()
      segment.remove()
      expect(document.activeElement).toBe(document.body)

      const next = ['x.txt', 'y.txt', 'z.txt'].map((name) => fileEntry({ name, path: `/root/up/${name}` }))
      vi.mocked(getFileRange).mockImplementation((_listingId, start: number, count: number) =>
        Promise.resolve(next.slice(start, start + count)),
      )
      live.listingId = 'listing-up'
      await list.settle(() => list.rowNames()[0] === 'x.txt', 'the parent folder rows')
      await list.settle(() => document.activeElement === listboxOf(list), 'the list to take focus back')
    } finally {
      segment.remove()
      unregister()
    }
  })

  it('is multiselectable, and says each option’s marked state, the cursor row included', async () => {
    const list = await mountList({ entries: ENTRIES, props: { selectedIndices: new SvelteSet([1]) } })
    expect(listboxOf(list).getAttribute('aria-multiselectable')).toBe('true')
    // The cursor sits on row 0, unmarked: `aria-selected` is marking, never the cursor.
    expect(list.rows().map((row) => row.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false'])
    await expectNoA11yViolations(list.target)
  })

  // ❗ VoiceOver invents a position from the RENDERED siblings for an option without one, so
  // a `..` left out read "Parent folder (1 of 8)" beside "(1 of 7)" (2026-10-15 VoiceOver spike).
  it('places every option in the whole list, the `..` row included', async () => {
    const list = await mountList({
      entries: ENTRIES,
      props: { hasParent: true, parentPath: '/', totalCount: ENTRIES.length + 1 },
    })
    await list.settle(() => list.rows().length === ENTRIES.length + 1, 'the `..` row and every entry')
    expect(list.rows().map((row) => [row.getAttribute('aria-posinset'), row.getAttribute('aria-setsize')])).toEqual([
      ['1', '4'],
      ['2', '4'],
      ['3', '4'],
      ['4', '4'],
    ])
    await expectNoA11yViolations(list.target)
  })

  it('names the cursor option by name and kind, and `..` as "Parent folder"', async () => {
    const entries = [dirEntry({ name: 'subfolder' }), ...ENTRIES]
    const live = $state<Partial<LiveListProps>>({})
    const list = await mountList({
      entries,
      liveProps: live,
      props: { hasParent: true, parentPath: '/', totalCount: entries.length + 1 },
    })
    const listbox = await focusAndLand(list)
    assertFocusModel(list.target)
    // Full view goes on with the `..` row's cells: "Parent folder, <dir>".
    expect(computeAccessibleName(cursorOption(listbox))).toMatch(/^Parent folder\b/)

    live.cursorIndex = 1
    await tick()
    assertFocusModel(list.target)
    expect(computeAccessibleName(cursorOption(listbox))).toMatch(/^subfolder \(folder\)/)
  })

  it('takes its pane’s name, and says when its cursor row lands', async () => {
    const onCursorRowLanded = vi.fn()
    const list = await mountList({
      entries: ENTRIES,
      props: { listLabel: 'Left pane, root', onCursorRowLanded },
    })
    expect(listboxOf(list).getAttribute('aria-label')).toBe('Left pane, root')
    await focusAndLand(list)
    expect(onCursorRowLanded).toHaveBeenCalledTimes(1)
  })

  // ARIA makes an option's children presentational and a listbox may hold only options, so
  // the rename field lives beside the listbox, laid over its row (`RenameOverlay`).
  it('opens the rename field outside the listbox, focused, with no a11y violations', async () => {
    const onRenameCancel = vi.fn()
    const list = await mountList({
      entries: ENTRIES,
      props: { renameState: renaming('/root/b.txt', 'b.txt'), onRenameCancel },
    })
    const input = list.target.querySelector<HTMLInputElement>('input.rename-input')
    expect(input, 'the rename field is up').not.toBeNull()
    expect(input?.closest('[role="listbox"]')).toBeNull()
    await list.settle(() => document.activeElement === input, 'the rename field to take focus')
    expect(input?.selectionEnd).toBe('b'.length)
    await expectNoA11yViolations(list.target)

    input?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(onRenameCancel).toHaveBeenCalledWith(7)
  })

  it('puts focus home when it unmounts holding it', async () => {
    const refocus = vi.fn()
    const unregister = registerFocusHome({ root: () => undefined, refocus })
    try {
      const list = await mountList({ entries: ENTRIES })
      list.component.focusList()
      refocus.mockClear()
      await unmount(list.component)
      await tick()
      expect(refocus).toHaveBeenCalledTimes(1)
    } finally {
      unregister()
    }
  })
}

/** A rename session open on `path`, as `rename-state.svelte.ts` holds it. */
function renaming(path: string, name: string): RenameState {
  return {
    active: true,
    target: { path, originalName: name, parentPath: '/root', isDirectory: false },
    currentName: name,
    validation: { severity: 'ok', message: '' },
    shaking: false,
    focusTrigger: 0,
    sessionId: 7,
  }
}
