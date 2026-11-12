/**
 * One demo webview: this frame's `window.__TAURI_INTERNALS__`, forwarding every `invoke` to the
 * page's one backend and keeping this frame's event listeners, as a Tauri webview does with the
 * Rust process. The main window installs one over its own backend; a secondary window's iframe
 * installs one over its parent's. See `DETAILS.md` § Backend and webviews.
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { Payload } from './answer-types.js'
import type { DemoBackend, EventTarget } from './backend.js'
import { installColorScheme } from './color-scheme.js'
import { installDragRegions } from './drag-regions.js'
import { isBrowserFullScreen, setBrowserFullScreen } from './browser-full-screen.js'
import { installPlatform } from './platform.js'

/** A listener's target, as `@tauri-apps/api/event`'s `listen` sends it. */
type ListenTarget = { kind: 'Any' } | { kind: string; label: string }

interface Listener {
  event: string
  target: ListenTarget
}

interface TauriInternals {
  runCallback(id: number, data: unknown): void
}

/** The wire between a webview and the backend is JSON, as Tauri's is: nothing shared, nothing mutable across it. */
function overTheWire<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T)
}

/** Commands about this webview's own window: the frame's viewport stands in for it. */
function windowAnswer(cmd: string, payload: Payload): (() => unknown) | undefined {
  const physical = () => ({
    width: Math.round(window.innerWidth * window.devicePixelRatio),
    height: Math.round(window.innerHeight * window.devicePixelRatio),
  })
  const answers: Partial<Record<string, () => unknown>> = {
    'plugin:window|outer_size': physical,
    'plugin:window|inner_size': physical,
    'plugin:window|scale_factor': () => window.devicePixelRatio,
    // The browser's full screen stands in for the window's, so Toggle full screen and Escape's
    // exit drive it (where the page may; a refusal answers like a done call).
    'plugin:window|is_fullscreen': isBrowserFullScreen,
    'plugin:window|set_fullscreen': async () => {
      await setBrowserFullScreen(payload.value === true)
      return null
    },
    'plugin:window|is_maximized': () => false,
    'plugin:window|is_focused': () => document.hasFocus(),
    'plugin:window|is_visible': () => true,
    'plugin:window|title': () => document.title,
    // A frame sits at its own origin; the panel around a secondary window is the page's business.
    'plugin:window|outer_position': () => ({ x: 0, y: 0 }),
    'plugin:window|inner_position': () => ({ x: 0, y: 0 }),
    // A window's frame is the page's to size and name, not the app's.
    'plugin:window|set_title': () => null,
    'plugin:window|set_min_size': () => null,
    'plugin:window|set_max_size': () => null,
  }
  return answers[cmd]
}

/** Whether an event sent to `target` reaches a listener on `listenTarget`, as Tauri routes it. */
function reaches(target: EventTarget, listenTarget: ListenTarget): boolean {
  if (target.kind === 'everyone' || listenTarget.kind === 'Any') return true
  return 'label' in listenTarget && listenTarget.label === target.label
}

/** Installs the webview for the window labeled `label`, attached to `backend`. */
// Only the IPC and event halves: a webview has no business with the menu bar or the panels.
export function installWebview(
  backend: Pick<DemoBackend, 'invoke' | 'emit' | 'attach' | 'mediaUrl'>,
  label: string,
): void {
  // Keyed by the handler's callback id, which doubles as the event id `listen` resolves with,
  // as the mocks' own `unregisterListener` assumes.
  const listeners = new Map<number, Listener>()
  // Where this frame last saw the pointer go down or ask for a context menu: where a native popup
  // opens when the app names no point (`backend.ts`'s `openPopup`).
  let lastPointer: { x: number; y: number } | null = null
  function notePointer(event: MouseEvent): void {
    lastPointer = { x: event.clientX, y: event.clientY }
  }
  window.addEventListener('pointerdown', notePointer, true)
  window.addEventListener('contextmenu', notePointer, true)
  const internals = () => (window as unknown as { __TAURI_INTERNALS__: TauriInternals }).__TAURI_INTERNALS__

  function deliver(event: string, payload: unknown, target: EventTarget): void {
    for (const [handler, listener] of listeners) {
      if (listener.event !== event || !reaches(target, listener.target)) continue
      internals().runCallback(handler, { event, id: handler, payload: overTheWire(payload) })
    }
  }

  async function invoke(cmd: string, payload: Payload): Promise<unknown> {
    switch (cmd) {
      case 'plugin:event|listen': {
        const handler = payload.handler as number
        listeners.set(handler, { event: payload.event as string, target: payload.target as ListenTarget })
        return handler
      }
      case 'plugin:event|unlisten':
        listeners.delete(payload.eventId as number)
        return null
      case 'plugin:event|emit':
        backend.emit(payload.event as string, payload.payload, { kind: 'everyone' })
        return null
      case 'plugin:event|emit_to': {
        const target = payload.target as ListenTarget
        const to: EventTarget = 'label' in target ? { kind: 'label', label: target.label } : { kind: 'everyone' }
        backend.emit(payload.event as string, payload.payload, to)
        return null
      }
    }
    // A question about ANOTHER window (main asking for the Settings window's size) goes to the
    // backend, which knows the panels; only this frame can answer for itself.
    const own = payload.label === undefined || payload.label === label ? windowAnswer(cmd, payload) : undefined
    if (own) return own()
    return overTheWire(await backend.invoke(cmd, overTheWire(payload), { label, pointer: lastPointer }))
  }

  mockWindows(label)
  mockIPC((cmd, payload) => invoke(cmd, (payload ?? {}) as Payload))
  // `convertFileSrc` as Tauri's macOS webview answers it, except the viewer's `cmdr-media`
  // scheme, which the page can't register: a `blob:` URL over the token's bytes instead.
  const tauri = (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__
  tauri.convertFileSrc = (filePath: string, protocol = 'asset') =>
    protocol === 'cmdr-media'
      ? backend.mediaUrl(filePath, label)
      : `${protocol}://localhost/${encodeURIComponent(filePath)}`
  // Light, Dark, or System, as the app's `setTheme` picks it for every window (`color-scheme.ts`).
  const scheme = installColorScheme(window)
  const detach = backend.attach({
    label,
    deliver,
    setTheme: (theme) => {
      scheme.set(theme)
    },
  })
  // Tauri's injected platform script, for this window (`platform.ts`).
  installPlatform(window)
  // Tauri's injected drag script, for this window: the backend moves the panel (`windows.ts`).
  installDragRegions((cmd, payload) => invoke(cmd, { ...payload, label }))
  // A secondary window's iframe going away is that window closing: stop sending it events.
  window.addEventListener('pagehide', detach)
}
