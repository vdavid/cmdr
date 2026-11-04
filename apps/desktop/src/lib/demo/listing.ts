/**
 * The demo's listing engine: the real `cmdr-listing` built to WASM (`crates/cmdr-listing-wasm`),
 * answering the listing commands in place of the Tauri backend. This file only loads and routes; it
 * holds no sorting, filtering, or fuzzy logic, so the demo lists exactly like the app. See
 * `DETAILS.md` § The listing seam.
 */
import type { Payload } from './answer-types.js'
import { demoContents } from './demo-contents.js'
import { DEMO_ROOT, DEMO_VOLUME_ID, demoEntries } from './demo-tree.js'
import { demoPath } from './volumes.js'

/** The loaded engine, by Tauri command name, as the backend routes to it. */
export interface ListingEngineHandle {
  /** Whether the engine answers `cmd`. Waits for the engine to load, so the first listing never races it. */
  handles(cmd: string): Promise<boolean>
  /** Answers like `invoke`: resolves with the answer, rejects with the typed error value. */
  invoke(cmd: string, payload: Payload): Promise<unknown>
  /** Frees the viewer sessions (and media tokens) of the window `label`, once it closed. */
  closeViewerWindow(label: string): void
}

/** Where the engine's events go: a Tauri event name, its payload, and the window label an `emit_to` names. */
export type ListingEventSink = (event: string, payload: unknown, target?: string) => void

/** The engine's view of a path: `~` expanded (it has no home folder), no trailing slash. */
function expandPaths(payload: Payload): Payload {
  const expanded = { ...payload }
  if (typeof expanded.path === 'string') expanded.path = demoPath(expanded.path)
  if (Array.isArray(expanded.paths)) {
    expanded.paths = expanded.paths.map((p: unknown) => (typeof p === 'string' ? demoPath(p) : p))
  }
  return expanded
}

/**
 * Starts loading the engine and returns its handle at once. The WASM downloads while the app boots;
 * the first listing command waits for it. Events go to `sink` a task late, in order: the in-memory
 * read can finish inside `list_directory_start_streaming`, but the pane expects that command's answer
 * before the listing's first event, as Rust's spawned read guarantees.
 */
export function startListingEngine(sink: ListingEventSink): ListingEngineHandle {
  const ready = (async () => {
    const wasm = await import('virtual:cmdr-listing-wasm')
    await wasm.default()
    const tree = { volumeId: DEMO_VOLUME_ID, root: DEMO_ROOT, entries: demoEntries }
    const engine = new wasm.DemoListing(
      tree,
      (event, payload, target) => {
        setTimeout(() => {
          sink(event, payload, target)
        }, 0)
      },
      demoContents(),
    )
    return { DemoListing: wasm.DemoListing, engine }
  })().catch((error: unknown) => {
    // Once, loudly: without the engine every listing command shows up as unanswered.
    // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
    console.error('[demo] the listing engine did not load', error)
    return null
  })

  return {
    handles: async (cmd) => (await ready)?.DemoListing.handles(cmd) ?? false,
    invoke: async (cmd, payload) => {
      const loaded = await ready
      if (!loaded) throw new Error(`[demo] the listing engine did not load, so it can't answer ${cmd}`)
      return loaded.engine.invoke(cmd, expandPaths(payload))
    },
    closeViewerWindow: (label) => {
      void ready.then((loaded) => loaded?.engine.closeViewerWindow(label))
    },
  }
}
