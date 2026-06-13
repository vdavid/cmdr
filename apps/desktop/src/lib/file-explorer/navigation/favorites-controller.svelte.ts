import { tick } from 'svelte'
import { removeFavorite, renameFavorite, reorderFavorites, stripFavoritePrefix } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { moveItem, clampedReorderTarget, pointerReorderTarget, pointerInsertionSlot } from './favorites-reorder'
import type { VolumeInfo } from '../types'

/** Below this many pixels of pointer travel, a mouseup is a plain click (navigate), not a drag. */
const DRAG_THRESHOLD_PX = 4

export interface FavoritesControllerDeps {
  /** The favorites in current display order (the component's `favorites` derived list). */
  getFavorites: () => VolumeInfo[]
  /** The full store volume list, for reconciling the optimistic order against store truth. */
  getVolumes: () => VolumeInfo[]
  /** The dropdown root element, used to measure favorite rows for pointer-drag reorder. */
  getDropdownRef: () => HTMLElement | undefined
  /** The inline rename `<input>`, focused + selected when a rename starts. */
  getRenameInputRef: () => HTMLInputElement | undefined
  /** Navigate to a favorite (the component's `handleVolumeSelect`). */
  navigate: (volume: VolumeInfo) => void
}

export interface FavoritesController {
  /** Optimistic favorite-id order override (null = render the store order). Read by the component's
   *  `effectiveVolumes` / `favorites` deriveds so a reorder shows instantly, before the IPC round-trip. */
  get optimisticFavoriteIds(): string[] | null
  get draggingFavoriteId(): string | null
  get dragOverIndex(): number | null
  get renamingFavoriteId(): string | null
  get renameDraft(): string
  set renameDraft(value: string)
  remove: (volume: VolumeInfo) => Promise<void>
  startRename: (volume: VolumeInfo) => void
  cancelRename: () => void
  commitRename: (volume: VolumeInfo) => Promise<void>
  handleRenameKeyDown: (e: KeyboardEvent, volume: VolumeInfo) => void
  handleMouseDown: (volume: VolumeInfo, e: MouseEvent) => void
  /** Keyboard reorder (Alt+↑/↓) of the highlighted favorite by ±1. Returns the favorite's new
   *  index so the caller can follow the moved item with the dropdown highlight, or null on no-op. */
  reorderHighlighted: (volume: VolumeInfo, delta: -1 | 1) => number | null
  destroy: () => void
}

export function createFavoritesController(deps: FavoritesControllerDeps): FavoritesController {
  // Optimistic favorite order for instant, local-first reorder. A keyboard (Alt+↑/↓) or pointer
  // reorder sets this to the new order of favorite ids SYNCHRONOUSLY, so the switcher re-renders
  // immediately and a rapid next press computes against fresh state; the backend persist runs in the
  // background. Reconciled to `null` once `volumes-changed` brings the store to the same order (or
  // the favorite set changes elsewhere). `null` = no override, render the store order.
  let optimisticFavoriteIds = $state<string[] | null>(null)

  // ── Inline rename ────────────────────────────────────────────────────
  let renamingFavoriteId = $state<string | null>(null)
  let renameDraft = $state('')

  // ── Pointer-drag reorder scratch state ───────────────────────────────
  let draggingFavoriteId = $state<string | null>(null)
  let dragOverIndex = $state<number | null>(null)
  // Set once the threshold is crossed; before that a mouseup is a plain click.
  let dragActive = false
  let dragStartY = 0
  let pendingDragFavorite: VolumeInfo | null = null

  // Drop the optimistic order once the store catches up to it (the persisted `volumes-changed`
  // landed), or if the favorite set changed elsewhere (add / remove) so the override is stale.
  $effect(() => {
    const order = optimisticFavoriteIds
    if (!order) return
    const storeFavIds = deps
      .getVolumes()
      .filter((v) => v.category === 'favorite')
      .map((v) => v.id)
    const sameSet = storeFavIds.length === order.length && storeFavIds.every((id) => order.includes(id))
    const sameOrder = sameSet && storeFavIds.every((id, i) => id === order[i])
    if (sameOrder || !sameSet) optimisticFavoriteIds = null
  })

  async function remove(volume: VolumeInfo): Promise<void> {
    try {
      await removeFavorite(stripFavoritePrefix(volume.id))
    } catch {
      addToast("Couldn't remove that favorite. Try again?", { level: 'error' })
    }
  }

  function startRename(volume: VolumeInfo) {
    renamingFavoriteId = volume.id
    renameDraft = volume.name
    void tick().then(() => {
      const input = deps.getRenameInputRef()
      input?.focus()
      input?.select()
    })
  }

  function cancelRename() {
    renamingFavoriteId = null
    renameDraft = ''
  }

  async function commitRename(volume: VolumeInfo): Promise<void> {
    const trimmed = renameDraft.trim()
    const id = renamingFavoriteId
    cancelRename()
    if (!id || !trimmed || trimmed === volume.name) return
    try {
      await renameFavorite(stripFavoritePrefix(id), trimmed)
    } catch {
      addToast("Couldn't rename that favorite. Try again?", { level: 'error' })
    }
  }

  function handleRenameKeyDown(e: KeyboardEvent, volume: VolumeInfo) {
    // The focused rename `<input>` owns every keystroke. Stop ALL keys from
    // bubbling to the pane's DOM listeners (Space-selection, type-to-jump,
    // etc.); the dispatch-level guards don't cover the raw DOM Space handler,
    // so without this a Space typed into the box also selects the file under
    // the cursor. Enter commits, Escape cancels, everything else edits the text.
    e.stopPropagation()
    if (e.key === 'Enter') {
      e.preventDefault()
      void commitRename(volume)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      cancelRename()
    }
  }

  // ── Pointer-drag reorder within the Favorites section ───────────────
  // HTML5 drag-and-drop does NOT fire under Tauri's `dragDropEnabled` (the OS
  // intercepts drag gestures before the webview sees `dragstart`/`drop`), so
  // we roll our own with pointer events, mirroring the native file-list drag.
  // `mousedown` on a favorite row records the grabbed id + start Y and arms
  // window-level move/up listeners; an actual reorder begins only once the
  // pointer crosses a small threshold, so a plain click still navigates.
  // `dragOverIndex` is the live insertion slot driving the drop-line cue.

  /** Midpoints of each favorite row in list order, for `pointerReorderTarget`. */
  function favoriteRowMidpoints(): number[] {
    const root = deps.getDropdownRef()
    if (!root) return []
    return deps.getFavorites().map((f) => {
      const el = root.querySelector(`.favorite-item[data-fav-id="${CSS.escape(f.id)}"]`)
      if (!el) return Number.POSITIVE_INFINITY
      const rect = el.getBoundingClientRect()
      return rect.top + rect.height / 2
    })
  }

  function handleMouseDown(volume: VolumeInfo, e: MouseEvent) {
    // Left button only; never start a drag from the inline rename input.
    if (e.button !== 0 || renamingFavoriteId === volume.id) return
    pendingDragFavorite = volume
    dragStartY = e.clientY
    dragActive = false
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
  }

  function handleMouseMove(e: MouseEvent) {
    const grabbed = pendingDragFavorite
    if (!grabbed) return
    if (!dragActive) {
      if (Math.abs(e.clientY - dragStartY) < DRAG_THRESHOLD_PX) return
      // Threshold crossed: begin the reorder.
      dragActive = true
      draggingFavoriteId = grabbed.id
    }
    const favorites = deps.getFavorites()
    const from = favorites.findIndex((f) => f.id === grabbed.id)
    if (from < 0) return
    // Drive the cue off the RAW insertion slot (the visual gap), not the move-target: dropping at
    // slot `from` or `from + 1` leaves the item in place, so hide the cue then (matches when the
    // drop's `pointerReorderTarget` returns null). Using the move-target here put the line one row
    // too high on downward drags.
    const slot = pointerInsertionSlot(favoriteRowMidpoints(), e.clientY)
    dragOverIndex = slot === from || slot === from + 1 ? null : slot
  }

  function handleMouseUp(e: MouseEvent) {
    window.removeEventListener('mousemove', handleMouseMove)
    window.removeEventListener('mouseup', handleMouseUp)
    const grabbed = pendingDragFavorite
    const wasDragging = dragActive
    endDrag()
    if (!grabbed) return
    if (!wasDragging) {
      // Never crossed the threshold: treat as a plain click → navigate.
      if (renamingFavoriteId !== grabbed.id) deps.navigate(grabbed)
      return
    }
    const ids = deps.getFavorites().map((f) => f.id)
    const from = ids.indexOf(grabbed.id)
    if (from < 0) return
    const to = pointerReorderTarget(favoriteRowMidpoints(), e.clientY, from)
    if (to === null) return
    persistOrder(moveItem(ids, from, to))
  }

  function endDrag() {
    draggingFavoriteId = null
    dragOverIndex = null
    dragActive = false
    dragStartY = 0
    pendingDragFavorite = null
  }

  function reorderHighlighted(volume: VolumeInfo, delta: -1 | 1): number | null {
    const ids = deps.getFavorites().map((f) => f.id)
    const from = ids.indexOf(volume.id)
    const to = clampedReorderTarget(from, delta, ids.length)
    if (to === null) return null
    persistOrder(moveItem(ids, from, to))
    return to
  }

  /** Local-first reorder: show the new order instantly via the optimistic override, then persist in
   *  the background. On failure, drop the override so the UI reverts to the store truth. */
  function persistOrder(orderedLocationIds: string[]) {
    optimisticFavoriteIds = orderedLocationIds
    void reorderFavorites(orderedLocationIds.map(stripFavoritePrefix)).catch(() => {
      addToast("Couldn't reorder favorites. Try again?", { level: 'error' })
      optimisticFavoriteIds = null
    })
  }

  function destroy() {
    // Tear down any in-flight pointer-drag listeners if the component unmounts mid-drag.
    window.removeEventListener('mousemove', handleMouseMove)
    window.removeEventListener('mouseup', handleMouseUp)
  }

  return {
    get optimisticFavoriteIds() {
      return optimisticFavoriteIds
    },
    get draggingFavoriteId() {
      return draggingFavoriteId
    },
    get dragOverIndex() {
      return dragOverIndex
    },
    get renamingFavoriteId() {
      return renamingFavoriteId
    },
    get renameDraft() {
      return renameDraft
    },
    set renameDraft(value: string) {
      renameDraft = value
    },
    remove,
    startRename,
    cancelRename,
    commitRename,
    handleRenameKeyDown,
    handleMouseDown,
    reorderHighlighted,
    destroy,
  }
}
