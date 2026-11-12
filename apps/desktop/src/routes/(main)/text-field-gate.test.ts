/**
 * A command that moves or removes files never runs from a key while a text field has focus, on any
 * OS: every one of `FILE_MOVING_COMMAND_IDS`, every one of its default keys, each OS's keymap. The
 * keyboard road leaves the key to the field, and the dispatch core refuses the menu road (a bar
 * accelerator fires before the field sees the key).
 *
 * Runs against the REAL registry and shortcut store, so a default key added later is walked too.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { FILE_MOVING_COMMAND_IDS, type CommandId } from '#lib/commands/index.js'
import type { Platform } from '#lib/platform/index.js'
import { withCapabilitiesPossibleOn, withPlatform } from '#lib/platform/test-platform.js'
import { formatKeyCombo } from '#lib/shortcuts/key-capture.js'
import { destroyShortcutDispatch, initShortcutDispatch } from '#lib/shortcuts/shortcut-dispatch.js'
import { getDefaultShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import type { DialogsOnScreen, DispatchSource } from './command-dispatch-context'
import { resolveGlobalKeyAction } from './global-keydown'
import { isRefusedInTextField } from './text-field-gate'

const NOTHING_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: false }

const ARROWS: Record<string, string> = { '←': 'ArrowLeft', '→': 'ArrowRight', '↑': 'ArrowUp', '↓': 'ArrowDown' }

/** The keydown a canonical combo (`⌘⌥Backspace`, `Shift+F8`) is, on `platform`. */
function keydownFor(combo: string, platform: Platform): KeyboardEvent {
  const init: KeyboardEventInit = {}
  let rest = combo
  if (platform === 'macos') {
    const glyphs: Record<string, keyof KeyboardEventInit> = {
      '⌘': 'metaKey',
      '⌃': 'ctrlKey',
      '⌥': 'altKey',
      '⇧': 'shiftKey',
    }
    while (rest.length > 1 && rest[0] in glyphs) {
      ;(init as Record<string, boolean>)[glyphs[rest[0]]] = true
      rest = rest.slice(1)
    }
  } else {
    const tokens: Record<string, keyof KeyboardEventInit> = {
      'Ctrl+': 'ctrlKey',
      'Alt+': 'altKey',
      'Shift+': 'shiftKey',
      'Super+': 'metaKey',
    }
    for (let found = true; found;) {
      found = false
      for (const [token, flag] of Object.entries(tokens)) {
        if (rest.startsWith(token) && rest.length > token.length) {
          ;(init as Record<string, boolean>)[flag] = true
          rest = rest.slice(token.length)
          found = true
        }
      }
    }
  }
  const isLetter = /^[A-Za-z]$/.test(rest)
  const key = ARROWS[rest] ?? (isLetter ? (init.shiftKey ? rest.toUpperCase() : rest.toLowerCase()) : rest)
  const code = isLetter ? `Key${rest.toUpperCase()}` : undefined
  return new KeyboardEvent('keydown', { ...init, key, code })
}

describe.each(['macos', 'windows', 'linux'] as const)('on %s, with a text field focused', (platform) => {
  const undos: (() => void)[] = []
  let input: HTMLInputElement

  beforeEach(() => {
    undos.push(withPlatform(platform), withCapabilitiesPossibleOn(platform))
    initShortcutDispatch()
    input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()
  })

  afterEach(() => {
    input.remove()
    destroyShortcutDispatch()
    for (const undo of undos.splice(0).reverse()) undo()
  })

  const cases = (): [CommandId, string][] =>
    FILE_MOVING_COMMAND_IDS.flatMap((commandId) =>
      getDefaultShortcuts(commandId).map((combo): [CommandId, string] => [commandId, combo]),
    )

  it('walks at least one key (the registry still binds these commands)', () => {
    expect(cases().length).toBeGreaterThan(0)
  })

  it('leaves every default key of every file-moving command to the field', () => {
    const ran: string[] = []
    for (const [commandId, combo] of cases()) {
      const event = keydownFor(combo, platform)
      // The helper must build the key the combo names, or the walk proves nothing.
      expect(formatKeyCombo(event), `${combo} as a keydown`).toBe(combo)
      const action = resolveGlobalKeyAction(event, NOTHING_OPEN)
      if (action.kind === 'dispatch') ran.push(`${combo} ran ${action.commandId} (a default of ${commandId})`)
    }
    expect(ran).toEqual([])
  })

  // The quick filter is a drawn badge over the file list, ❌ not a DOM text field, so Total
  // Commander's "filter, then F5 / F6 / F8" stays live: focus sits on the list, never an input.
  it('leaves F5, F6, and F8 live with focus on the file list (where the quick filter types)', () => {
    input.blur()
    const list = document.createElement('div')
    list.tabIndex = 0
    document.body.appendChild(list)
    list.focus()
    const shift = platform === 'macos' ? '⇧' : 'Shift+'
    for (const [combo, commandId] of [
      ['F5', 'file.copy'],
      ['F6', 'file.move'],
      ['F8', 'file.delete'],
      [`${shift}F8`, 'file.deletePermanently'],
    ] as const) {
      expect(resolveGlobalKeyAction(keydownFor(combo, platform), NOTHING_OPEN), combo).toEqual({
        kind: 'dispatch',
        commandId,
      })
    }
    list.remove()
  })

  it('refuses every file-moving command from the keyboard and the menu bar, and only those roads', () => {
    for (const commandId of FILE_MOVING_COMMAND_IDS) {
      for (const source of ['keyboard', 'menu'] as const satisfies readonly DispatchSource[]) {
        expect(isRefusedInTextField(commandId, source, true), `${commandId} from ${source}`).toBe(true)
      }
      for (const source of ['palette', 'explorer', 'mcp', 'mouse'] as const satisfies readonly DispatchSource[]) {
        expect(isRefusedInTextField(commandId, source, true), `${commandId} from ${source}`).toBe(false)
      }
      expect(isRefusedInTextField(commandId, 'keyboard', false)).toBe(false)
    }
    expect(isRefusedInTextField('file.copy', 'keyboard', true)).toBe(false)
  })
})
