/**
 * The demo's one backend per page: every answer, the stores, the WASM listing engine, and the event
 * fan-out to every attached webview. Lives in the top frame; a secondary window (an iframe of its
 * route) reaches it through `window.parent.__cmdrDemoBackend`, the way every Tauri webview talks to
 * the one Rust process. See `DETAILS.md` § Backend and webviews.
 */
import type { AnswerTable, Payload } from './answer-types.js'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'
import { startListingEngine, type ListingEngineHandle } from './listing.js'
import { pluginHandler } from './plugins.js'
import { noteUnanswered } from './unanswered.js'
import { volumeAnswers } from './volumes.js'
import { writeAnswers } from './writes.js'

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
}

function camelCase(snake: string): string {
  return snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())
}

export function createDemoBackend(): DemoBackend {
  const webviews = new Set<DemoWebview>()

  function emit(event: string, payload: unknown, target: EventTarget): void {
    for (const webview of webviews) webview.deliver(event, payload, target)
  }

  const silent: AnswerTable = Object.fromEntries(silentCommands.map((name) => [name, () => undefined]))
  const answers = { ...silent, ...appAnswers, ...volumeAnswers, ...writeAnswers } as Record<string, Handler | undefined>
  const engine: ListingEngineHandle = startListingEngine((event, payload) => {
    emit(event, payload, { kind: 'everyone' })
  })

  async function invoke(cmd: string, payload: Payload): Promise<unknown> {
    const handler = pluginHandler(cmd) ?? rawAnswers[cmd] ?? answers[camelCase(cmd)]
    if (handler) return handler(payload)
    if (await engine.handles(cmd)) return engine.invoke(cmd, payload)
    return noteUnanswered(cmd, payload)
  }

  return {
    invoke,
    emit,
    attach(webview) {
      webviews.add(webview)
      return () => webviews.delete(webview)
    },
  }
}
