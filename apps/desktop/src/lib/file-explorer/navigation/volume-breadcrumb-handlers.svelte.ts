import { tString } from '$lib/intl/messages.svelte'
import type { SmbConnectionState, VolumeInfo } from '../types'

/** Owns the keyboard-vs-mouse mode toggle for the dropdown. Mouse moves >5px exit
 *  keyboard mode (and we may want to update highlight to the item under the cursor). */
export function createKeyboardModeTracker() {
  let isKeyboardMode = $state(false)
  let lastMousePos = $state<{ x: number; y: number } | null>(null)

  function enter() {
    isKeyboardMode = true
    lastMousePos = null
  }

  function reset() {
    isKeyboardMode = false
    lastMousePos = null
  }

  /** Returns the index under the mouse cursor if we just exited keyboard mode, else null. */
  function onMouseMove(e: MouseEvent): number | null {
    if (!isKeyboardMode) return null
    if (!lastMousePos) {
      lastMousePos = { x: e.clientX, y: e.clientY }
      return null
    }
    const dx = Math.abs(e.clientX - lastMousePos.x)
    const dy = Math.abs(e.clientY - lastMousePos.y)
    if (dx <= 5 && dy <= 5) return null
    isKeyboardMode = false
    lastMousePos = null
    const volumeItem = (e.target as HTMLElement).closest('.volume-item')
    if (!volumeItem) return null
    const idx = parseInt(volumeItem.getAttribute('data-index') ?? '-1', 10)
    return idx >= 0 ? idx : null
  }

  return {
    get isKeyboardMode() {
      return isKeyboardMode
    },
    enter,
    reset,
    onMouseMove,
  }
}

/** Submenu state and open/close helpers for the "Connect directly" submenu. */
export function createSubmenuController() {
  let volumeId = $state<string | null>(null)
  let position = $state<{ top: number; left: number } | null>(null)
  let highlighted = $state(false)

  function open(vid: string, triggerEl?: HTMLElement, fromKeyboard = false) {
    volumeId = vid
    highlighted = fromKeyboard
    if (triggerEl) {
      const rect = triggerEl.getBoundingClientRect()
      position = { top: rect.top - 4, left: rect.right - 5 }
    }
  }

  function close() {
    volumeId = null
    position = null
    highlighted = false
  }

  function setHighlighted(value: boolean) {
    highlighted = value
  }

  return {
    get volumeId() {
      return volumeId
    },
    get position() {
      return position
    },
    get highlighted() {
      return highlighted
    },
    open,
    close,
    setHighlighted,
  }
}

/** Breadcrumb inline popup state (yellow os_mount indicator). */
export function createBreadcrumbPopupController() {
  let open = $state(false)

  return {
    get isOpen() {
      return open
    },
    toggle() {
      open = !open
    },
    close() {
      open = false
    },
  }
}

/** Tooltip text for the SMB connection indicator. */
export function getConnectionTooltip(state: SmbConnectionState): string {
  return state === 'direct'
    ? tString('fileExplorer.navigation.connectionTooltipDirect')
    : tString('fileExplorer.navigation.connectionTooltipSystem')
}

/** Whether a volume should show the active checkmark.
 *  Favorites never get a checkmark; actual volumes match by id. */
export function shouldShowCheckmark(volume: VolumeInfo, containingVolumeId: string | null): boolean {
  if (volume.category === 'favorite') return false
  return volume.id === containingVolumeId
}

interface DropdownKeyHandlers {
  moveHighlight: (delta: number) => void
  goHome: () => void
  goEnd: () => void
  activate: () => void
  close: () => void
  openSubmenuAtHighlight: () => void
  highlightedSupportsSubmenu: () => boolean
}

/** Handles a single key in the main dropdown (not submenu). Returns true if handled. */
export function handleDropdownKey(key: string, h: DropdownKeyHandlers): boolean {
  switch (key) {
    case 'ArrowDown':
      h.moveHighlight(1)
      return true
    case 'ArrowUp':
      h.moveHighlight(-1)
      return true
    case 'ArrowRight':
      if (h.highlightedSupportsSubmenu()) h.openSubmenuAtHighlight()
      return true
    case 'Enter':
      h.activate()
      return true
    case 'Escape':
      h.close()
      return true
    case 'Home':
      h.goHome()
      return true
    case 'End':
      h.goEnd()
      return true
    default:
      return false
  }
}

interface SubmenuKeyHandlers {
  isOpen: () => boolean
  close: () => void
  activate: () => void
}

/** Handles keyboard events when the submenu is open. Returns true if handled, null if submenu not open. */
export function handleSubmenuKey(key: string, h: SubmenuKeyHandlers): boolean | null {
  if (!h.isOpen()) return null
  switch (key) {
    case 'ArrowDown':
    case 'ArrowUp':
    case 'ArrowRight':
      return true // absorb: single-item submenu
    case 'ArrowLeft':
    case 'Escape':
      h.close()
      return true
    case 'Enter':
      h.activate()
      return true
    default:
      return null
  }
}
