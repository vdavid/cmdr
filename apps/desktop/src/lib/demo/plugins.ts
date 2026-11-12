/**
 * Tauri plugin commands (`plugin:<name>|<command>`) the backend answers: the store plugin, which
 * keeps each file in memory (a demo session's settings live until the tab reloads, and every
 * webview sees the same values, as with the real plugin), plus `path`, `app`, the drag image's `fs`
 * writes, and the opener's `open_url` (a new browser tab, `opener.ts`). Commands about one webview's
 * own window live in `webview.ts`.
 */
import type { RestrictedWindowSettings } from '#lib/ipc/bindings.js'
import { browserSafeShortcuts } from './browser-shortcuts.js'
import { visitorPlatform } from './platform.js'
import { DEMO_HOME } from './demo-tree.js'
import { openInBrowserTab } from './opener.js'

/** What a fresh demo starts with, per store file. Only keys an actor would have set (sparse, like the app). */
const seeds: Partial<Record<string, () => Record<string, unknown>>> = {
  'settings.json': () => ({
    // A user who just finished today's wizard (`startup-gates.ts::completeOnboardingWizard` writes
    // both): a visitor came to try the panes, and there's no newer onboarding to point them at.
    'onboarding.completed': true,
    'onboarding.upgradeNudgeShown': true,
    // The demo shows Cmdr in its own color. "System default" stays in Settings and picks the
    // virtual Mac's accent, the blue `getAccentColor` answers.
    'appearance.appColor': 'cmdr-gold',
    // A browser tab is usually smaller than a Mac's display at its default zoom, and the demo reads better a bit
    // bigger. The app's own zoom setting, so Settings shows 125% and can take it back to 100%.
    'appearance.textSize': 125,
    // The demo doesn't browse archives (DETAILS § Archives). Open sends Enter on one to `open_path`,
    // which the backend answers with a notice, where the registry's Ask would offer a Browse that
    // can only end on an error screen. Settings still offers all three.
    'behavior.archiveEnter.zip': 'open',
  }),
  // One `shortcut:<commandId>` key per customization, as `shortcuts-store.ts` reads them, in the
  // visitor's own OS's spelling (the same answer `platform.ts` gives every frame).
  'shortcuts.json': () =>
    Object.fromEntries(
      Object.entries(browserSafeShortcuts(visitorPlatform(navigator.userAgent))).map(([id, keys]) => [
        `shortcut:${id}`,
        keys,
      ]),
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
  stores.set(rid, { defaults, entries: new Map(Object.entries({ ...defaults, ...seeds[path]?.() })) })
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

/** `path.join` as the plugin answers it on macOS: `/`-joined, with `.`, `..`, and doubled slashes folded. */
function joinPaths(parts: string[]): string {
  const absolute = parts[0]?.startsWith('/') ?? false
  const segments: string[] = []
  for (const segment of parts.join('/').split('/')) {
    if (segment === '' || segment === '.') continue
    if (segment === '..') segments.pop()
    else segments.push(segment)
  }
  return (absolute ? '/' : '') + segments.join('/')
}

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
  'plugin:path|join': (p) => joinPaths(p.paths as string[]),
  // The one `plugin-fs` user is the drag image (`file-explorer/drag/drag-drop.ts`), a temp PNG for
  // the OS drag session to read. A page has no OS to read it, so it's taken and dropped; the volume
  // never sees it. `plugins.test.ts` keeps it the one user.
  'plugin:fs|write_file': () => undefined,
  'plugin:fs|remove': () => undefined,
  'plugin:app|version': () => 'demo',
  'plugin:app|name': () => 'Cmdr',
  'plugin:app|tauri_version': () => '2',
  // `openExternalUrl`: a new browser tab, for web and email links only (`opener.ts`).
  'plugin:opener|open_url': (p) => openInBrowserTab(p.url as string),
}

/**
 * The custom shortcuts `shortcuts.json` holds, by command id: what the desktop's
 * `get_restricted_window_shortcuts` reads off disk for a window that can't open the
 * store (the viewer). Only `shortcut:*` keys with a list, and only its strings.
 */
export function customShortcuts(): Record<string, string[]> {
  const entries = stores.get(storeFor('shortcuts.json'))?.entries ?? new Map<string, unknown>()
  const custom: Record<string, string[]> = {}
  for (const [key, value] of entries) {
    if (!key.startsWith('shortcut:') || !Array.isArray(value)) continue
    custom[key.slice('shortcut:'.length)] = value.filter((combo): combo is string => typeof combo === 'string')
  }
  return custom
}

/**
 * The settings snapshot a restricted window (the viewer, the queue) starts on, read off `settings.json` the way the
 * desktop's `parse_restricted_window_settings` reads the file: a key that isn't there, or holds the wrong type, is
 * `null` (the registry default).
 */
export function restrictedWindowSettings(): RestrictedWindowSettings {
  const entries = stores.get(storeFor('settings.json'))?.entries ?? new Map<string, unknown>()
  const flag = (key: string) => {
    const value = entries.get(key)
    return typeof value === 'boolean' ? value : null
  }
  const number = (key: string) => {
    const value = entries.get(key)
    return typeof value === 'number' ? value : null
  }
  const text = (key: string) => {
    const value = entries.get(key)
    return typeof value === 'string' ? value : null
  }
  return {
    viewerWordWrap: flag('viewer.wordWrap'),
    viewerShowTextCursor: flag('viewer.showTextCursor'),
    fileViewerSuppressBinaryWarning: flag('fileViewer.suppressBinaryWarning'),
    appearanceTextSize: number('appearance.textSize'),
    appearanceAppColor: text('appearance.appColor'),
    appearanceFileSizeFormat: text('appearance.fileSizeFormat'),
    appearanceLanguage: text('appearance.language'),
  }
}

/** The backend's answer for a plugin command, or `undefined` for one it doesn't know. */
export function pluginHandler(cmd: string): PluginHandler | undefined {
  return handlers[cmd]
}
