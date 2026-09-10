/**
 * The dialog gate against the REAL registry rules: whether a command may run while a dialog,
 * an explorer overlay, or the command palette is up, per road it came in by.
 *
 * The regression it pins is the one the gate exists for: a native-menu accelerator (⌘W, ⌘K)
 * used to reach its handler behind an open dialog, because only the keyboard road knew about
 * dialogs.
 */
import { describe, it, expect } from 'vitest'
import { isRefusedOverDialog } from './dialog-command-gate'
import { DISPATCH_SOURCES, type DialogsOnScreen, type DispatchSource } from './command-dispatch-context'
import type { CommandId } from '$lib/commands'

const NOTHING_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: false }
const DIALOG_OPEN: DialogsOnScreen = { dialogOpen: true, paletteOpen: false }
const PALETTE_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: true }

const USER_SOURCES = DISPATCH_SOURCES.filter((source) => source !== 'mcp')

function refused(
  commandId: CommandId,
  source: DispatchSource,
  onScreen: DialogsOnScreen,
  textInputFocused = false,
): boolean {
  return isRefusedOverDialog({ commandId, source, onScreen, textInputFocused })
}

describe('isRefusedOverDialog', () => {
  it('refuses nothing while nothing is open', () => {
    for (const source of DISPATCH_SOURCES) {
      expect(refused('tab.close', source, NOTHING_OPEN), source).toBe(false)
    }
  })

  it('refuses a pane command behind a dialog from every road a user drives', () => {
    for (const source of USER_SOURCES) {
      expect(refused('tab.close', source, DIALOG_OPEN), source).toBe(true)
      expect(refused('servers.connect', source, DIALOG_OPEN), source).toBe(true)
    }
  })

  it('lets MCP through, since its tools answer for themselves and some act on the dialog', () => {
    expect(refused('tab.close', 'mcp', DIALOG_OPEN)).toBe(false)
    expect(refused('dialog.confirm', 'mcp', DIALOG_OPEN)).toBe(false)
  })

  it('runs the commands that opted out, like Settings on ⌘,', () => {
    for (const source of USER_SOURCES) {
      expect(refused('app.settings', source, DIALOG_OPEN), source).toBe(false)
      expect(refused('queue.show', source, DIALOG_OPEN), source).toBe(false)
    }
  })

  it('runs the text-editing family only with focus in a text input', () => {
    expect(refused('edit.paste', 'menu', DIALOG_OPEN, true)).toBe(false)
    expect(refused('selection.selectAll', 'keyboard', DIALOG_OPEN, true)).toBe(false)
    expect(refused('edit.paste', 'menu', DIALOG_OPEN, false)).toBe(true)
    expect(refused('edit.copy', 'menu', DIALOG_OPEN, false)).toBe(true)
  })

  it('counts the palette as in the way for every road but its own rows', () => {
    expect(refused('tab.new', 'palette', PALETTE_OPEN)).toBe(false)
    expect(refused('tab.new', 'keyboard', PALETTE_OPEN)).toBe(true)
    expect(refused('tab.new', 'menu', PALETTE_OPEN)).toBe(true)
  })

  it('still refuses a palette row when a dialog is up besides the palette', () => {
    expect(refused('tab.new', 'palette', { dialogOpen: true, paletteOpen: true })).toBe(true)
  })
})
