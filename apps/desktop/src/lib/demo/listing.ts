/**
 * The demo's listing commands: the seam where the real listing engine, built to WASM, answers in
 * place of the Tauri backend. This file only routes; it holds no sorting, filtering, or fuzzy
 * logic, so the demo lists exactly like the app. See `DETAILS.md` § The listing seam.
 */
// eslint-disable-next-line cmdr/no-raw-bindings-import -- the demo plays the backend: it EMITS these events, which no app wrapper does
import { events } from '#lib/ipc/bindings.js'
import type { AnswerTable, CommandName, Payload, RawAnswer } from './answer-types.js'
import { demoEntries, type DemoEntry } from './demo-tree.js'
import { noteUnanswered } from './unanswered.js'

/** Every command the listing engine answers, by its bindings name. */
const listingCommands = [
  'listDirectoryStartStreaming',
  'cancelListing',
  'listDirectoryEnd',
  'keepListingsAlive',
  'refreshListing',
  'getFileRange',
  'getFileAt',
  'getFileBeside',
  'findFileIndex',
  'findFileIndices',
  'findFirstFuzzyMatch',
  'resortListing',
  'setListingIncludeHidden',
  'setListingNameFilter',
  'getListingStats',
  'getSelectionSnapshot',
  'getSelectionEntries',
  'getBriefColumnTextWidths',
  'compareDirectories',
  'statPathsKinds',
] as const satisfies readonly CommandName[]

export type ListingCommand = (typeof listingCommands)[number]

type Events = typeof events

/** Every event the listing engine may send, by its bindings name. */
type ListingEventName =
  | 'listingOpening'
  | 'listingProgress'
  | 'listingReadComplete'
  | 'listingComplete'
  | 'listingError'
  | 'listingCancelled'
  | 'listingStalled'
  | 'listingRespelled'
  | 'listingIndexSizesChanged'
  | 'directoryDiff'

type EventPayload<K extends keyof Events> = Parameters<Events[K]['emit']>[0]

/** One event from the engine, typed against the bindings' payload for that event. */
export type DemoListingEvent = { [K in ListingEventName]: { event: K; payload: EventPayload<K> } }[ListingEventName]

/**
 * What the WASM shim provides. `call` takes the command's bindings name and its IPC payload (the
 * same camelCase object the Rust `#[tauri::command]` deserializes) and returns the command's `ok`
 * value. For a `typedError` command's error branch it throws the typed error value, exactly as a
 * Rust `Err` reaches `invoke`.
 */
export interface DemoListingEngine {
  call<K extends ListingCommand>(command: K, payload: Payload): RawAnswer<K> | Promise<RawAnswer<K>>
}

/** What the engine starts from: the demo's file tree, and where it sends its events. */
export interface DemoListingEngineInit {
  entries: readonly DemoEntry[]
  emit: (event: DemoListingEvent) => void
}

/**
 * Hands an engine event to the app's listeners. Deferred to a fresh task, in order: the in-memory
 * engine finishes a listing inside `listDirectoryStartStreaming`, but the app expects the start
 * command's answer before that listing's first event, as it gets from Rust's spawned read.
 */
function emit(event: DemoListingEvent): void {
  setTimeout(() => {
    void (events[event.event].emit as (payload: unknown) => Promise<void>)(event.payload)
  }, 0)
}

/**
 * Loads the engine. WASM SEAM: until the `wasm-bindgen` package lands, this resolves `null` and
 * every listing command shows up in `window.__cmdrDemo.unanswered`. With it, this becomes a
 * dynamic `import()` of the package (so the WASM downloads in parallel with the app's own boot),
 * its `init()`, and an adapter from its API to `DemoListingEngine`.
 */
// eslint-disable-next-line @typescript-eslint/require-await -- the WASM loader awaits; this keeps its signature
async function loadEngine(_init: DemoListingEngineInit): Promise<DemoListingEngine | null> {
  return null
}

let engine: Promise<DemoListingEngine | null> | undefined

/** Starts loading the engine. Called once by `boot.ts`, before the app's first listing. */
export function startListingEngine(): void {
  engine ??= loadEngine({ entries: demoEntries, emit })
}

function route<K extends ListingCommand>(command: K) {
  return async (payload: Payload): Promise<RawAnswer<K>> => {
    const loaded = await engine
    if (!loaded) return noteUnanswered(command, payload) as RawAnswer<K>
    return loaded.call(command, payload)
  }
}

/** The listing commands, each routed to the engine. A function, so a non-demo build tree-shakes it. */
export function listingAnswers(): AnswerTable {
  return Object.fromEntries(listingCommands.map((command) => [command, route(command)]))
}

/** Listing-adjacent commands with nothing to compute: the demo volume is local and never syncs. */
export const listingSideAnswers: AnswerTable = {
  getSyncStatus: () => ({ data: {}, timedOut: false }),
  storedSpellings: (p) => ({ data: p.paths as string[], timedOut: false }),
}
