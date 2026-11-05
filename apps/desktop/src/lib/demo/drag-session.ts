/**
 * The OS's drag session, for a page. On desktop, `start_drag_paths` hands the files to AppKit, which
 * then owns the mouse and tells the webview what it does through Tauri's drag-drop events
 * (`tauri://drag-enter`, `-over`, `-drop`, `-leave`) plus the swizzle's `drag-modifiers`. Here the
 * answer to `start_drag_paths` follows the pointer that's still down and sends those same events to
 * the main window, so the app's drag controller (`file-explorer/pane/drag-drop-controller.svelte.ts`)
 * runs unchanged: its hit test, overlay, auto-scroll, and the real transfer dialog. See `DETAILS.md`
 * § Drag and drop.
 *
 * Files dragged in from the computer are `os-drop.ts`'s: a browser hands those over as HTML
 * drag events, not a pointer this follows.
 *
 * A finger works the same way: the app arms a touch drag on a hold (`file-explorer/drag/touch-hold.ts`)
 * and keeps the page from scrolling under it, and this follows that pointer like a mouse.
 */
import type { Payload } from './answer-types.js'

/** Sends one event to the main window, as the swizzle and wry do for its webview. */
type EmitToMain = (event: string, payload: unknown) => void

interface Modifiers {
  altHeld: boolean
  cmdHeld: boolean
  shiftHeld: boolean
}

interface Position {
  x: number
  y: number
}

export interface DragSessions {
  /** The answer for a drag command, or `undefined` for one this doesn't own. */
  handler(cmd: string): ((payload: Payload) => unknown) | undefined
  /** Ends any session and removes every listener (tests, and a page going away). */
  dispose(): void
}

/** What an OS drag loop keeps from the page while it runs: the page sees no mouse until the drop. */
const SWALLOWED_MOUSE = ['mousedown', 'mousemove', 'mouseup', 'click', 'dblclick', 'contextmenu'] as const

function modifiersOf(event: MouseEvent | KeyboardEvent): Modifiers {
  return { altHeld: event.altKey, cmdHeld: event.metaKey, shiftHeld: event.shiftKey }
}

function sameModifiers(a: Modifiers | null, b: Modifiers): boolean {
  return a !== null && a.altHeld === b.altHeld && a.cmdHeld === b.cmdHeld && a.shiftHeld === b.shiftHeld
}

export function createDragSessions(win: Window, emit: EmitToMain): DragSessions {
  let endSession: (() => void) | null = null

  function start(paths: string[]): void {
    endSession?.()
    // Whether the pointer is over the page (the main window), and what it last told the app.
    let inside = false
    let sentModifiers: Modifiers | null = null

    const isInPage = ({ x, y }: Position) => x >= 0 && y >= 0 && x < win.innerWidth && y < win.innerHeight

    function sendModifiers(modifiers: Modifiers): void {
      if (sameModifiers(sentModifiers, modifiers)) return
      sentModifiers = modifiers
      emit('drag-modifiers', modifiers)
    }

    function enter(position: Position, modifiers: Modifiers): void {
      inside = true
      // The swizzle sends the keys as the drag enters, before wry's enter, so the first verdict has them.
      sendModifiers(modifiers)
      emit('tauri://drag-enter', { paths, position })
    }

    function leave(): void {
      if (!inside) return
      inside = false
      // The app stops tracking keys on leave and forgets them, so a re-entry sends them again.
      sentModifiers = null
      emit('tauri://drag-leave', null)
    }

    function onPointerMove(event: PointerEvent): void {
      if (!event.isPrimary) return
      event.stopPropagation()
      // The button came up before the session began (a quick flick): nothing is being dragged.
      if (event.buttons === 0) {
        finish()
        return
      }
      const position = { x: event.clientX, y: event.clientY }
      if (!isInPage(position)) {
        leave()
        return
      }
      if (!inside) enter(position, modifiersOf(event))
      else sendModifiers(modifiersOf(event))
      emit('tauri://drag-over', { position })
    }

    function onPointerUp(event: PointerEvent): void {
      if (!event.isPrimary) return
      event.stopPropagation()
      const position = { x: event.clientX, y: event.clientY }
      // Let go outside the page: nothing out there takes the files, so it's a clean no-op.
      if (isInPage(position)) {
        if (!inside) enter(position, modifiersOf(event))
        emit('tauri://drag-drop', { paths, position })
        inside = false
      }
      finish()
    }

    function onPointerCancel(event: PointerEvent): void {
      if (!event.isPrimary) return
      finish()
    }

    function onKey(event: KeyboardEvent): void {
      if (event.type === 'keydown' && event.key === 'Escape') {
        // Escape cancels an OS drag, and the app never sees it.
        event.preventDefault()
        event.stopPropagation()
        finish()
        return
      }
      if (inside) sendModifiers(modifiersOf(event))
    }

    function swallow(event: Event): void {
      event.stopPropagation()
      if (event.type === 'contextmenu') event.preventDefault()
    }

    const listeners: [string, (event: never) => void][] = [
      ['pointermove', onPointerMove],
      ['pointerup', onPointerUp],
      ['pointercancel', onPointerCancel],
      ['keydown', onKey],
      ['keyup', onKey],
      ['blur', finish],
      ...SWALLOWED_MOUSE.map((type): [string, (event: never) => void] => [type, swallow]),
    ]
    for (const [type, listener] of listeners) win.addEventListener(type, listener as EventListener, true)

    function finish(): void {
      if (endSession !== finish) return
      endSession = null
      leave()
      for (const [type, listener] of listeners) {
        if (SWALLOWED_MOUSE.includes(type as (typeof SWALLOWED_MOUSE)[number])) continue
        win.removeEventListener(type, listener as EventListener, true)
      }
      // The release's own `mouseup` and `click` follow its `pointerup` in the same task; the OS
      // would have eaten them too. Hand the mouse back after that.
      win.setTimeout(() => {
        for (const type of SWALLOWED_MOUSE) win.removeEventListener(type, swallow, true)
      }, 0)
    }
    endSession = finish
  }

  return {
    handler(cmd) {
      if (cmd !== 'start_drag_paths') return undefined
      return (payload) => {
        start(payload.paths as string[])
        return null
      }
    },
    dispose() {
      endSession?.()
    },
  }
}
