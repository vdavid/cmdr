/**
 * The demo's listing engine: the real `cmdr-listing` built to WASM (`crates/cmdr-listing-wasm`),
 * answering the listing commands in place of the Tauri backend. This file only loads and routes; it
 * holds no sorting, filtering, or fuzzy logic, so the demo lists exactly like the app. See
 * `DETAILS.md` § The listing seam.
 */
import type { Payload } from './answer-types.js'
import { demoContents } from './demo-contents.js'
import { DEMO_HOME, DEMO_ROOT, DEMO_VOLUME_ID, demoEntries } from './demo-tree.js'
import { loadEngineModule } from './engine-module.js'
import type { DropImporter } from './os-drop.js'
import { demoPath } from './volumes.js'

/** The loaded engine, by Tauri command name, as the backend routes to it. */
export interface ListingEngineHandle {
  /** Whether the engine answers `cmd`. Waits for the engine to load, so the first listing never races it. */
  handles(cmd: string): Promise<boolean>
  /** Answers like `invoke`: resolves with the answer, rejects with the typed error value. */
  invoke(cmd: string, payload: Payload): Promise<unknown>
  /** Frees the viewer sessions (and media tokens) of the window `label`, once it closed. */
  closeViewerWindow(label: string): void
  /** A viewer media token's bytes for the window `label`, or `undefined` (an unknown token, or no engine yet). */
  mediaBody(token: string, label: string): { mime: string; bytes: Uint8Array } | undefined
  /** The engine's door for files dropped in from the computer (`os-drop.ts`); waits for the engine. */
  importer: DropImporter
}

/** One engine event: its Tauri name, its payload, and the window label an `emit_to` names. */
export interface EngineEvent {
  name: string
  payload: unknown
  target?: string
}

/** Where the engine's events go. */
export type ListingEventSink = (event: EngineEvent) => void

/**
 * Every argument that names a path on the demo volume, across the listing and write commands. The
 * desktop's commands expand `~` in these for `root` (`expand_tilde`); the engine has no home folder.
 */
const PATH_ARGS = ['path', 'parentPath', 'destPath', 'destination', 'from', 'to', 'dir'] as const
const PATH_LIST_ARGS = ['paths', 'sources', 'sourcePaths'] as const

/** The engine's view of a path: `~` expanded (it has no home folder), no trailing slash. */
function expandPaths(payload: Payload): Payload {
  const expanded = { ...payload }
  for (const key of PATH_ARGS) {
    const value = expanded[key]
    if (typeof value === 'string') expanded[key] = demoPath(value)
  }
  for (const key of PATH_LIST_ARGS) {
    const value = expanded[key]
    if (Array.isArray(value)) expanded[key] = value.map((p: unknown) => (typeof p === 'string' ? demoPath(p) : p))
  }
  return expanded
}

/**
 * The free space the page's `?free=<bytes>` asks the disk to start with, so a visitor (or the E2E
 * spec) can watch a copy fail on a full disk and Retry it after a delete. `undefined` (the default,
 * or anything that isn't a whole number of bytes) gives the disk the size the volume list shows.
 */
export function freeBytesFromSearch(search: string): number | undefined {
  const raw = new URLSearchParams(search).get('free')
  if (raw === null || !/^\d{1,15}$/.test(raw)) return undefined
  return Number(raw)
}

/**
 * Starts loading the engine and returns its handle at once. The WASM downloads while the app boots;
 * the first listing command waits for it. Events go to `sink` a task late, in order: the in-memory
 * read can finish inside `list_directory_start_streaming`, but the pane expects that command's answer
 * before the listing's first event, as Rust's spawned read guarantees.
 */
export function startListingEngine(sink: ListingEventSink): ListingEngineHandle {
  // Set once the engine loads: a media URL is built synchronously, long after any open needed it.
  let engine: { mediaBody(token: string, label: string): { mime: string; bytes: Uint8Array } | undefined } | null = null
  const ready = loadEngineModule().then(async (wasm) => {
    if (!wasm) return null
    const contents = await demoContents()
    const tree = {
      volumeId: DEMO_VOLUME_ID,
      volumeName: 'Macintosh HD',
      home: DEMO_HOME,
      freeBytes: freeBytesFromSearch(window.location.search),
      root: DEMO_ROOT,
      entries: demoEntries,
    }
    try {
      const loadedEngine = new wasm.DemoListing(
        tree,
        (event, payload, target) => {
          setTimeout(() => {
            sink({ name: event, payload, target })
          }, 0)
        },
        contents,
      )
      engine = loadedEngine
      return { DemoListing: wasm.DemoListing, engine: loadedEngine }
    } catch (error) {
      // The engine refuses a bad tree at construction; every listing command is then unanswered.
      // eslint-disable-next-line no-console -- the demo's developer reads the devtools console
      console.error('[demo] the listing engine refused the demo tree', error)
      return null
    }
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
    mediaBody: (token, label) => engine?.mediaBody(token, label),
    importer: {
      plan: async (items) => {
        const loaded = await ready
        if (!loaded) throw new Error("[demo] the listing engine did not load, so it can't take a drop")
        return loaded.engine.importPlan(items)
      },
      file: async (batch, index, bytes) => {
        const loaded = await ready
        if (!loaded) throw new Error("[demo] the listing engine did not load, so it can't take a drop")
        return loaded.engine.importFile(batch, index, bytes)
      },
      handOff: async (batch, offered) => (await ready)?.engine.importHandOff(batch, offered),
      releaseOffered: async () => (await ready)?.engine.importReleaseOffered(),
    },
  }
}
