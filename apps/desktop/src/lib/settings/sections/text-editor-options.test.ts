/**
 * The list rules behind the "Edit files in" row.
 *
 * Three things worth pinning: the system default is always the first row (it's
 * what F4 does for anyone who never touches this), the other editors read in the
 * app's language rather than in whatever order LaunchServices answered, and a
 * chosen app that's gone reads as the system default without anything written.
 */

import { describe, it, expect } from 'vitest'
import type { TextEditorApp, TextEditorList } from '$lib/ipc/bindings'
import { CHOOSE_APP_VALUE } from './app-choice-options'
import { selectedTextEditorId, textEditorItems, type TextEditorRowLabels } from './text-editor-options'

function app(id: string, displayName: string, icon: string | null = null): TextEditorApp {
  return { id, displayName, icon }
}

function list(overrides: Partial<TextEditorList> = {}): TextEditorList {
  return {
    defaultAppName: 'TextEdit',
    defaultAppIcon: 'data:image/webp;base64,dGV4dA==',
    apps: [],
    chosenId: 'system',
    ...overrides,
  }
}

const LABELS: TextEditorRowLabels = {
  systemDefault: (appName) => (appName === null ? 'System default' : `System default (${appName})`),
  chooseApp: 'Choose an app…',
  locale: 'en',
}

const SUBLIME = app('com.sublimetext.4', 'Sublime Text', 'data:image/webp;base64,c3Vi')
const XCODE = app('com.apple.dt.Xcode', 'Xcode')

describe('textEditorItems', () => {
  it('puts the system default first, named after the app macOS resolved, with its icon', () => {
    const items = textEditorItems(list({ apps: [XCODE, SUBLIME] }), LABELS)
    expect(items[0]).toEqual({
      value: 'system',
      label: 'System default (TextEdit)',
      iconUrl: 'data:image/webp;base64,dGV4dA==',
    })
  })

  it('uses the unnamed label when nothing on this Mac claims plain text', () => {
    const items = textEditorItems(list({ defaultAppName: null, defaultAppIcon: null }), LABELS)
    expect(items[0]).toEqual({ value: 'system', label: 'System default' })
  })

  it('sorts the other editors by name in the app language, not in byte order', () => {
    // Byte order puts "Zed" before "Ölmaker"; German collation reads Ö as O.
    const items = textEditorItems(list({ apps: [app('dev.zed.Zed', 'Zed'), app('com.example.oel', 'Ölmaker')] }), {
      ...LABELS,
      locale: 'de',
    })
    expect(items.map((item) => item.label)).toEqual(['System default (TextEdit)', 'Ölmaker', 'Zed', 'Choose an app…'])
  })

  it('offers each editor under the id the backend will be handed back, with its icon', () => {
    const picked = app('/Applications/Nova.app', 'Nova')
    const items = textEditorItems(list({ apps: [SUBLIME, picked] }), LABELS)
    expect(items.slice(1, 3)).toEqual([
      { value: '/Applications/Nova.app', label: 'Nova' },
      { value: 'com.sublimetext.4', label: 'Sublime Text', iconUrl: 'data:image/webp;base64,c3Vi' },
    ])
  })

  it('puts the app picker last, even when TextEdit is the only editor', () => {
    expect(textEditorItems(list(), LABELS)).toEqual([
      { value: 'system', label: 'System default (TextEdit)', iconUrl: 'data:image/webp;base64,dGV4dA==' },
      { value: CHOOSE_APP_VALUE, label: 'Choose an app…' },
    ])
  })
})

describe('selectedTextEditorId', () => {
  it('selects the chosen editor', () => {
    expect(selectedTextEditorId(list({ apps: [SUBLIME], chosenId: 'com.sublimetext.4' }))).toBe('com.sublimetext.4')
  })

  it('selects the system default row while nothing else is chosen', () => {
    expect(selectedTextEditorId(list({ apps: [SUBLIME] }))).toBe('system')
  })

  it('shows the system default once the chosen editor is gone', () => {
    // `chosenId: null` is how the backend says the stored app isn't on this Mac.
    // F4 falls back to the system default then, so the row says the same thing.
    expect(selectedTextEditorId(list({ apps: [SUBLIME], chosenId: null }))).toBe('system')
  })
})
