import { describe, expect, it } from 'vitest'
import { browserSafeShortcuts } from './browser-shortcuts.js'
import { DEMO_HOME } from './demo-tree.js'
import { pluginHandler } from './plugins.js'

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

  it('seeds the browser-safe shortcuts as shortcut:<id> keys, as the shortcuts store reads them', () => {
    const rid = load('shortcuts.json')
    for (const [id, combos] of Object.entries(browserSafeShortcuts)) {
      expect(call('plugin:store|get', { rid, key: `shortcut:${id}` })).toEqual([combos, true])
    }
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

  it('names the app and its version', () => {
    expect(call('plugin:app|name')).toBe('Cmdr')
    expect(call('plugin:app|version')).toBe('demo')
    expect(call('plugin:app|tauri_version')).toBe('2')
    expect(call('plugin:app|set_app_theme', { theme: 'dark' })).toBeUndefined()
  })

  it('knows no other plugin command', () => {
    expect(pluginHandler('plugin:dialog|open')).toBeUndefined()
  })
})
