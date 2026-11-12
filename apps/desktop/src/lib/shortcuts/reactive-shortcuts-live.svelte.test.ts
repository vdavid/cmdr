/**
 * The reactive hint readers show only LIVE keys: a default another command's user binding
 * shadows runs that other command (`binding-priority.ts`), so a chip, tooltip, or palette row
 * naming it would send the user to the wrong command.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const effective = new Map<string, string[]>()
const defaults = new Map<string, string[]>()

vi.mock('./shortcuts-store', () => ({
  // Stand-in for the store's binding rule: `view.showHidden`'s user binding takes `tab.new`'s
  // first default, so only `tab.new`'s second key is live.
  getLiveShortcuts: (id: string) =>
    (effective.get(id) ?? defaults.get(id) ?? []).filter(
      (combo) => !(id === 'tab.new' && (effective.get('view.showHidden') ?? []).includes(combo)),
    ),
  onShortcutChange: () => () => {},
}))

import {
  commandHasShortcut,
  getFirstShiftShortcutReactive,
  getFirstShortcutReactive,
  getLiveShortcutsReactive,
  shortcutAsText,
} from './reactive-shortcuts.svelte'

beforeEach(() => {
  effective.clear()
  defaults.clear()
})

describe('reactive hint readers', () => {
  it('skip a default the user gave to another command, and show the next live key', () => {
    defaults.set('tab.new', ['Ctrl+T', 'Shift+F4'])
    effective.set('view.showHidden', ['Ctrl+T'])
    expect(getFirstShortcutReactive('tab.new')).toBe('Shift+F4')
    expect(getLiveShortcutsReactive('tab.new')).toEqual(['Shift+F4'])
    expect(getFirstShiftShortcutReactive('tab.new')).toBe('Shift+F4')
    expect(shortcutAsText('tab.new', 'tooltip')).toBe('Shift+F4')
    expect(commandHasShortcut('tab.new')).toBe(true)
  })

  it('show no key for a command whose every key is shadowed', () => {
    defaults.set('tab.new', ['Ctrl+T'])
    effective.set('view.showHidden', ['Ctrl+T'])
    expect(getFirstShortcutReactive('tab.new')).toBeUndefined()
    expect(getLiveShortcutsReactive('tab.new')).toEqual([])
    expect(shortcutAsText('tab.new', 'toast')).toBeUndefined()
    expect(commandHasShortcut('tab.new')).toBe(false)
  })
})
