/**
 * The demo's secondary windows: `new WebviewWindow('settings', …)` opens an in-page floating panel
 * holding a same-origin iframe of the route, named after the window's label, so its boot attaches
 * to the page's one backend (`boot.ts`). Lives with the backend, in the top frame. Only the windows
 * that run on the demo's answers open; the rest are refused the way Tauri refuses a creation
 * (`tauri://error` on the opener's handle). See `DETAILS.md` § Secondary windows.
 */
import { resolve } from '$app/paths'
import { refuseWith, type Payload } from './answer-types.js'
import type { EventTarget } from './backend.js'
import {
  createMainWindowLights,
  createTrafficLights,
  lightsInset,
  MAIN_LIGHTS_INSET,
  type TrafficLightPosition,
  type TrafficLights,
} from './traffic-lights.js'

/**
 * The windows that open. Settings and Keyboard shortcuts run on the stores and answers the demo
 * has, the transfer queue on the write engine, and the file viewer on the WASM engine's viewer (its
 * labels are `viewer-<timestamp>`, several at once).
 */
const OPENABLE = new Set(['settings', 'shortcuts', 'queue'])
const OPENABLE_PREFIXES = ['viewer-']

export function isOpenable(label: string): boolean {
  return OPENABLE.has(label) || OPENABLE_PREFIXES.some((prefix) => label.startsWith(prefix))
}

/** Room the panel leaves around itself inside the viewport, in CSS pixels. */
const MARGIN = 16

/** How much of a panel's top stays on screen when dragged down: a title bar's height, enough to grab it again. */
const TITLE_STRIP = 28

type Emit = (event: string, payload: unknown, target: EventTarget) => void
type Handler = (payload: Payload) => unknown

interface Panel {
  label: string
  root: HTMLElement
  iframe: HTMLIFrameElement
  lights: TrafficLights
  /** The panel's own size and place before a zoom filled the viewport, for the next zoom to restore. */
  unzoomed: { left: string; top: string; width: string; height: string } | null
  maxWidth: number
}

/** The route URL under the demo's base path, query string kept. */
function routeUrl(url: string): string {
  const query = url.indexOf('?')
  const pathname = query < 0 ? url : url.slice(0, query)
  return (resolve as (path: string) => string)(pathname) + (query < 0 ? '' : url.slice(query))
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max))
}

/** Where windows may start: under the demo's menu bar (`--cmdr-demo-window-top`), as macOS keeps them under its own. */
function desktopTop(): number {
  return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cmdr-demo-window-top')) || 0
}

/** The panel's look, from the app's own tokens. Injected once, scoped to the demo's class names. */
const STYLE = `
.cmdr-demo-window {
  position: fixed;
  z-index: var(--z-modal);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--color-bg-primary);
  border-radius: var(--radius-window);
  box-shadow: var(--shadow-lg);
}
/* The hairline sits over the page, as a macOS window's does, so the page and the lights measure
   from the window's outer edge as on desktop (a layout border would push both 1px in). */
.cmdr-demo-window::after {
  content: '';
  position: absolute;
  inset: 0;
  z-index: 2;
  border: 1px solid var(--color-border);
  border-radius: inherit;
  pointer-events: none;
}
/* Over the page's overlay title bar, as the OS draws them over a Tauri overlay window. */
.cmdr-demo-window .cmdr-demo-lights { position: absolute; z-index: 1; }
.cmdr-demo-window iframe { flex: 1; width: 100%; border: 0; background: transparent; }
.cmdr-demo-window.dragging iframe { pointer-events: none; }
`

export interface WindowHost {
  /** The backend's answer for a window command, or `undefined` for one this host doesn't own. */
  handler(cmd: string): Handler | undefined
  /** The panel in front, while one is open: what the menu bar's Close tab closes outside the main window. */
  front(): { label: string; frame: HTMLIFrameElement } | undefined
  /** Closes the panel labeled `label`, as its close button does. */
  close(label: string): void
  /** Draws the main window's traffic lights at its top-left (`traffic-lights.ts`). Call once, in the top frame. */
  mountMainWindowLights(): void
}

/** `onClosed` hears every panel that closes, after its `tauri://destroyed`, as the app's window-destroyed hook does. */
export function createWindowHost(emit: Emit, onClosed: (label: string) => void = () => {}): WindowHost {
  const panels = new Map<string, Panel>()
  let styled = false
  // What had focus in the main window when a panel took it. A native window keeps its focused
  // element while another window is key; the page has one focus, so the main window's is kept here.
  let mainFocus: HTMLElement | null = null
  // The key window's label, whose lights are lit. Every window's go grey when the page loses
  // focus, as all of an app's windows do when another app comes forward.
  let key = 'main'
  let mainLights: TrafficLights | null = null

  function refreshLights(): void {
    const pageFocused = document.hasFocus()
    mainLights?.setActive(pageFocused && key === 'main')
    for (const panel of panels.values()) panel.lights.setActive(pageFocused && key === panel.label)
  }

  function setKey(label: string): void {
    key = label
    refreshLights()
  }

  function mountMainWindowLights(): void {
    if (mainLights) return
    const lights = createMainWindowLights()
    mainLights = lights
    // The app's start script runs in `<body>`, so it exists by the time the backend is created.
    document.body.append(lights.root)
    refreshLights()
    // Focus moving into a panel's iframe blurs this window while the page keeps focus, so the
    // answer is read a task later, once `document.hasFocus()` has settled.
    const later = () => setTimeout(refreshLights, 0)
    window.addEventListener('focus', later)
    window.addEventListener('blur', later)
  }

  /** The green light on a resizable window: fill the desktop under the menu bar (up to the window's max width), then back. */
  function zoom(panel: Panel): void {
    const style = panel.root.style
    if (panel.unzoomed) {
      Object.assign(style, panel.unzoomed)
      panel.unzoomed = null
      panel.lights.setZoomGlyph('enterFullScreen')
      return
    }
    panel.unzoomed = { left: style.left, top: style.top, width: style.width, height: style.height }
    panel.lights.setZoomGlyph('exitFullScreen')
    const width = Math.min(window.innerWidth - 2 * MARGIN, panel.maxWidth)
    style.width = `${String(width)}px`
    const top = desktopTop()
    style.height = `${String(window.innerHeight - top - 2 * MARGIN)}px`
    style.left = `${String(Math.round((window.innerWidth - width) / 2))}px`
    style.top = `${String(top + MARGIN)}px`
  }

  // The panel raised last, while it's open.
  let front: Panel | undefined

  function focusPanel(panel: Panel): void {
    front = panel
    const active = document.activeElement
    if (active instanceof HTMLElement && !active.closest('.cmdr-demo-window')) mainFocus = active
    // On top of the other panels, as the raised window is.
    for (const other of panels.values()) other.root.style.zIndex = 'var(--z-modal)'
    panel.root.style.zIndex = 'var(--z-modal-top)'
    panel.iframe.focus()
    setKey(panel.label)
    emit('tauri://blur', null, { kind: 'label', label: 'main' })
    emit('tauri://focus', null, { kind: 'label', label: panel.label })
  }

  function close(label: string): void {
    const panel = panels.get(label)
    if (!panel) return
    panels.delete(label)
    if (front === panel) front = undefined
    emit('tauri://destroyed', null, { kind: 'label', label })
    onClosed(label)
    panel.root.remove()
    // The window under it gets focus back, as macOS hands it to the next window.
    const next = [...panels.values()].pop()
    if (next) {
      focusPanel(next)
      return
    }
    window.focus()
    if (mainFocus?.isConnected) mainFocus.focus({ preventScroll: true })
    mainFocus = null
    setKey('main')
    emit('tauri://focus', null, { kind: 'label', label: 'main' })
  }

  /**
   * `start_dragging` from the page's drag region (`drag-regions.ts`): the panel follows the mouse
   * until it's released, as the OS moves a window. Screen coordinates, since the press was in the
   * panel's frame and the moves may land in either document (the frame stops taking the pointer
   * once `dragging` is on, but a browser can keep routing a held button's moves to it).
   */
  function startDrag(panel: Panel, screenX: number, screenY: number): void {
    const start = panel.root.getBoundingClientRect()
    const windows = [window, panel.iframe.contentWindow].filter((w): w is Window => w !== null)
    panel.root.classList.add('dragging')
    const move = (event: MouseEvent) => {
      const { width } = panel.root.getBoundingClientRect()
      // The window's top strip stays reachable: never dragged past an edge.
      const left = clamp(start.left + event.screenX - screenX, 0, window.innerWidth - width)
      const top = clamp(start.top + event.screenY - screenY, desktopTop(), window.innerHeight - TITLE_STRIP)
      panel.root.style.left = `${String(left)}px`
      panel.root.style.top = `${String(top)}px`
    }
    const up = () => {
      for (const w of windows) {
        w.removeEventListener('mousemove', move)
        w.removeEventListener('mouseup', up)
      }
      panel.root.classList.remove('dragging')
    }
    for (const w of windows) {
      w.addEventListener('mousemove', move)
      w.addEventListener('mouseup', up)
    }
  }

  function open(options: Payload): void {
    const label = options.label as string
    if (!styled) {
      const style = document.createElement('style')
      style.textContent = STYLE
      document.head.append(style)
      styled = true
    }
    const title = typeof options.title === 'string' ? options.title : ''
    const width = Math.min(Number(options.width) || 800, window.innerWidth - 2 * MARGIN)
    const top = desktopTop()
    const height = Math.min(Number(options.height) || 600, window.innerHeight - top - 2 * MARGIN)

    const root = document.createElement('div')
    root.className = 'cmdr-demo-window'
    root.setAttribute('role', 'dialog')
    root.setAttribute('aria-label', title)
    root.style.width = `${String(width)}px`
    root.style.height = `${String(height)}px`
    root.style.left = `${String(Math.round((window.innerWidth - width) / 2))}px`
    root.style.top = `${String(top + Math.round((window.innerHeight - top - height) / 2))}px`

    // Close closes; zoom fills the viewport and back, the nearest a panel gets to full screen;
    // minimize stays inert, since a page has no Dock to bring a minimized panel back from. The
    // glyph is the full-screen arrows a resizable window's green light shows on macOS 27.
    const lights = createTrafficLights(
      {
        close: () => {
          close(label)
        },
        zoom: () => {
          zoom(panel)
        },
      },
      'enterFullScreen',
    )
    // Every Cmdr window is an overlay window with a hidden title: no title bar of its own, the
    // lights over the page at its `trafficLightPosition`, and the page's drag strip under them.
    const inset = options.trafficLightPosition
      ? lightsInset(options.trafficLightPosition as TrafficLightPosition)
      : MAIN_LIGHTS_INSET
    lights.root.style.left = `${String(inset.left)}px`
    lights.root.style.top = `${String(inset.top)}px`

    const iframe = document.createElement('iframe')
    iframe.name = label
    iframe.title = title
    iframe.src = routeUrl(typeof options.url === 'string' ? options.url : '/')

    root.append(lights.root, iframe)
    const maxWidth = Number(options.maxWidth) || Infinity
    const panel: Panel = { label, root, iframe, lights, unzoomed: null, maxWidth }
    root.addEventListener('pointerdown', () => {
      focusPanel(panel)
    })
    // Esc is the window's own (it calls `close_child_window`). ⌘W is the native menu's Close
    // Window, which the browser keeps for its tab, so the panel takes ⌃W, the demo's
    // browser-safe stand-in for ⌘W (`browser-shortcuts.ts`).
    iframe.addEventListener('load', () => {
      iframe.contentWindow?.addEventListener('keydown', (event) => {
        const onlyControl = event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey
        if (onlyControl && event.key.toLowerCase() === 'w') {
          event.preventDefault()
          close(label)
        }
      })
      focusPanel(panel)
    })
    document.body.append(root)
    panels.set(label, panel)
  }

  const handlers: Partial<Record<string, Handler>> = {
    'plugin:webview|create_webview_window': (p) => {
      const options = p.options as Payload
      const label = options.label as string
      // Tauri's refusal is a string, and it reaches the opener as `tauri://error`.
      if (!isOpenable(label)) refuseWith(`The ${label} window isn't in the demo`)
      if (panels.has(label)) refuseWith(`A window labeled ${label} already exists`)
      open(options)
      return null
    },
    'plugin:window|get_all_windows': () => ['main', ...panels.keys()],
    'plugin:webview|get_all_webviews': () => ['main', ...panels.keys()].map((label) => ({ label, windowLabel: label })),
    'plugin:window|available_monitors': () => [],
    // The panel's look is the page's (`STYLE`); no native material to apply.
    'plugin:window|set_effects': () => null,
    'plugin:window|close': (p) => {
      close(p.label as string)
      return null
    },
    'plugin:window|destroy': (p) => {
      close(p.label as string)
      return null
    },
    'plugin:window|set_focus': (p) => {
      const panel = panels.get(p.label as string)
      if (panel) focusPanel(panel)
      return null
    },
    // A press and a double click on the page's drag region (`drag-regions.ts`). The main window is
    // the page itself, so its own drag strip moves nothing.
    'plugin:window|start_dragging': (p) => {
      const panel = panels.get(p.label as string)
      if (panel) startDrag(panel, Number(p.screenX), Number(p.screenY))
      return null
    },
    'plugin:window|internal_toggle_maximize': (p) => {
      const panel = panels.get(p.label as string)
      if (panel) zoom(panel)
      return null
    },
    // Cmdr's own child-window commands: a window closing itself (Esc in Settings) and being raised.
    close_child_window: (p) => {
      close(p.label as string)
      return null
    },
    raise_child_window: (p) => {
      const panel = panels.get(p.label as string)
      if (panel) focusPanel(panel)
      return null
    },
  }

  return {
    handler: (cmd) => handlers[cmd],
    front: () => (front ? { label: front.label, frame: front.iframe } : undefined),
    close,
    mountMainWindowLights,
  }
}
