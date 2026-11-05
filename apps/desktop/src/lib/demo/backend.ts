/**
 * The demo's one backend per page: every answer, the stores, the WASM listing engine, the menu
 * bar's state (`menu-engine.ts`), and the event fan-out to every attached webview. Lives in the top
 * frame; a secondary window (an iframe of its route) reaches it through `window.parent.__cmdrDemoBackend`, the way every Tauri webview talks to
 * the one Rust process. See `DETAILS.md` § Backend and webviews.
 */
import type { AnswerTable, Payload } from './answer-types.js'
import type { AppTheme } from './color-scheme.js'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'
import { startListingEngine, type ListingEngineHandle } from './listing.js'
import { createContextMenuHost, type ContextMenuHost } from './context-menu.js'
import { startMenuEngine, type ContextPopup, type MenuEngineHandle } from './menu-engine.js'
import { pluginHandler } from './plugins.js'
import { openInBrowserTab } from './opener.js'
import { noteUnanswered } from './unanswered.js'
import type { TimedOut } from '#lib/ipc/bindings.js'
import { demoPath, volumeAnswers } from './volumes.js'
import { createDragSessions } from './drag-session.js'
import { createWindowHost, type WindowHost } from './windows.js'

type Handler = (payload: Payload) => unknown

/** Where an event goes: everyone (`emit`), or the webviews of one label (`emit_to`). */
export type EventTarget = { kind: 'everyone' } | { kind: 'label'; label: string }

/**
 * Which webview an invoke came from, and where its pointer last was (its frame's CSS pixels): what
 * a right-click menu needs to open over the window it was asked from, at the pointer, as a native
 * popup does. `null` before the frame saw a pointer.
 */
export interface InvokeOrigin {
  label: string
  pointer: { x: number; y: number } | null
}

/** One webview attached to the backend: its label, and how to hand it an event. */
export interface DemoWebview {
  label: string
  deliver(event: string, payload: unknown, target: EventTarget): void
  /** Pins this window's appearance: `setTheme` is app-wide, as on desktop. */
  setTheme(theme: AppTheme): void
}

export interface DemoBackend {
  /** Answers one IPC call (snake_case wire name, JSON-shaped payload), as the Rust process would. */
  invoke(cmd: string, payload: Payload, origin?: InvokeOrigin): Promise<unknown>
  /** Sends an event to the attached webviews, as Tauri's `emit` / `emit_to` do. */
  emit(event: string, payload: unknown, target: EventTarget): void
  /** Attaches a webview to the event fan-out. Returns the detach. */
  attach(webview: DemoWebview): () => void
  /** The menu bar's state, which the top frame's bar draws (`menu-bar.svelte.ts`). */
  readonly menu: MenuEngineHandle
  /** The secondary windows, which a menu click can close or hand an edit to. */
  readonly windows: WindowHost
  /** A `blob:` URL over a viewer media token's bytes, for the window `label`; `''` for a token that names nothing. */
  mediaUrl(token: string, label: string): string
}

/** The engine's own notices, which no Tauri listener knows (`crates/cmdr-listing-wasm/src/viewer.rs`). */
const DEMO_EVENT_PREFIX = 'demo://'

function camelCase(snake: string): string {
  return snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())
}

export function createDemoBackend(): DemoBackend {
  const webviews = new Set<DemoWebview>()
  // One `blob:` URL per media token, made on the first ask and revoked when the engine drops the token.
  const mediaUrls = new Map<string, string>()

  /** What the page does for an engine notice: open a followed web link, or revoke a dropped token's URL. */
  function onEngineNotice(event: string, payload: unknown): void {
    if (event === 'demo://open-url') {
      const { url } = payload as { url: string }
      // The one opener for every external link, scheme check included.
      try {
        openInBrowserTab(url)
      } catch (refusal) {
        // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
        console.warn('[demo] a viewer link the demo does not open', refusal)
      }
    } else if (event === 'demo://media-token-dropped') {
      const { token } = payload as { token: string }
      const url = mediaUrls.get(token)
      if (url !== undefined) URL.revokeObjectURL(url)
      mediaUrls.delete(token)
    }
  }

  function mediaUrl(token: string, label: string): string {
    const known = mediaUrls.get(token)
    if (known !== undefined) return known
    const body = engine.mediaBody(token, label)
    if (!body) return ''
    // A copy the `Blob` owns: the engine's view of its memory may move.
    const url = URL.createObjectURL(new Blob([body.bytes.slice()], { type: body.mime }))
    mediaUrls.set(token, url)
    return url
  }

  // The app's appearance (`setTheme`), for every window, and for a window that opens later.
  let theme: AppTheme = null
  function setAppTheme(payload: Payload): null {
    theme = payload.theme === 'light' || payload.theme === 'dark' ? payload.theme : null
    for (const webview of webviews) webview.setTheme(theme)
    return null
  }

  function emit(event: string, payload: unknown, target: EventTarget): void {
    for (const webview of webviews) webview.deliver(event, payload, target)
  }

  const silent: AnswerTable = Object.fromEntries(silentCommands.map((name) => [name, () => undefined]))
  const answers = { ...silent, ...appAnswers, ...volumeAnswers } as Record<string, Handler | undefined>
  const engine: ListingEngineHandle = startListingEngine(({ name, payload, target }) => {
    if (name.startsWith(DEMO_EVENT_PREFIX)) {
      onEngineNotice(name, payload)
      return
    }
    emit(name, payload, target === undefined ? { kind: 'everyone' } : { kind: 'label', label: target })
  })

  // A closed viewer panel frees its sessions, as the app's window-destroyed hook does.
  const windows = createWindowHost(emit, (label) => {
    engine.closeViewerWindow(label)
  })
  // The backend lives in the top frame, which is the main window.
  windows.mountMainWindowLights()
  // Drags happen in the main window, the only one with file panes; its events go there, as wry's do.
  const drags = createDragSessions(window, (event, payload) => {
    emit(event, payload, { kind: 'label', label: 'main' })
  })
  const menu = startMenuEngine()

  // The demo's "default app" for a file is Cmdr's own viewer: Enter opens it, through the event the
  // main window already answers (`open-file-viewer`). A folder or package keeps `openPath`'s refusal.
  // What's a file is the live volume's answer (`stat_paths_kinds`), so a file a copy, move, or
  // rename landed opens too, and one a delete removed doesn't.
  async function openInViewer(payload: Payload): Promise<boolean> {
    const path = demoPath(String(payload.path))
    const kinds = (await engine.invoke('stat_paths_kinds', { paths: [path] })) as TimedOut<(boolean | null)[]>
    if (kinds.data[0] !== false) return false
    emit('open-file-viewer', { path }, { kind: 'label', label: 'main' })
    return true
  }

  // The right-click menus, drawn in this (the top) frame the first time one opens.
  let contextMenus: ContextMenuHost | undefined

  /**
   * A `show_*` command: the engine's rows, drawn at the anchor the app named (a keyboard-opened
   * menu) or at the pointer, in the top frame's pixels. Answers `null` once it's up, as the
   * native command returns while its menu tracks.
   */
  async function openPopup(cmd: string, payload: Payload, origin: InvokeOrigin): Promise<null> {
    const popup: ContextPopup = await menu.popup(cmd, payload, origin.label)
    const frame = origin.label === 'main' ? undefined : windows.frame(origin.label)
    const offset = frame?.getBoundingClientRect() ?? { left: 0, top: 0 }
    const point = popup.anchor ?? origin.pointer ?? { x: 0, y: 0 }
    contextMenus ??= createContextMenuHost({ menu, emit, invoke, windows })
    contextMenus.open(popup, { x: point.x + offset.left, y: point.y + offset.top }, popup.anchor !== null)
    return null
  }

  async function invoke(cmd: string, payload: Payload, origin?: InvokeOrigin): Promise<unknown> {
    if (cmd === 'open_path' && (await openInViewer(payload))) return null
    if (cmd === 'plugin:app|set_app_theme') return setAppTheme(payload)
    const handler =
      drags.handler(cmd) ?? windows.handler(cmd) ?? pluginHandler(cmd) ?? rawAnswers[cmd] ?? answers[camelCase(cmd)]
    if (handler) return handler(payload)
    if (await menu.opensPopup(cmd)) return openPopup(cmd, payload, origin ?? { label: 'main', pointer: null })
    if (await menu.handles(cmd)) return menu.invoke(cmd, payload)
    if (await engine.handles(cmd)) return engine.invoke(cmd, payload)
    return noteUnanswered(cmd, payload)
  }

  return {
    invoke,
    emit,
    menu,
    windows,
    mediaUrl,
    attach(webview) {
      webviews.add(webview)
      webview.setTheme(theme)
      return () => webviews.delete(webview)
    },
  }
}
