/**
 * Keyboard reachability for the command registry: every command a person can
 * run has a default key, a palette row, or a written reason it needs neither.
 * A new command that has none of the three fails here, so the keyboard path
 * ships with the command instead of being found missing later (#383).
 *
 * Runs per platform, on what that OS can offer (`withCapabilitiesPossibleOn`): a command whose
 * capability the OS lacks isn't there at all (hidden from the palette, cfg-gated out of the
 * native menus), so it's absent, not mouse-only.
 */
import { afterAll, beforeAll, describe, it, expect } from 'vitest'
import { withCapabilitiesPossibleOn, withPlatform } from '#lib/platform/test-platform.js'
import { getDefaultShortcuts } from '#lib/shortcuts/shortcuts-store.js'
import { commandOffered, commands } from './command-registry'
import { mcpCommands } from './sources/mcp'
import { PLATFORMS } from './platform-shortcuts'
import type { CommandId } from './command-ids'
import type { Platform } from '#lib/platform/index.js'

const MCP_ONLY_REASON =
  'MCP-only: carries a per-pane payload for an MCP tool, and the focused-pane commands are the keyboard path.'

/** Commands with neither a default key nor a palette row, each with the reason that's fine. */
const KEYBOARD_PATH_NOT_NEEDED: Partial<Record<CommandId, string>> = {
  ...Object.fromEntries(mcpCommands.map((c) => [c.id, MCP_ONLY_REASON])),
  'view.setMode':
    "Per-pane twin of `view.briefMode` / `view.fullMode`, dispatched by a click on the inactive pane's View menu item so that pane changes without taking focus.",
  'sort.set': MCP_ONLY_REASON,
}

/** Per-OS additions to `KEYBOARD_PATH_NOT_NEEDED`, for a command that has a keyboard path on one OS only. */
const KEYBOARD_PATH_NOT_NEEDED_ON: Record<Platform, Partial<Record<CommandId, string>>> = {
  macos: {
    'app.showAll':
      'AppKit-native, unbound by default. It lives in the Cmdr app menu, which the menu bar keyboard focus (⌃F2) reaches.',
  },
  windows: {},
  linux: {},
}

const hasNoKeyboardPath = (c: (typeof commands)[number]): boolean =>
  getDefaultShortcuts(c.id).length === 0 && !c.showInPalette

describe.each(PLATFORMS)('keyboard reachability on %s', (platform) => {
  const undos: (() => void)[] = []
  const notNeeded = { ...KEYBOARD_PATH_NOT_NEEDED, ...KEYBOARD_PATH_NOT_NEEDED_ON[platform] }

  beforeAll(() => {
    undos.push(withPlatform(platform), withCapabilitiesPossibleOn(platform))
  })

  afterAll(() => {
    for (const undo of undos.reverse()) undo()
  })

  it('gives every command a default key or a palette row, or a reason it needs neither', () => {
    const unreachable = commands
      .filter(commandOffered)
      .filter(hasNoKeyboardPath)
      .map((c) => c.id)
      .filter((id) => !(id in notNeeded))
    expect(
      unreachable,
      'Give these a default shortcut, set `showInPalette: true`, or add a reason to KEYBOARD_PATH_NOT_NEEDED',
    ).toEqual([])
  })

  it('keeps the exemption list tight: every exempt command is on this OS and really has no key and no palette row', () => {
    const stale = commands
      .filter((c) => c.id in notNeeded && (!commandOffered(c) || !hasNoKeyboardPath(c)))
      .map((c) => c.id)
    expect(stale, 'Drop these from KEYBOARD_PATH_NOT_NEEDED (or move them to this OS’s list)').toEqual([])
  })
})
