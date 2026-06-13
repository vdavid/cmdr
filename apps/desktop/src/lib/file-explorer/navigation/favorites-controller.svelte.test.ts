/**
 * Unit tests for the favorites interaction controller extracted from `VolumeBreadcrumb.svelte`.
 *
 * This file uses Svelte runes (`$effect.root`) to instantiate the rune-based factory outside a
 * component, so the filename carries the `.svelte.test.ts` suffix. The component-level behavior
 * (keyboard reorder, the rename keyboard guard) is pinned by `VolumeBreadcrumb.svelte.test.ts` and
 * `pane/volume-breadcrumb.test.ts`; here we cover the pointer-drag reorder, rename, and remove paths
 * directly so the local-first / click-vs-drag logic stays tested as its own unit.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'

const reorderFavorites = vi.fn(() => Promise.resolve())
const removeFavorite = vi.fn(() => Promise.resolve())
const renameFavorite = vi.fn(() => Promise.resolve())
const addToast = vi.fn(() => 'toast-id')

vi.mock('$lib/tauri-commands', () => ({
  reorderFavorites: (...args: unknown[]) => reorderFavorites(...(args as [])),
  removeFavorite: (...args: unknown[]) => removeFavorite(...(args as [])),
  renameFavorite: (...args: unknown[]) => renameFavorite(...(args as [])),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: (...args: unknown[]) => addToast(...(args as [])) }))

import { createFavoritesController } from './favorites-controller.svelte'
import type { VolumeInfo } from '../types'

function fav(id: string, name = id): VolumeInfo {
  return { id, name, path: `/Users/test/${name}`, category: 'favorite', isEjectable: false }
}

/** Builds a dropdown root with a `.favorite-item[data-fav-id]` row per favorite, each row's
 *  `getBoundingClientRect` stubbed to a 20px-tall slot stacked from y=0, so the pointer-drag
 *  midpoint math (`favoriteRowMidpoints`) returns deterministic values. */
function buildDropdown(favorites: VolumeInfo[]): HTMLDivElement {
  const root = document.createElement('div')
  favorites.forEach((f, i) => {
    const row = document.createElement('div')
    row.className = 'favorite-item'
    row.setAttribute('data-fav-id', f.id)
    const top = i * 20
    row.getBoundingClientRect = () => ({
      top,
      height: 20,
      bottom: top + 20,
      left: 0,
      right: 100,
      width: 100,
      x: 0,
      y: top,
      toJSON: () => ({}),
    })
    root.appendChild(row)
  })
  return root
}

describe('favorites-controller', () => {
  let dispose: (() => void) | undefined
  let favorites: VolumeInfo[]
  let dropdown: HTMLDivElement
  let renameInput: HTMLInputElement | undefined
  const navigate = vi.fn<(v: VolumeInfo) => void>()

  function create(initialFavorites: VolumeInfo[]) {
    favorites = [...initialFavorites]
    dropdown = buildDropdown(favorites)
    renameInput = document.createElement('input')
    let controller!: ReturnType<typeof createFavoritesController>
    dispose = $effect.root(() => {
      controller = createFavoritesController({
        getFavorites: () => favorites,
        getVolumes: () => favorites,
        getDropdownRef: () => dropdown,
        getRenameInputRef: () => renameInput,
        navigate,
      })
    })
    return controller
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  describe('pointer-drag reorder', () => {
    it('treats a mousedown+mouseup below the threshold as a plain click (navigate, no reorder)', () => {
      const c = create([fav('fav-1'), fav('fav-2'), fav('fav-3')])
      c.handleMouseDown(favorites[0], new MouseEvent('mousedown', { button: 0, clientY: 10 }))
      // Move 2px (< DRAG_THRESHOLD_PX of 4): still a click.
      window.dispatchEvent(new MouseEvent('mousemove', { clientY: 12 }))
      window.dispatchEvent(new MouseEvent('mouseup', { clientY: 12 }))
      expect(navigate).toHaveBeenCalledWith(favorites[0])
      expect(reorderFavorites).not.toHaveBeenCalled()
      expect(c.draggingFavoriteId).toBe(null)
    })

    it('crossing the threshold begins a drag and dropping past a row persists the reordered list', () => {
      const c = create([fav('fav-1'), fav('fav-2'), fav('fav-3')])
      c.handleMouseDown(favorites[0], new MouseEvent('mousedown', { button: 0, clientY: 10 }))
      // Drag well past the threshold, down to below row index 1 (midpoint 30): marks dragging.
      window.dispatchEvent(new MouseEvent('mousemove', { clientY: 35 }))
      expect(c.draggingFavoriteId).toBe('fav-1')
      // Drop at y=35: below midpoints 10 and 30 → slot 2, move target for from=0 is 1.
      window.dispatchEvent(new MouseEvent('mouseup', { clientY: 35 }))
      expect(navigate).not.toHaveBeenCalled()
      expect(reorderFavorites).toHaveBeenCalledTimes(1)
      expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
      // Optimistic order set synchronously, drag scratch cleared.
      expect(c.optimisticFavoriteIds).toEqual(['fav-2', 'fav-1', 'fav-3'])
      expect(c.draggingFavoriteId).toBe(null)
      expect(c.dragOverIndex).toBe(null)
    })

    it('a drag that lands on the same slot is a no-op (no persist)', () => {
      const c = create([fav('fav-1'), fav('fav-2'), fav('fav-3')])
      c.handleMouseDown(favorites[0], new MouseEvent('mousedown', { button: 0, clientY: 10 }))
      window.dispatchEvent(new MouseEvent('mousemove', { clientY: 18 }))
      // Drop back at y=5: above all midpoints → slot 0 == from, no reorder.
      window.dispatchEvent(new MouseEvent('mouseup', { clientY: 5 }))
      expect(reorderFavorites).not.toHaveBeenCalled()
    })

    it('ignores a non-left mousedown', () => {
      const c = create([fav('fav-1'), fav('fav-2')])
      c.handleMouseDown(favorites[0], new MouseEvent('mousedown', { button: 2, clientY: 10 }))
      window.dispatchEvent(new MouseEvent('mousemove', { clientY: 50 }))
      expect(c.draggingFavoriteId).toBe(null)
    })

    it('reverts the optimistic order when the background persist rejects', async () => {
      reorderFavorites.mockRejectedValueOnce(new Error('nope'))
      const c = create([fav('fav-1'), fav('fav-2'), fav('fav-3')])
      const newIndex = c.reorderHighlighted(favorites[0], 1)
      expect(newIndex).toBe(1)
      expect(c.optimisticFavoriteIds).toEqual(['fav-2', 'fav-1', 'fav-3'])
      await tick()
      await Promise.resolve()
      expect(c.optimisticFavoriteIds).toBe(null)
      expect(addToast).toHaveBeenCalledWith("Couldn't reorder favorites. Try again?", { level: 'error' })
    })
  })

  describe('keyboard reorder', () => {
    it('returns the new index and persists when moving down', () => {
      const c = create([fav('fav-1'), fav('fav-2'), fav('fav-3')])
      expect(c.reorderHighlighted(favorites[0], 1)).toBe(1)
      expect(reorderFavorites).toHaveBeenCalledWith(['2', '1', '3'])
    })

    it('returns null and does not persist at the top edge', () => {
      const c = create([fav('fav-1'), fav('fav-2')])
      expect(c.reorderHighlighted(favorites[0], -1)).toBe(null)
      expect(reorderFavorites).not.toHaveBeenCalled()
    })
  })

  describe('rename', () => {
    it('startRename focuses and selects the rename input after a tick', async () => {
      const c = create([fav('fav-1', 'Docs')])
      const focusSpy = vi.spyOn(renameInput as HTMLInputElement, 'focus')
      const selectSpy = vi.spyOn(renameInput as HTMLInputElement, 'select')
      c.startRename(favorites[0])
      expect(c.renamingFavoriteId).toBe('fav-1')
      expect(c.renameDraft).toBe('Docs')
      await tick()
      expect(focusSpy).toHaveBeenCalled()
      expect(selectSpy).toHaveBeenCalled()
    })

    it('commitRename persists the trimmed new name with the bare id, then clears state', async () => {
      const c = create([fav('fav-1', 'Docs')])
      c.startRename(favorites[0])
      c.renameDraft = '  Projects  '
      await c.commitRename(favorites[0])
      expect(renameFavorite).toHaveBeenCalledWith('1', 'Projects')
      expect(c.renamingFavoriteId).toBe(null)
      expect(c.renameDraft).toBe('')
    })

    it('commitRename skips the IPC when the name is unchanged', async () => {
      const c = create([fav('fav-1', 'Docs')])
      c.startRename(favorites[0])
      await c.commitRename(favorites[0])
      expect(renameFavorite).not.toHaveBeenCalled()
    })

    it('cancelRename clears the draft without persisting', () => {
      const c = create([fav('fav-1', 'Docs')])
      c.startRename(favorites[0])
      c.cancelRename()
      expect(c.renamingFavoriteId).toBe(null)
      expect(renameFavorite).not.toHaveBeenCalled()
    })

    it('handleRenameKeyDown stops propagation for every key and commits on Enter', () => {
      const c = create([fav('fav-1', 'Docs')])
      c.startRename(favorites[0])
      c.renameDraft = 'New'
      const enter = new KeyboardEvent('keydown', { key: 'Enter' })
      const stop = vi.spyOn(enter, 'stopPropagation')
      c.handleRenameKeyDown(enter, favorites[0])
      expect(stop).toHaveBeenCalled()
      expect(renameFavorite).toHaveBeenCalledWith('1', 'New')
    })

    it('handleRenameKeyDown cancels on Escape', () => {
      const c = create([fav('fav-1', 'Docs')])
      c.startRename(favorites[0])
      c.handleRenameKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }), favorites[0])
      expect(c.renamingFavoriteId).toBe(null)
    })
  })

  describe('remove', () => {
    it('calls removeFavorite with the bare id', async () => {
      const c = create([fav('fav-1')])
      await c.remove(favorites[0])
      expect(removeFavorite).toHaveBeenCalledWith('1')
    })

    it('shows a toast when removal rejects', async () => {
      removeFavorite.mockRejectedValueOnce(new Error('nope'))
      const c = create([fav('fav-1')])
      await c.remove(favorites[0])
      expect(addToast).toHaveBeenCalledWith("Couldn't remove that favorite. Try again?", { level: 'error' })
    })
  })

  describe('destroy', () => {
    it('removes window drag listeners (a post-destroy mousemove does not start a drag)', () => {
      const c = create([fav('fav-1'), fav('fav-2')])
      c.handleMouseDown(favorites[0], new MouseEvent('mousedown', { button: 0, clientY: 10 }))
      c.destroy()
      window.dispatchEvent(new MouseEvent('mousemove', { clientY: 80 }))
      expect(c.draggingFavoriteId).toBe(null)
    })
  })
})
