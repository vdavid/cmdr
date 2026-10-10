/**
 * The controller behind the house `Menu` (`lib/ui/Menu.svelte`): everything that isn't
 * rendering. It owns open state, the anchor, the highlight, the keyboard contract,
 * keyboard-vs-pointer mode, submenus, and reorder; `Menu.svelte` owns the DOM and hands
 * back the one measurement the drag needs.
 *
 * ❗ **Not `menu.svelte.ts`.** macOS filesystems are case-insensitive, so the specifier
 * `./menu.svelte` resolves to the sibling `Menu.svelte` COMPONENT here and to the
 * controller on a case-sensitive CI runner: the same import would mean two different
 * modules on the two platforms (measured 2026-09-16, vite 8 resolving `./menu.svelte` to
 * `Menu.svelte` and yielding `createMenu is not a function`). Any `.svelte.ts` module
 * needs a name no sibling component can collide with.
 *
 * The caller hands over data and gets callbacks back: the controller keeps no copy of the
 * sections and persists nothing. Keys arrive through a document-level CAPTURE listener that
 * lives only while the menu is open, which is what makes them deterministic regardless of
 * where focus landed — the model `file-explorer/pane/enter-menu.svelte.ts` proved.
 */

import type {
  MenuActivationSource,
  MenuAnchor,
  MenuDisclosureChange,
  MenuItem,
  MenuReorder,
  MenuSection,
} from './menu-types'
import {
  disclosureOf,
  disclosureRowValue,
  itemByAccelerator,
  itemOf,
  menuKeyAction,
  navigableValues,
  nextValue,
  sectionOf,
  type MenuAction,
  type MenuKeyContext,
} from './menu-navigation'
import { clampedReorderTarget, moveItem, pointerInsertionSlot, pointerReorderTarget } from './menu-reorder'

/** Below this many pixels of pointer travel, a mouseup is a plain click (activate), not a drag. */
const DRAG_THRESHOLD_PX = 4

/**
 * Every open menu, in the order they opened, so the one on top owns the keyboard.
 *
 * ❗ A menu can be opened from INSIDE another (the drive-index badge sits in a volume-switcher
 * row and opens its own). Both hold a document-level CAPTURE listener, and `stopPropagation`
 * never stops a listener on the same node, so without this stack one Enter would activate a
 * row in each menu and one Escape would close the pair. The topmost is also the only one that
 * should be swallowing keys; the one underneath is inert until it's on top again.
 */
const openMenus: object[] = []

function pushOpen(menu: object): void {
  dropOpen(menu)
  openMenus.push(menu)
}

function dropOpen(menu: object): void {
  const at = openMenus.indexOf(menu)
  if (at !== -1) openMenus.splice(at, 1)
}

function isTopmost(menu: object): boolean {
  return openMenus[openMenus.length - 1] === menu
}

/** Pointer travel that ends keyboard mode, so a resting mouse can't steal the cursor. */
const KEYBOARD_MODE_EXIT_PX = 5

export interface MenuDeps<T = unknown> {
  /** Read live on every access, so the menu tracks the caller's state with no syncing. */
  getSections: () => MenuSection<T>[]
  /** `source` says whether a click, Enter, or an accelerator did it; ignore it unless you care. */
  onSelect: (item: MenuItem<T>, source: MenuActivationSource) => void
  /** Fires once, on drop or on a ⌥↑/⌥↓ that actually moves something. The caller persists. */
  onReorder?: (reorder: MenuReorder) => void
  /**
   * A section's disclosure row was flipped (a click, Enter or Space, → or ←). The menu stays
   * open and keeps no copy of the state: the caller applies it to what `getSections` returns.
   */
  onDisclosureChange?: (change: MenuDisclosureChange) => void
  /** The caller's first look at every key while open. Return true to claim it. */
  onKey?: (event: KeyboardEvent) => boolean
  /** While true an inline editor owns every keystroke, and drag is off. */
  isEditing?: () => boolean
  onOpenChange?: (open: boolean) => void
  /** Called on close, so the caller can put focus back where it was. */
  restoreFocus?: () => void
  /**
   * The anchor's whole control cluster: a pointer-down anywhere inside it is INSIDE the menu,
   * so it doesn't close. The anchor element alone is exempt already; this covers the controls
   * BESIDE it, which a pointer-down would otherwise close the menu on before they act.
   */
  keepOpenWithin?: () => HTMLElement | null | undefined
}

/** What `Menu.svelte` registers so the controller can do drag math against real rows. */
export interface MenuSurfaceHooks {
  /** The vertical midpoint of each row in a section, in display order. */
  getRowMidpoints: (sectionId: string) => number[]
}

/** The wiring `Menu.svelte` drives. Consumers never touch this; they use the surface above it. */
export interface MenuSurface {
  hover: (value: string) => void
  /**
   * A pointer move over the surface. `valueUnderPointer` is the row it happened over (null
   * between rows): leaving keyboard mode hands it the cursor, so the `:hover` paint that
   * comes back can't light a second row.
   */
  pointerMoved: (event: MouseEvent, valueUnderPointer?: string | null) => void
  activate: (value: string) => void
  /**
   * A right-click on a row: the second door to its submenu, opened as a hover would, so
   * right-click and `→` show the same rows. A row without a submenu has nothing to offer.
   */
  contextMenu: (value: string) => void
  startDrag: (value: string, event: MouseEvent) => void
  openSubmenu: (value: string, fromKeyboard: boolean) => void
  closeSubmenu: () => void
  /** Pointer hover inside an open submenu: moves ITS cursor to that row, unless it's disabled. */
  hoverSubmenu: (value: string) => void
  bindSurface: (hooks: MenuSurfaceHooks) => void
}

export interface MenuController<T = unknown> {
  readonly isOpen: boolean
  readonly anchor: MenuAnchor | null
  /** The anchor's control cluster, if the caller named one: a pointer-down in it isn't outside. */
  readonly keepOpenWithin: HTMLElement | null
  readonly sections: MenuSection<T>[]
  readonly highlightedValue: string | null
  /** True once a key moved the cursor: the surface suppresses `:hover` so there's one cursor. */
  readonly keyboardMode: boolean
  readonly openSubmenuValue: string | null
  /** The row an open submenu's own cursor sits on, or null while it shows no cursor. */
  readonly submenuHighlightedValue: string | null
  /** Whether the open submenu shows a cursor at all. */
  readonly submenuHighlighted: boolean
  /** The single-cursor rule: an open submenu takes the parent row's highlight. */
  readonly parentHighlightSuppressed: boolean
  readonly draggingValue: string | null
  /** The insertion gap the drop-line cue sits at, or null when a drop would change nothing. */
  readonly dropSlot: number | null
  readonly draggingSectionId: string | null
  readonly surface: MenuSurface
  openUnder: (element: HTMLElement) => void
  openAt: (point: { x: number; y: number }) => void
  toggleUnder: (element: HTMLElement) => void
  close: () => void
  /** Move the cursor (open-at-current-item). A disabled or unknown row is refused. */
  highlight: (value: string | null) => void
  /** Route one keydown. Returns true when the menu consumed it. */
  handleKey: (event: KeyboardEvent) => boolean
  destroy: () => void
}

export function createMenu<T = unknown>(deps: MenuDeps<T>): MenuController<T> {
  let open = $state(false)
  let anchor = $state<MenuAnchor | null>(null)
  let highlightedValue = $state<string | null>(null)
  let keyboardMode = $state(false)
  let openSubmenuValue = $state<string | null>(null)
  // The submenu's cursor is a VALUE, like the parent list's: a boolean would light every row
  // of a multi-item submenu, which the one-item "Connect directly" happened to hide.
  let submenuHighlightedValue = $state<string | null>(null)
  let draggingValue = $state<string | null>(null)
  let draggingSectionId = $state<string | null>(null)
  let dropSlot = $state<number | null>(null)

  // Not reactive: read by handlers, never rendered.
  let lastPointerPos: { x: number; y: number } | null = null
  let hooks: MenuSurfaceHooks | null = null
  let pendingDrag: { value: string; sectionId: string; startY: number } | null = null
  let dragActive = false
  /** A finished drag must not also activate the row when the browser's click lands after mouseup. */
  let justDragged = false

  const sections = (): MenuSection<T>[] => deps.getSections()

  /** The surface's measurement, or nothing when no surface is mounted (a controller-only test). */
  function rowMidpoints(sectionId: string): number[] {
    return hooks ? hooks.getRowMidpoints(sectionId) : []
  }

  // ── The document capture listener, live only while open ───────────────
  // It catches keydowns wherever focus landed (the menu, or the host behind it) and stops
  // them before the app's own dispatch sees them. Focus timing can't race it.
  let keyListenerAttached = false
  function onDocumentKeydown(event: KeyboardEvent): void {
    controller.handleKey(event)
  }
  function attachKeyListener(): void {
    if (keyListenerAttached || typeof document === 'undefined') return
    document.addEventListener('keydown', onDocumentKeydown, true)
    keyListenerAttached = true
  }
  function detachKeyListener(): void {
    if (!keyListenerAttached || typeof document === 'undefined') return
    document.removeEventListener('keydown', onDocumentKeydown, true)
    keyListenerAttached = false
  }

  function enterKeyboardMode(): void {
    keyboardMode = true
    lastPointerPos = null
  }

  function isNavigable(value: string): boolean {
    return navigableValues(sections()).includes(value)
  }

  function setHighlight(value: string | null): void {
    if (value === null || isNavigable(value)) highlightedValue = value
  }

  function doOpen(next: MenuAnchor): void {
    anchor = next
    open = true
    keyboardMode = false
    lastPointerPos = null
    justDragged = false
    highlightedValue = navigableValues(sections())[0] ?? null
    attachKeyListener()
    pushOpen(controller)
    deps.onOpenChange?.(true)
  }

  function close(): void {
    if (!open) return
    open = false
    anchor = null
    highlightedValue = null
    keyboardMode = false
    closeSubmenu()
    endDrag()
    detachKeyListener()
    dropOpen(controller)
    deps.onOpenChange?.(false)
    deps.restoreFocus?.()
  }

  function closeSubmenu(): void {
    openSubmenuValue = null
    submenuHighlightedValue = null
  }

  /** The rows an open submenu offers, disabled ones skipped. */
  function submenuValues(): string[] {
    if (openSubmenuValue === null) return []
    const parent = itemOf(sections(), openSubmenuValue)
    return (parent?.submenu ?? []).filter((child) => !child.disabled).map((child) => child.value)
  }

  function openSubmenu(value: string, fromKeyboard: boolean): void {
    const item = itemOf(sections(), value)
    if (!item?.submenu?.length) return
    openSubmenuValue = value
    // A submenu opened by hovering its parent row shows no cursor until the pointer or the
    // keyboard reaches INTO it; opened by keyboard, the cursor is already there. One whose
    // every row is disabled still opens, cursorless: its greyed rows are the answer to "why
    // can't I eject this?", and hiding them would leave an arrow that leads nowhere.
    const first = item.submenu.find((child) => !child.disabled)
    submenuHighlightedValue = fromKeyboard ? (first?.value ?? null) : null
  }

  /** Walk an open submenu's own rows, wrapping; from no cursor, enter at the near end. */
  function moveSubmenu(delta: -1 | 1): void {
    const values = submenuValues()
    if (values.length === 0) return
    submenuHighlightedValue = nextValue(values, submenuHighlightedValue, delta)
    enterKeyboardMode()
  }

  /**
   * Ask the caller to open or fold a section. The cursor goes to its disclosure row, so a fold
   * never leaves it on a row that just went out of sight.
   */
  function disclose(section: MenuSection<T>, expanded: boolean): void {
    closeSubmenu()
    highlightedValue = disclosureRowValue(section.id)
    deps.onDisclosureChange?.({ sectionId: section.id, expanded })
  }

  function activate(value: string, source: MenuActivationSource): void {
    if (!open) return
    if (justDragged) {
      justDragged = false
      return
    }
    // A disclosure row flips its section and picks nothing, so the menu stays up.
    const folding = disclosureOf(sections(), value)
    if (folding) {
      disclose(folding, !folding.disclosure?.expanded)
      return
    }
    const item = itemOf(sections(), value)
    if (!item || item.disabled) return
    // A `keepsMenuOpen` pick closes only its submenu, and the parent row keeps the cursor,
    // so the next `→` goes straight back in.
    if (item.keepsMenuOpen) closeSubmenu()
    else close()
    deps.onSelect(item, source)
  }

  function activateHighlighted(): void {
    if (openSubmenuValue === null) {
      if (highlightedValue !== null) activate(highlightedValue, 'keyboard')
      return
    }
    if (submenuHighlightedValue !== null) {
      activate(submenuHighlightedValue, 'keyboard')
      return
    }
    // A hover-opened submenu shows no cursor yet; Enter still means its first row.
    const values = submenuValues()
    if (values.length > 0) activate(values[0], 'keyboard')
  }

  /** A typed digit or assigned letter. The open menu absorbs keys with no matching row. */
  function activateAccelerator(char: string): void {
    const item = itemByAccelerator(sections(), char)
    if (item) activate(item.value, /^[A-Z]$/.test(char) ? 'shortcut' : 'accelerator')
  }

  function reorderHighlighted(delta: -1 | 1): void {
    const value = highlightedValue
    if (value === null) return
    const found = sectionOf(sections(), value)
    if (!found?.section.reorderable) return
    const values = found.section.items.map((item) => item.value)
    const to = clampedReorderTarget(found.index, delta, values.length)
    if (to === null) return
    // The highlight is a VALUE, so it rides along with the moved row for free: a repeated
    // ⌥↓ keeps walking the same item without the caller re-deriving an index.
    deps.onReorder?.({
      sectionId: found.section.id,
      orderedValues: moveItem(values, found.index, to),
      from: found.index,
      to,
    })
    enterKeyboardMode()
  }

  // ── Pointer-drag reorder ──────────────────────────────────────────────
  // ❗ HTML5 drag-and-drop does NOT fire under Tauri's `dragDropEnabled`: macOS intercepts
  // drag gestures before the WKWebView sees `dragstart`/`drop`, so a `draggable` reorder
  // looks wired up and does nothing. Don't reintroduce it. Synthetic MCP/test events bypass
  // the OS interception, so "it works under MCP" is not proof it works with a real mouse.
  function onWindowMouseMove(event: MouseEvent): void {
    const pending = pendingDrag
    if (!pending) return
    if (!dragActive) {
      if (Math.abs(event.clientY - pending.startY) < DRAG_THRESHOLD_PX) return
      dragActive = true
      draggingValue = pending.value
      draggingSectionId = pending.sectionId
    }
    const found = sectionOf(sections(), pending.value)
    if (!found) return
    // The cue rides the RAW insertion slot (the visual gap), not the move target: dropping at
    // slot `from` or `from + 1` leaves the row where it is, so the cue hides there. Driving it
    // off the move target put the line one row too high on downward drags.
    const slot = pointerInsertionSlot(rowMidpoints(pending.sectionId), event.clientY)
    dropSlot = slot === found.index || slot === found.index + 1 ? null : slot
  }

  function onWindowMouseUp(event: MouseEvent): void {
    const pending = pendingDrag
    const wasDragging = dragActive
    const midpoints = pending ? rowMidpoints(pending.sectionId) : []
    endDrag()
    if (!pending) return
    if (!wasDragging) {
      // Never crossed the threshold: a plain click, so open the row.
      activate(pending.value, 'pointer')
      return
    }
    justDragged = true
    const found = sectionOf(sections(), pending.value)
    if (!found) return
    const to = pointerReorderTarget(midpoints, event.clientY, found.index)
    if (to === null) return
    const values = found.section.items.map((item) => item.value)
    deps.onReorder?.({
      sectionId: found.section.id,
      orderedValues: moveItem(values, found.index, to),
      from: found.index,
      to,
    })
  }

  function endDrag(): void {
    if (typeof window !== 'undefined') {
      window.removeEventListener('mousemove', onWindowMouseMove)
      window.removeEventListener('mouseup', onWindowMouseUp)
    }
    draggingValue = null
    draggingSectionId = null
    dropSlot = null
    dragActive = false
    pendingDrag = null
  }

  /** What the cursor is sitting on, which decides what a key means. */
  function currentKeyContext(): MenuKeyContext {
    const value = highlightedValue
    const item = value === null ? null : itemOf(sections(), value)
    const found = value === null ? null : sectionOf(sections(), value)
    const folding = value === null ? null : disclosureOf(sections(), value)
    return {
      hasSubmenu: (item?.submenu?.length ?? 0) > 0,
      submenu: openSubmenuValue === null ? 'closed' : submenuHighlightedValue === null ? 'shown' : 'entered',
      reorderable: found?.section.reorderable ?? false,
      disclosure: folding ? (folding.disclosure?.expanded ? 'expanded' : 'collapsed') : undefined,
      insideDisclosure: found?.section.disclosure !== undefined,
    }
  }

  /** Flip the section under the cursor, or step back up to the one the cursor sits inside. */
  function applyDisclosureAction(action: Extract<MenuAction, { kind: 'disclose' } | { kind: 'toDisclosure' }>): void {
    const value = highlightedValue
    if (value === null) return
    if (action.kind === 'disclose') {
      const folding = disclosureOf(sections(), value)
      if (folding) disclose(folding, action.expanded)
    } else {
      const found = sectionOf(sections(), value)
      if (found?.section.disclosure) setHighlight(disclosureRowValue(found.section.id))
    }
    enterKeyboardMode()
  }

  /**
   * Move the parent list's cursor: one step from where it is, or to an end. A hover-opened
   * submenu belongs to the row the cursor leaves, so it closes.
   */
  function applyCursorMove(action: Extract<MenuAction, { kind: 'move' } | { kind: 'edge' }>): void {
    closeSubmenu()
    const values = navigableValues(sections())
    if (action.kind === 'move') {
      setHighlight(nextValue(values, highlightedValue, action.delta))
    } else {
      // Nothing to land on (every section empty or disabled): leave the cursor alone.
      if (values.length === 0) return
      setHighlight(action.edge === 'first' ? values[0] : values[values.length - 1])
    }
    enterKeyboardMode()
  }

  /** Open a submenu, close it, or walk its own rows. */
  function applySubmenuAction(
    action: Extract<MenuAction, { kind: 'openSubmenu' } | { kind: 'closeSubmenu' } | { kind: 'moveSubmenu' }>,
  ): void {
    if (action.kind === 'moveSubmenu') {
      moveSubmenu(action.delta)
      return
    }
    if (action.kind === 'openSubmenu') {
      if (highlightedValue !== null) openSubmenu(highlightedValue, true)
    } else {
      closeSubmenu()
    }
    enterKeyboardMode()
  }

  /**
   * Carry out what `menuKeyAction` decided. The event is already claimed by the caller.
   *
   * Every arm is one call, deliberately: a single body deciding every class of key ran past the
   * complexity cap once accelerators joined it, and the cap was right — the cursor moves and the
   * submenu moves are separate decisions that just happened to share a switch. Keep the dispatch
   * flat and put any new branching in a named helper.
   */
  function applyAction(action: MenuAction): void {
    switch (action.kind) {
      case 'move':
      case 'edge':
        applyCursorMove(action)
        return
      case 'openSubmenu':
      case 'closeSubmenu':
      case 'moveSubmenu':
        applySubmenuAction(action)
        return
      case 'activate':
        activateHighlighted()
        return
      case 'close':
        close()
        return
      case 'reorder':
        reorderHighlighted(action.delta)
        return
      case 'accelerator':
        activateAccelerator(action.char)
        return
      case 'disclose':
      case 'toDisclosure':
        applyDisclosureAction(action)
        return
      case 'absorb':
      case 'none':
        return
    }
  }

  const surface: MenuSurface = {
    hover(value) {
      // ❗ The whole hover is off in keyboard mode, the submenu half included: a resting
      // mouse over a row with a submenu would otherwise open it and take the cursor.
      if (keyboardMode) return
      setHighlight(value)
      if (itemOf(sections(), value)?.submenu?.length) openSubmenu(value, false)
      else closeSubmenu()
    },
    pointerMoved(event, valueUnderPointer = null) {
      if (!keyboardMode) return
      if (!lastPointerPos) {
        lastPointerPos = { x: event.clientX, y: event.clientY }
        return
      }
      const dx = Math.abs(event.clientX - lastPointerPos.x)
      const dy = Math.abs(event.clientY - lastPointerPos.y)
      if (dx <= KEYBOARD_MODE_EXIT_PX && dy <= KEYBOARD_MODE_EXIT_PX) return
      keyboardMode = false
      lastPointerPos = null
      // ❗ The cursor goes to the row the pointer is already on. Without this there would be
      // two: `:hover` starts painting again the moment keyboard mode drops, and no
      // `mouseover` is coming for a row the pointer never left.
      if (valueUnderPointer !== null) setHighlight(valueUnderPointer)
    },
    // Everything that reaches the surface's `activate` is a click: the keyboard paths
    // go through `handleKey` and never come back out here.
    activate(value) {
      activate(value, 'pointer')
    },
    contextMenu(value) {
      if (!itemOf(sections(), value)?.submenu?.length) {
        closeSubmenu()
        return
      }
      setHighlight(value)
      openSubmenu(value, false)
    },
    startDrag(value, event) {
      if (event.button !== 0 || deps.isEditing?.()) return
      const found = sectionOf(sections(), value)
      if (!found?.section.reorderable) return
      pendingDrag = { value, sectionId: found.section.id, startY: event.clientY }
      dragActive = false
      if (typeof window === 'undefined') return
      window.addEventListener('mousemove', onWindowMouseMove)
      window.addEventListener('mouseup', onWindowMouseUp)
    },
    openSubmenu,
    closeSubmenu,
    hoverSubmenu(value) {
      if (submenuValues().includes(value)) submenuHighlightedValue = value
    },
    bindSurface(next) {
      hooks = next
    },
  }

  const controller: MenuController<T> = {
    get isOpen() {
      return open
    },
    get anchor() {
      return anchor
    },
    get keepOpenWithin() {
      return deps.keepOpenWithin?.() ?? null
    },
    get sections() {
      return sections()
    },
    get highlightedValue() {
      return highlightedValue
    },
    get keyboardMode() {
      return keyboardMode
    },
    get openSubmenuValue() {
      return openSubmenuValue
    },
    get submenuHighlightedValue() {
      return submenuHighlightedValue
    },
    get submenuHighlighted() {
      return submenuHighlightedValue !== null
    },
    get parentHighlightSuppressed() {
      return openSubmenuValue !== null
    },
    get draggingValue() {
      return draggingValue
    },
    get dropSlot() {
      return dropSlot
    },
    get draggingSectionId() {
      return draggingSectionId
    },
    surface,
    openUnder(element) {
      doOpen({ kind: 'element', element })
    },
    openAt(point) {
      doOpen({ kind: 'point', x: point.x, y: point.y })
    },
    toggleUnder(element) {
      if (open) close()
      else doOpen({ kind: 'element', element })
    },
    close,
    highlight: setHighlight,
    handleKey(event) {
      if (!open) return false
      // A menu opened from inside this one owns the keyboard until it closes. Both listen on
      // the document, so without this the key would be handled twice over.
      if (!isTopmost(controller)) return false
      // An inline editor owns every keystroke, untouched: not even swallowed, or the field
      // would lose the keys it exists to receive.
      if (deps.isEditing?.()) return false
      if (deps.onKey?.(event)) {
        // A claimed key ends here like every other key an open menu handles, or it would be
        // the ONE class that escapes to the app's dispatch and fires twice. `stopPropagation`
        // only, never `preventDefault`: a consumer key can be a menu-bar accelerator
        // (`favorites.open` is), and those keep meaning what they mean.
        event.stopPropagation()
        return true
      }

      const action = menuKeyAction(event, currentKeyContext())
      if (
        action.kind === 'none' ||
        (action.kind === 'accelerator' && /^[A-Z]$/.test(action.char) && !itemByAccelerator(sections(), action.char))
      ) {
        // An open menu owns the keyboard, which is what keeps the panes behind it inert.
        // Swallowed from the app, but never `preventDefault`ed: ⌘Q and the menu-bar
        // accelerators still mean what they mean.
        event.stopPropagation()
        return true
      }
      event.preventDefault()
      event.stopPropagation()
      applyAction(action)
      return true
    },
    destroy() {
      detachKeyListener()
      endDrag()
      // A menu torn down while open would otherwise sit on top of the stack forever, leaving
      // the menu underneath it inert.
      dropOpen(controller)
    },
  }

  return controller
}
