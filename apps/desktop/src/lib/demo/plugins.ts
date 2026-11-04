/**
 * Tauri plugin commands (`plugin:<name>|<command>`) the demo answers itself. The store plugin keeps
 * each file in memory, so a demo session's settings live until the tab reloads.
 */
import { browserSafeShortcuts } from './browser-shortcuts.js'
import { DEMO_HOME } from './demo-tree.js'

/** What a fresh demo starts with, per store file. Only keys an actor would have set (sparse, like the app). */
const seeds: Record<string, Record<string, unknown>> = {
  // Skip the first-run wizard: a visitor came to try the panes.
  'settings.json': { 'onboarding.completed': true },
  'shortcuts.json': browserSafeShortcuts,
}

const storesByPath = new Map<string, number>()
const stores = new Map<number, Map<string, unknown>>()
let nextRid = 1

/** `load`'s `defaults`, applied the way the plugin does: only to keys the file doesn't hold. */
function storeFor(path: string, defaults: Record<string, unknown> = {}): number {
  const known = storesByPath.get(path)
  if (known !== undefined) return known
  const rid = nextRid++
  storesByPath.set(path, rid)
  stores.set(rid, new Map(Object.entries({ ...defaults, ...seeds[path] })))
  return rid
}

function entriesOf(payload: Record<string, unknown>): Map<string, unknown> {
  const entries = stores.get(payload.rid as number)
  if (!entries) throw new Error(`Demo store ${String(payload.rid)} isn't loaded`)
  return entries
}

type PluginHandler = (payload: Record<string, unknown>) => unknown

const handlers: Record<string, PluginHandler> = {
  'plugin:store|load': (p) =>
    storeFor(p.path as string, (p.options as { defaults?: Record<string, unknown> } | undefined)?.defaults),
  'plugin:store|get_store': (p) => storesByPath.get(p.path as string) ?? null,
  'plugin:store|get': (p) => {
    const entries = entriesOf(p)
    const key = p.key as string
    return [entries.get(key) ?? null, entries.has(key)]
  },
  'plugin:store|set': (p) => void entriesOf(p).set(p.key as string, p.value),
  'plugin:store|has': (p) => entriesOf(p).has(p.key as string),
  'plugin:store|delete': (p) => entriesOf(p).delete(p.key as string),
  'plugin:store|clear': (p) => {
    entriesOf(p).clear()
  },
  'plugin:store|reset': (p) => {
    entriesOf(p).clear()
  },
  'plugin:store|keys': (p) => [...entriesOf(p).keys()],
  'plugin:store|values': (p) => [...entriesOf(p).values()],
  'plugin:store|entries': (p) => [...entriesOf(p).entries()],
  'plugin:store|length': (p) => entriesOf(p).size,
  'plugin:store|save': () => undefined,
  'plugin:store|reload': () => undefined,
  // Every standard folder resolves inside the demo's home, which is all the app asks for.
  'plugin:path|resolve_directory': () => DEMO_HOME,
  // The "window" is the browser viewport.
  'plugin:window|outer_size': () => ({ width: window.innerWidth, height: window.innerHeight }),
  'plugin:window|inner_size': () => ({ width: window.innerWidth, height: window.innerHeight }),
  'plugin:window|scale_factor': () => window.devicePixelRatio,
  'plugin:window|is_fullscreen': () => false,
  'plugin:window|is_maximized': () => false,
  'plugin:window|is_focused': () => document.hasFocus(),
  'plugin:window|is_visible': () => true,
  'plugin:window|title': () => document.title,
  'plugin:app|version': () => 'demo',
  'plugin:app|set_app_theme': () => undefined,
  'plugin:app|name': () => 'Cmdr',
  'plugin:app|tauri_version': () => '2',
  'plugin:resources|close': () => undefined,
}

/** The plugin's answer, or `undefined` for a plugin command the demo doesn't know. */
export function pluginHandler(cmd: string): PluginHandler | undefined {
  return handlers[cmd]
}
