/**
 * The demo's one backend per page: every answer, the stores, the WASM listing engine, the menu
 * bar's state (`menu-engine.ts`), and the event fan-out to every attached webview. Lives in the top
 * frame; a secondary window (an iframe of its route) reaches it through `window.parent.__cmdrDemoBackend`, the way every Tauri webview talks to
 * the one Rust process. See `DETAILS.md` § Backend and webviews.
 */
import type { AnswerTable, Payload } from './answer-types.js'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'
import { startListingEngine, type ListingEngineHandle } from './listing.js'
import { startMenuEngine, type MenuEngineHandle } from './menu-engine.js'
import { pluginHandler } from './plugins.js'
import { noteUnanswered } from './unanswered.js'
import { demoEntries } from './demo-tree.js'
import { demoPath, volumeAnswers } from './volumes.js'
import { createWindowHost, type WindowHost } from './windows.js'

type Handler = (payload: Payload) => unknown

/** Where an event goes: everyone (`emit`), or the webviews of one label (`emit_to`). */
export type EventTarget = { kind: 'everyone' } | { kind: 'label'; label: string }

/** One webview attached to the backend: its label, and how to hand it an event. */
export interface DemoWebview {
  label: string
  deliver(event: string, payload: unknown, target: EventTarget): void
}

export interface DemoBackend {
  /** Answers one IPC call (snake_case wire name, JSON-shaped payload), as the Rust process would. */
  invoke(cmd: string, payload: Payload): Promise<unknown>
  /** Sends an event to the attached webviews, as Tauri's `emit` / `emit_to` do. */
  emit(event: string, payload: unknown, target: EventTarget): void
  /** Attaches a webview to the event fan-out. Returns the detach. */
  attach(webview: DemoWebview): () => void
  /** The menu bar's state, which the top frame's bar draws (`menu-bar.ts`). */
  readonly menu: MenuEngineHandle
  /** The secondary windows, which a menu click can close or hand an edit to. */
  readonly windows: WindowHost
  /** A `blob:` URL over a viewer media token's bytes, for the window `label`; `''` for a token that names nothing. */
  mediaUrl(token: string, label: string): string
}

/** The engine's own notices, which no Tauri listener knows (`crates/cmdr-listing-wasm/src/viewer.rs`). */
const DEMO_EVENT_PREFIX = 'demo://'

/** Every file in the demo tree: what Enter opens in the viewer. */
const demoFiles = new Set(demoEntries.filter((entry) => entry.kind === 'file').map((entry) => entry.path))

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
      window.open(url, '_blank', 'noopener')
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
  const menu = startMenuEngine()

  // The demo's "default app" for a file is Cmdr's own viewer: Enter opens it, through the event the
  // main window already answers (`open-file-viewer`). A folder or package keeps `openPath`'s refusal.
  function openInViewer(payload: Payload): boolean {
    const path = demoPath(String(payload.path))
    if (!demoFiles.has(path)) return false
    emit('open-file-viewer', { path }, { kind: 'label', label: 'main' })
    return true
  }

  async function invoke(cmd: string, payload: Payload): Promise<unknown> {
    if (cmd === 'open_path' && openInViewer(payload)) return null
    const handler = windows.handler(cmd) ?? pluginHandler(cmd) ?? rawAnswers[cmd] ?? answers[camelCase(cmd)]
    if (handler) return handler(payload)
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
      return () => webviews.delete(webview)
    },
  }
}
