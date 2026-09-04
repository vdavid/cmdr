/**
 * The list rules behind the "Open terminal here uses" row.
 *
 * Two things worth pinning: the escape hatch is always the last row (so a
 * terminal Cmdr carries no recipe for is never a dead end), and an uninstalled
 * choice reads as Terminal rather than as nothing, matching what the action
 * itself does when it falls back.
 */

import { describe, it, expect } from 'vitest'
import type { TerminalApp } from '$lib/ipc/bindings'
import {
  CHOOSE_APP_VALUE,
  TERMINAL_APP_BUNDLE_ID,
  selectedTerminalAppId,
  terminalAppItems,
} from './terminal-app-options'

function app(id: string, displayName: string, icon: string | null = null): TerminalApp {
  return { id, displayName, icon, isRunning: false }
}

const TERMINAL = app(TERMINAL_APP_BUNDLE_ID, 'Terminal', 'data:image/webp;base64,dGVybQ==')
const GHOSTTY = app('com.mitchellh.ghostty', 'Ghostty')

describe('terminalAppItems', () => {
  it('offers every installed terminal in the order the backend listed them', () => {
    const items = terminalAppItems([TERMINAL, GHOSTTY], 'Choose an app…')
    expect(items.slice(0, 2).map((i) => [i.value, i.label])).toEqual([
      [TERMINAL_APP_BUNDLE_ID, 'Terminal'],
      ['com.mitchellh.ghostty', 'Ghostty'],
    ])
  })

  it('puts the app picker last, so an unknown terminal is never a dead end', () => {
    const items = terminalAppItems([TERMINAL, GHOSTTY], 'Choose an app…')
    expect(items.at(-1)).toEqual({ value: CHOOSE_APP_VALUE, label: 'Choose an app…' })
  })

  it('still offers the picker when nothing else is installed', () => {
    expect(terminalAppItems([], 'Choose an app…')).toEqual([{ value: CHOOSE_APP_VALUE, label: 'Choose an app…' }])
  })

  it('carries each app icon through as the row image', () => {
    const items = terminalAppItems([TERMINAL, GHOSTTY], 'Choose an app…')
    expect(items[0].iconUrl).toBe('data:image/webp;base64,dGVybQ==')
    // A bundle with no readable icon comes back `null`; the row just has no image.
    expect(items[1].iconUrl).toBeUndefined()
  })

  it('shows a custom pick under the name the backend read off its bundle', () => {
    const custom = app('/Applications/Terminus.app', 'Terminus')
    const items = terminalAppItems([TERMINAL, custom], 'Choose an app…')
    expect(items[1]).toMatchObject({ value: '/Applications/Terminus.app', label: 'Terminus' })
  })
})

describe('selectedTerminalAppId', () => {
  it('selects the chosen app', () => {
    expect(selectedTerminalAppId({ apps: [TERMINAL, GHOSTTY], chosenId: 'com.mitchellh.ghostty' })).toBe(
      'com.mitchellh.ghostty',
    )
  })

  it('falls back to Terminal when the chosen app has been uninstalled', () => {
    // `chosenId: null` is exactly how the backend reports that: the stored app
    // isn't in `apps` anymore.
    expect(selectedTerminalAppId({ apps: [TERMINAL], chosenId: null })).toBe(TERMINAL_APP_BUNDLE_ID)
  })

  it('selects nothing while the list is still empty', () => {
    expect(selectedTerminalAppId({ apps: [], chosenId: null })).toBe('')
  })
})
