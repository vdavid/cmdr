/**
 * Keyboard reachability for the command registry: every command a person can
 * run has a default key, a palette row, or a written reason it needs neither.
 * A new command that has none of the three fails here, so the keyboard path
 * ships with the command instead of being found missing later (#383).
 *
 * Runs on macOS, with every capability a command asks for, because that's where every command
 * exists: the capability-gated ones (Quick Look, Get Info, tags, cloud actions) are
 * hidden from the palette elsewhere AND cfg-gated out of the native menus, so
 * off macOS they're absent, not mouse-only.
 */
import { describe, it, expect, vi } from 'vitest'
import { commands } from './command-registry'
import { mcpCommands } from './sources/mcp'
import type { CommandId } from './command-ids'

vi.hoisted(() => {
  vi.stubGlobal('__CMDR_PLATFORM__', 'macos')
  // Every capability a command asks for, as macOS answers them.
  vi.stubGlobal('__CMDR_CAPABILITIES__', {
    quickLook: true,
    getInfo: true,
    openTerminalHere: true,
    cloudFileActions: true,
    colorTags: true,
    cliCommand: true,
  })
})

const MCP_ONLY_REASON =
  'MCP-only: carries a per-pane payload for an MCP tool, and the focused-pane commands are the keyboard path.'

/** Commands with neither a default key nor a palette row, each with the reason that's fine. */
const KEYBOARD_PATH_NOT_NEEDED: Partial<Record<CommandId, string>> = {
  ...Object.fromEntries(mcpCommands.map((c) => [c.id, MCP_ONLY_REASON])),
  'app.showAll':
    'AppKit-native, unbound by default on macOS too. It lives in the Cmdr app menu, which the menu bar keyboard focus (⌃F2) reaches.',
  'view.setMode':
    "Per-pane twin of `view.briefMode` / `view.fullMode`, dispatched by a click on the inactive pane's View menu item so that pane changes without taking focus.",
  'sort.set': MCP_ONLY_REASON,
}

const hasNoKeyboardPath = (c: (typeof commands)[number]): boolean => c.shortcuts.length === 0 && !c.showInPalette

describe('keyboard reachability', () => {
  it('gives every command a default key or a palette row, or a reason it needs neither', () => {
    const unreachable = commands
      .filter(hasNoKeyboardPath)
      .map((c) => c.id)
      .filter((id) => !(id in KEYBOARD_PATH_NOT_NEEDED))
    expect(
      unreachable,
      'Give these a default shortcut, set `showInPalette: true`, or add a reason to KEYBOARD_PATH_NOT_NEEDED',
    ).toEqual([])
  })

  it('keeps the exemption list tight: every exempt command really has no key and no palette row', () => {
    const exemptButReachable = commands
      .filter((c) => c.id in KEYBOARD_PATH_NOT_NEEDED && !hasNoKeyboardPath(c))
      .map((c) => c.id)
    expect(exemptButReachable, 'Drop these from KEYBOARD_PATH_NOT_NEEDED').toEqual([])
  })
})
