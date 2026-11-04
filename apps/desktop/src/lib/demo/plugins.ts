/**
 * Tauri plugin commands (`plugin:<name>|<command>`) the backend answers: the store plugin, which
 * keeps each file in memory (a demo session's settings live until the tab reloads, and every
 * webview sees the same values, as with the real plugin), plus `path` and `app`. Commands about one
 * webview's own window live in `webview.ts`.
 */
import { browserSafeShortcuts } from './browser-shortcuts.js'
import { DEMO_HOME } from './demo-tree.js'

/** What a fresh demo starts with, per store file. Only keys an actor would have set (sparse, like the app). */
const seeds: Record<string, Record<string, unknown>> = {
  'settings.json': {
    // A user who just finished today's wizard (`startup-gates.ts::completeOnboardingWizard` writes
    // both): a visitor came to try the panes, and there's no newer onboarding to point them at.
    'onboarding.completed': true,
    'onboarding.upgradeNudgeShown': true,
    // The demo shows Cmdr in its own color. "System default" stays in Settings and picks the
    // virtual Mac's accent, the blue `getAccentColor` answers.
    'appearance.appColor': 'cmdr-gold',
  },
  // One `shortcut:<commandId>` key per customization, as `shortcuts-store.ts` reads them.
  'shortcuts.json': Object.fromEntries(
    Object.entries(browserSafeShortcuts).map(([id, keys]) => [`shortcut:${id}`, keys]),
  ),
}

const storesByPath = new Map<string, number>()
const stores = new Map<number, { defaults: Record<string, unknown>; entries: Map<string, unknown> }>()
let nextRid = 1

/** `load`'s `defaults`, applied the way the plugin does: only to keys the file doesn't hold. */
function storeFor(path: string, defaults: Record<string, unknown> = {}): number {
  const known = storesByPath.get(path)
  if (known !== undefined) return known
  const rid = nextRid++
  storesByPath.set(path, rid)
  stores.set(rid, { defaults, entries: new Map(Object.entries({ ...defaults, ...seeds[path] })) })
  return rid
}

function storeOf(payload: Record<string, unknown>) {
  const store = stores.get(payload.rid as number)
  if (!store) throw new Error(`Demo store ${String(payload.rid)} isn't loaded`)
  return store
}

function entriesOf(payload: Record<string, unknown>): Map<string, unknown> {
  return storeOf(payload).entries
}

type PluginHandler = (payload: Record<string, unknown>) => unknown

const handlers: Partial<Record<string, PluginHandler>> = {
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
  // Back to the `load` defaults, as the plugin does.
  'plugin:store|reset': (p) => {
    const store = storeOf(p)
    store.entries = new Map(Object.entries(store.defaults))
  },
  'plugin:store|keys': (p) => [...entriesOf(p).keys()],
  'plugin:store|values': (p) => [...entriesOf(p).values()],
  'plugin:store|entries': (p) => [...entriesOf(p).entries()],
  'plugin:store|length': (p) => entriesOf(p).size,
  'plugin:store|save': () => undefined,
  'plugin:store|reload': () => undefined,
  'plugin:resources|close': () => undefined,
  // Every standard folder resolves inside the demo's home, which is all the app asks for.
  'plugin:path|resolve_directory': () => DEMO_HOME,
  'plugin:app|version': () => 'demo',
  'plugin:app|set_app_theme': () => undefined,
  'plugin:app|name': () => 'Cmdr',
  'plugin:app|tauri_version': () => '2',
}

/** The backend's answer for a plugin command, or `undefined` for one it doesn't know. */
export function pluginHandler(cmd: string): PluginHandler | undefined {
  return handlers[cmd]
}
