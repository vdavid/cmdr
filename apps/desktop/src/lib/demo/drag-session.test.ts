import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createDragSessions, type DragSessions } from './drag-session'

const PATHS = ['/Users/you/Documents/notes.md']

interface Sent {
  event: string
  payload: unknown
}

function pointer(type: string, init: PointerEventInit = {}): PointerEvent {
  return new PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    pointerId: 1,
    pointerType: 'mouse',
    isPrimary: true,
    buttons: 1,
    ...init,
  })
}

describe('createDragSessions', () => {
  let sent: Sent[]
  let sessions: DragSessions

  beforeEach(() => {
    sent = []
    sessions = createDragSessions(window, (event, payload) => sent.push({ event, payload }))
  })
  afterEach(() => {
    sessions.dispose()
  })

  function start(paths = PATHS): void {
    const handler = sessions.handler('start_drag_paths')
    if (!handler) throw new Error('start_drag_paths has no answer')
    expect(handler({ paths, iconPath: '/tmp/drag-image.png', sourceVolumeId: 'root' })).toBeNull()
  }

  const names = () => sent.map((s) => s.event)

  it('answers only the drag start', () => {
    expect(sessions.handler('start_drag_paths')).toBeDefined()
    expect(sessions.handler('list_directory_start')).toBeUndefined()
  })

  it('sends enter, over, and drop as the pointer moves and lets go, as the OS does', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 30, clientY: 40 }))
    window.dispatchEvent(pointer('pointermove', { clientX: 50, clientY: 60 }))
    window.dispatchEvent(pointer('pointerup', { clientX: 55, clientY: 65, buttons: 0 }))

    expect(names()).toEqual([
      'drag-modifiers',
      'tauri://drag-enter',
      'tauri://drag-over',
      'tauri://drag-over',
      'tauri://drag-drop',
    ])
    expect(sent[1].payload).toEqual({ paths: PATHS, position: { x: 30, y: 40 } })
    expect(sent[3].payload).toEqual({ position: { x: 50, y: 60 } })
    expect(sent[4].payload).toEqual({ paths: PATHS, position: { x: 55, y: 65 } })
  })

  it('drops where it lets go even when it never moved', () => {
    start()
    window.dispatchEvent(pointer('pointerup', { clientX: 5, clientY: 6, buttons: 0 }))
    expect(names()).toEqual(['drag-modifiers', 'tauri://drag-enter', 'tauri://drag-drop'])
  })

  it('sends the modifier keys on enter and when they change', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10, altKey: true }))
    window.dispatchEvent(pointer('pointermove', { clientX: 11, clientY: 10, altKey: true }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Meta', altKey: true, metaKey: true }))

    const modifiers = sent.filter((s) => s.event === 'drag-modifiers').map((s) => s.payload)
    expect(modifiers).toEqual([
      { altHeld: true, cmdHeld: false, shiftHeld: false },
      { altHeld: true, cmdHeld: true, shiftHeld: false },
    ])
  })

  it('leaves when the pointer goes outside the page and enters again when it comes back', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10 }))
    window.dispatchEvent(pointer('pointermove', { clientX: -5, clientY: 10 }))
    window.dispatchEvent(pointer('pointermove', { clientX: 12, clientY: 10 }))
    expect(names()).toEqual([
      'drag-modifiers',
      'tauri://drag-enter',
      'tauri://drag-over',
      'tauri://drag-leave',
      'drag-modifiers',
      'tauri://drag-enter',
      'tauri://drag-over',
    ])
  })

  it('lets go outside the page as a clean no-op: no drop, since nothing out there takes files', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10 }))
    window.dispatchEvent(pointer('pointermove', { clientX: -5, clientY: 10 }))
    window.dispatchEvent(pointer('pointerup', { clientX: -5, clientY: 10, buttons: 0 }))
    expect(names()).not.toContain('tauri://drag-drop')
    expect(names().at(-1)).toBe('tauri://drag-leave')
  })

  it('cancels on Escape, keeping the key from the app', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10 }))
    const escape = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true, bubbles: true })
    let reachedApp = false
    document.addEventListener('keydown', () => (reachedApp = true), { once: true })
    document.dispatchEvent(escape)

    expect(names().at(-1)).toBe('tauri://drag-leave')
    expect(reachedApp).toBe(false)
    window.dispatchEvent(pointer('pointerup', { clientX: 10, clientY: 10, buttons: 0 }))
    expect(names()).not.toContain('tauri://drag-drop')
  })

  it('ends when the browser takes the pointer', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10 }))
    window.dispatchEvent(pointer('pointercancel'))
    expect(names().at(-1)).toBe('tauri://drag-leave')
  })

  it('ends with no drop when the button was already up', () => {
    start()
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10, buttons: 0 }))
    expect(names()).not.toContain('tauri://drag-drop')
    expect(names()).not.toContain('tauri://drag-enter')
  })

  it('keeps the mouse from the app while dragging, as an OS drag loop does', () => {
    start()
    const seen: string[] = []
    for (const type of ['mousemove', 'mouseup', 'click', 'contextmenu']) {
      document.addEventListener(type, () => seen.push(type), { once: true })
    }
    document.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }))
    window.dispatchEvent(pointer('pointerup', { clientX: 10, clientY: 10, buttons: 0 }))
    document.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }))
    // The click a release brings arrives in the same task as the release.
    document.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(seen).toEqual([])
  })

  it('gives the mouse back once the drag is over', async () => {
    start()
    window.dispatchEvent(pointer('pointerup', { clientX: 10, clientY: 10, buttons: 0 }))
    await new Promise((resolve) => setTimeout(resolve, 0))
    let clicked = false
    document.addEventListener('click', () => (clicked = true), { once: true })
    document.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(clicked).toBe(true)
  })

  it('a second start replaces the first', () => {
    start(['/a'])
    window.dispatchEvent(pointer('pointermove', { clientX: 10, clientY: 10 }))
    start(['/b'])
    window.dispatchEvent(pointer('pointermove', { clientX: 11, clientY: 10 }))
    window.dispatchEvent(pointer('pointerup', { clientX: 11, clientY: 10, buttons: 0 }))
    const drops = sent.filter((s) => s.event === 'tauri://drag-drop')
    expect(drops).toEqual([{ event: 'tauri://drag-drop', payload: { paths: ['/b'], position: { x: 11, y: 10 } } }])
  })
})

describe('files dropped in from the OS', () => {
  it('does nothing, and keeps the browser from opening the file in place of the demo', () => {
    const sessions = createDragSessions(window, () => undefined)
    try {
      for (const type of ['dragover', 'drop']) {
        const event = new Event(type, { cancelable: true, bubbles: true })
        document.dispatchEvent(event)
        expect(event.defaultPrevented).toBe(true)
      }
    } finally {
      sessions.dispose()
    }
  })
})
