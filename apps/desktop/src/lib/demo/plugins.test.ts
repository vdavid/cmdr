import { readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { browserSafeShortcuts } from './browser-shortcuts.js'
import { DEMO_HOME } from './demo-tree.js'
import { customShortcuts, pluginHandler, restrictedWindowSettings } from './plugins.js'

/** Calls one plugin command, failing the test when the demo doesn't answer it. */
function call(cmd: string, payload: Record<string, unknown> = {}): unknown {
  const handler = pluginHandler(cmd)
  if (!handler) throw new Error(`no handler for ${cmd}`)
  return handler(payload)
}

// The store map is module state, so every test loads its own file name.
function load(path: string, defaults?: Record<string, unknown>): number {
  return call('plugin:store|load', { path, options: defaults ? { defaults } : undefined }) as number
}

describe('in-memory store plugin', () => {
  it('applies load defaults, with seeds winning over them', () => {
    const rid = load('settings.json', { 'onboarding.completed': false, 'listing.showHiddenFiles': false })
    expect(call('plugin:store|get', { rid, key: 'onboarding.completed' })).toEqual([true, true])
    expect(call('plugin:store|get', { rid, key: 'listing.showHiddenFiles' })).toEqual([false, true])
  })

  it('seeds Cmdr gold as the app color, so "System default" stays a choice in Settings', () => {
    const rid = load('settings.json')
    expect(call('plugin:store|get', { rid, key: 'appearance.appColor' })).toEqual(['cmdr-gold', true])
  })

  it('seeds a 125% zoom, through the app’s own text size setting', () => {
    const rid = load('settings.json')
    expect(call('plugin:store|get', { rid, key: 'appearance.textSize' })).toEqual([125, true])
  })

  it('answers the viewer’s settings snapshot from the store, as the desktop reads settings.json', () => {
    // Regression: `get_restricted_window_settings` went unanswered, so the viewer and the queue ran on registry
    // defaults (100% zoom, the system color) while the main window showed the seeds.
    const rid = load('settings.json')
    call('plugin:store|set', { rid, key: 'viewer.wordWrap', value: true })
    call('plugin:store|set', { rid, key: 'appearance.language', value: 'hu' })
    expect(restrictedWindowSettings()).toEqual({
      viewerWordWrap: true,
      viewerShowTextCursor: null,
      fileViewerSuppressBinaryWarning: null,
      appearanceTextSize: 125,
      appearanceAppColor: 'cmdr-gold',
      appearanceFileSizeFormat: null,
      appearanceLanguage: 'hu',
    })
    call('plugin:store|delete', { rid, key: 'viewer.wordWrap' })
    call('plugin:store|delete', { rid, key: 'appearance.language' })
  })

  it('seeds the browser-safe shortcuts as shortcut:<id> keys, as the shortcuts store reads them', () => {
    const rid = load('shortcuts.json')
    for (const [id, combos] of Object.entries(browserSafeShortcuts)) {
      expect(call('plugin:store|get', { rid, key: `shortcut:${id}` })).toEqual([combos, true])
    }
  })

  it('reads the custom shortcuts back the way the viewer window asks for them', () => {
    // Regression: `get_restricted_window_shortcuts` (the viewer's one shortcut read) went unanswered.
    const rid = load('shortcuts.json')
    call('plugin:store|set', { rid, key: 'shortcut:viewer.close', value: ['⌃W'] })
    call('plugin:store|set', { rid, key: 'not-a-shortcut', value: ['x'] })
    const read = customShortcuts()
    expect(read['viewer.close']).toEqual(['⌃W'])
    expect(read).not.toHaveProperty('not-a-shortcut')
    for (const id of Object.keys(browserSafeShortcuts)) expect(read).toHaveProperty(id)
  })

  it('answers get with [value, exists], so a missing key reads as absent', () => {
    const rid = load('get-test.json')
    expect(call('plugin:store|get', { rid, key: 'nope' })).toEqual([null, false])
    call('plugin:store|set', { rid, key: 'k', value: null })
    expect(call('plugin:store|get', { rid, key: 'k' })).toEqual([null, true])
  })

  it('returns the same store for the same file, found again by get_store', () => {
    const rid = load('same.json')
    expect(load('same.json')).toBe(rid)
    expect(call('plugin:store|get_store', { path: 'same.json' })).toBe(rid)
    expect(call('plugin:store|get_store', { path: 'never-loaded.json' })).toBeNull()
  })

  it('sets, lists, deletes, and clears', () => {
    const rid = load('crud.json')
    call('plugin:store|set', { rid, key: 'a', value: 1 })
    call('plugin:store|set', { rid, key: 'b', value: 2 })
    expect(call('plugin:store|has', { rid, key: 'a' })).toBe(true)
    expect(call('plugin:store|keys', { rid })).toEqual(['a', 'b'])
    expect(call('plugin:store|values', { rid })).toEqual([1, 2])
    expect(call('plugin:store|entries', { rid })).toEqual([
      ['a', 1],
      ['b', 2],
    ])
    expect(call('plugin:store|length', { rid })).toBe(2)
    expect(call('plugin:store|delete', { rid, key: 'a' })).toBe(true)
    expect(call('plugin:store|has', { rid, key: 'a' })).toBe(false)
    call('plugin:store|clear', { rid })
    expect(call('plugin:store|length', { rid })).toBe(0)
  })

  it('resets to the load defaults, not to empty', () => {
    const rid = load('reset.json', { theme: 'system' })
    call('plugin:store|set', { rid, key: 'theme', value: 'dark' })
    call('plugin:store|set', { rid, key: 'extra', value: 1 })
    call('plugin:store|reset', { rid })
    expect(call('plugin:store|entries', { rid })).toEqual([['theme', 'system']])
  })

  it('treats save and reload as no-ops, and refuses an unknown store id', () => {
    const rid = load('noop.json')
    expect(call('plugin:store|save', { rid })).toBeUndefined()
    expect(call('plugin:store|reload', { rid })).toBeUndefined()
    expect(call('plugin:resources|close', { rid })).toBeUndefined()
    expect(() => call('plugin:store|keys', { rid: 99_999 })).toThrow()
  })
})

describe('path and app plugins', () => {
  it('resolves every standard folder inside the demo home', () => {
    expect(call('plugin:path|resolve_directory', { directory: 1 })).toBe(DEMO_HOME)
  })

  it('joins paths as the plugin does on macOS', () => {
    expect(call('plugin:path|join', { paths: ['/Users/you', 'drag-image.png'] })).toBe('/Users/you/drag-image.png')
    expect(call('plugin:path|join', { paths: ['/a/', './b', '../c//d'] })).toBe('/a/c/d')
    expect(call('plugin:path|join', { paths: ['a', 'b'] })).toBe('a/b')
  })

  it('takes the drag image and drops it, for there is no OS drag session to read it', () => {
    expect(call('plugin:fs|write_file', {})).toBeUndefined()
    expect(call('plugin:fs|remove', { path: '/Users/you/drag-image.png' })).toBeUndefined()
  })

  // The demo takes every `plugin-fs` write and keeps nothing, which is only honest while the drag
  // image is the one writer. A second user needs its own answer (or the volume), not this one.
  it('has the drag image as the only plugin-fs user in the app', () => {
    const src = path.resolve(import.meta.dirname, '../..')
    const users = readdirSync(src, { recursive: true, encoding: 'utf8' })
      .filter((file) => /\.(ts|svelte)$/.test(file) && !/\.test\.ts$/.test(file))
      .filter((file) => readFileSync(path.join(src, file), 'utf8').includes("'@tauri-apps/plugin-fs'"))
    expect(users).toEqual(['lib/file-explorer/drag/drag-drop.ts'])
  })

  it('names the app and its version', () => {
    expect(call('plugin:app|name')).toBe('Cmdr')
    expect(call('plugin:app|version')).toBe('demo')
    expect(call('plugin:app|tauri_version')).toBe('2')
  })

  it('knows no other plugin command', () => {
    expect(pluginHandler('plugin:dialog|open')).toBeUndefined()
  })
})

describe('opener plugin', () => {
  it('opens a web or email link in a new browser tab, cut off from this page', () => {
    const opened = vi.spyOn(window, 'open').mockImplementation(() => null)
    try {
      for (const url of ['https://github.com/vdavid/cmdr', 'http://example.com/', 'mailto:hello@getcmdr.com']) {
        expect(call('plugin:opener|open_url', { url })).toBeNull()
        expect(opened).toHaveBeenLastCalledWith(url, '_blank', 'noopener,noreferrer')
      }
    } finally {
      opened.mockRestore()
    }
  })

  it('refuses any other scheme, as the plugin refuses a URL its scope disallows', () => {
    const opened = vi.spyOn(window, 'open').mockImplementation(() => null)
    try {
      for (const url of ['javascript:alert(1)', 'file:///etc/passwd', 'x-apple.systempreferences:', 'not a url']) {
        expect(() => call('plugin:opener|open_url', { url })).toThrow()
      }
      expect(opened).not.toHaveBeenCalled()
    } finally {
      opened.mockRestore()
    }
  })
})
