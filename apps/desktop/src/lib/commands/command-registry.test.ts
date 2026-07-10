import { describe, it, expect } from 'vitest'
import {
  commands,
  getPaletteCommands,
  updateLicenseCommandName,
  NATIVE_SHORTCUT_COMMAND_IDS,
  FIXED_KEY_COMMAND_IDS,
} from './command-registry'
import { COMMAND_IDS, isCommandId, type CommandId } from './command-ids'
import type { CommandArgs, CommandDispatchArgs } from './types'
import { DISPATCH_EXEMPT_IDS } from '../../routes/(main)/command-handlers/types'

/**
 * The exact set of commands the palette shows the user. Pinned so a new
 * registry entry can't silently appear in (or vanish from) the palette: the MCP
 * per-pane commands and every low-level navigation id are `showInPalette: false`
 * on purpose, and a regression that flips one visible would otherwise slip
 * through unnoticed. Update this list only when intentionally changing what the
 * user can pick from the palette.
 */
const EXPECTED_PALETTE_IDS: readonly CommandId[] = [
  'app.about',
  'app.licenseKey',
  'app.settings',
  'app.checkForUpdates',
  'cmdr.openOnboarding',
  'help.openShortcuts',
  'queue.show',
  'help.sendErrorReport',
  'help.whatsNew',
  'feedback.send',
  'log.operationLog',
  'search.open',
  'nav.goToPath',
  'favorites.add',
  'downloads.goToLatest',
  'view.showHidden',
  'view.briefMode',
  'view.fullMode',
  'view.zoom.set75',
  'view.zoom.set100',
  'view.zoom.set125',
  'view.zoom.set150',
  'view.zoom.in',
  'view.zoom.out',
  'sort.byName',
  'sort.byExtension',
  'sort.byModified',
  'sort.bySize',
  'sort.byCreated',
  'sort.ascending',
  'sort.descending',
  'sort.toggleOrder',
  'pane.switch',
  'pane.swap',
  'pane.leftVolumeChooser',
  'pane.rightVolumeChooser',
  'pane.copyPathLeftToRight',
  'pane.copyPathRightToLeft',
  'tab.new',
  'tab.close',
  'tab.reopen',
  'tab.next',
  'tab.prev',
  'tab.togglePin',
  'tab.closeOthers',
  'nav.open',
  'nav.parent',
  'nav.home',
  'nav.end',
  'nav.pageUp',
  'nav.pageDown',
  'nav.back',
  'nav.forward',
  'file.rename',
  'file.view',
  'file.edit',
  'file.copy',
  'file.move',
  'file.compress',
  'edit.copy',
  'edit.cut',
  'edit.paste',
  'edit.pasteAsMove',
  'file.newFolder',
  'file.newFile',
  'file.delete',
  'file.deletePermanently',
  'file.showInFinder',
  'file.copyPath',
  'file.copyCurrentDirectoryPath',
  'file.copyFilename',
  'file.contextMenu',
  'selection.toggle',
  'selection.toggleAndDown',
  'selection.selectAll',
  'selection.deselectAll',
  'selection.selectFiles',
  'selection.deselectFiles',
  'network.refresh',
  'share.back',
  'share.selectShare',
  'about.openWebsite',
  'about.openUpgrade',
  'about.close',
]

/**
 * The MCP-only per-pane commands (and `nav.openUnderCursor`). These exist so the
 * MCP adapter can target a specific pane / tab / option that the focused-pane
 * palette commands can't express. They MUST stay out of the palette.
 */
const MCP_ONLY_HIDDEN_IDS: readonly CommandId[] = [
  'view.setMode',
  'sort.set',
  'selection.mcpSelect',
  'cursor.moveTo',
  'cursor.scrollTo',
  'volume.selectByName',
  'tab.mcpAction',
  'dialog.confirm',
  'pane.refresh',
  'nav.openUnderCursor',
]

describe('command-registry id sync', () => {
  it('COMMAND_IDS and the registry ids are the same set (both directions)', () => {
    const tupleIds = new Set<string>(COMMAND_IDS)
    const registryIds = new Set<string>(commands.map((c) => c.id))

    // tuple ⊇ registry is also enforced at compile time (`Command.id: CommandId`),
    // but assert it here too so a failure names the offending id instead of a wall
    // of TS errors. registry ⊇ tuple has NO compile-time guard — this is its only one.
    const inTupleNotRegistry = [...tupleIds].filter((id) => !registryIds.has(id))
    const inRegistryNotTuple = [...registryIds].filter((id) => !tupleIds.has(id))

    expect(inTupleNotRegistry, 'ids in COMMAND_IDS with no registry entry').toEqual([])
    expect(inRegistryNotTuple, 'registry ids missing from COMMAND_IDS').toEqual([])
  })

  it('has no duplicate ids in the tuple', () => {
    expect(COMMAND_IDS.length).toBe(new Set(COMMAND_IDS).size)
  })
})

describe('nativeShortcut flag', () => {
  it('the source list names the four macOS-native commands', () => {
    // Sanity guard against a silently-failing import: the set-equality below
    // would falsely pass if `NATIVE_SHORTCUT_COMMAND_IDS` resolved to undefined.
    expect([...NATIVE_SHORTCUT_COMMAND_IDS].sort()).toEqual(
      ['app.hide', 'app.hideOthers', 'app.quit', 'app.showAll'].sort(),
    )
  })

  it('marks exactly the commands in NATIVE_SHORTCUT_COMMAND_IDS', () => {
    // The `nativeShortcut` registry flag and `NATIVE_SHORTCUT_COMMAND_IDS` are
    // two views of the same set: macOS owns both the behavior AND the accelerator
    // (PredefinedMenuItems), so the editor must render them read-only and the
    // store must refuse to rebind them. Keying the flag off the list keeps the
    // sites from drifting.
    const flagged = new Set(commands.filter((c) => c.nativeShortcut).map((c) => c.id))
    const expected = new Set<string>(NATIVE_SHORTCUT_COMMAND_IDS)
    expect(flagged).toEqual(expected)
  })

  it('every native command is also dispatch-exempt (Family 1)', () => {
    // The native commands are handler-less by design (AppKit runs them). The
    // dispatch-exempt list sources Family 1 from the same registry list, so this
    // also proves that single-source wiring round-trips.
    const exempt = new Set<string>(DISPATCH_EXEMPT_IDS)
    for (const id of NATIVE_SHORTCUT_COMMAND_IDS) {
      expect(exempt.has(id), `${id} must be dispatch-exempt`).toBe(true)
    }
  })

  it('only ever uses `true` for the flag (never `false`/`undefined` noise)', () => {
    for (const cmd of commands) {
      if (cmd.nativeShortcut !== undefined) {
        expect(cmd.nativeShortcut).toBe(true)
      }
    }
  })
})

describe('palette-visible command set', () => {
  it('shows exactly the pinned set of commands (no silent additions or removals)', () => {
    const paletteIds = getPaletteCommands().map((c) => c.id)
    expect(paletteIds).toEqual(EXPECTED_PALETTE_IDS)
  })

  it('keeps the MCP-only per-pane commands out of the palette', () => {
    const paletteIds = new Set(getPaletteCommands().map((c) => c.id))
    for (const id of MCP_ONLY_HIDDEN_IDS) {
      expect(paletteIds.has(id), `${id} must stay showInPalette: false`).toBe(false)
      // The id must still be a real registry entry, just a hidden one.
      expect(
        commands.some((c) => c.id === id),
        `${id} must exist in the registry`,
      ).toBe(true)
    }
  })
})

describe('isCommandId', () => {
  it('accepts every registry id', () => {
    for (const command of commands) {
      expect(isCommandId(command.id)).toBe(true)
    }
  })

  it('rejects an unknown id', () => {
    expect(isCommandId('file.doesNotExist')).toBe(false)
    expect(isCommandId('')).toBe(false)
  })

  it('narrows the type for downstream use', () => {
    const raw = 'file.rename' as string
    if (isCommandId(raw)) {
      // Compiles only because `raw` is now `CommandId`; a no-op assignment proves it.
      const narrowed: CommandId = raw
      expect(narrowed).toBe('file.rename')
    } else {
      throw new Error('expected file.rename to be a CommandId')
    }
  })
})

describe('CommandId is a closed union (compile-time)', () => {
  // These assertions are enforced by `tsc` / `svelte-check`, not at runtime.
  // The runtime `expect`s only let Vitest execute the block.
  it('accepts a real id and rejects a bogus one', () => {
    const real: CommandId = 'file.rename'

    // @ts-expect-error -- 'file.doesNotExist' is not a member of the CommandId union.
    const bogus: CommandId = 'file.doesNotExist'

    expect(real).toBe('file.rename')
    expect(bogus).toBe('file.doesNotExist')
  })

  it('arg-less ids resolve to an empty dispatch tuple', () => {
    // An arg-less command's dispatch-args tuple is `[]`.
    const noArgs: CommandDispatchArgs<'file.view'> = []
    // The arg map entry for an arg-less command is the `NoCommandArgs` marker.
    const argValue: CommandArgs['file.view'] = undefined

    expect(noArgs).toEqual([])
    expect(argValue).toBeUndefined()
  })

  it('arg-carrying ids resolve to a single-payload dispatch tuple', () => {
    // `view.setMode` overrides its `CommandArgs` entry with `{ pane, mode, fromMenu }`,
    // so its dispatch tuple is `[args]` (one required payload), not `[]`.
    const withArgs: CommandDispatchArgs<'view.setMode'> = [{ pane: 'left', mode: 'full', fromMenu: true }]
    const argValue: CommandArgs['view.setMode'] = { pane: 'right', mode: 'brief', fromMenu: false }

    // @ts-expect-error -- an arg-carrying id can't be dispatched with no payload.
    const missing: CommandDispatchArgs<'view.setMode'> = []
    void missing

    expect(withArgs[0]).toEqual({ pane: 'left', mode: 'full', fromMenu: true })
    expect(argValue.pane).toBe('right')
  })

  it('optional-payload ids accept both an arg-less and an arg-carrying dispatch', () => {
    // `file.copy` is dispatched arg-less from the F-bar / palette and with a
    // payload from the MCP `copy` tool, so its tuple is `[args?]`.
    const noArgs: CommandDispatchArgs<'file.copy'> = []
    const withArgs: CommandDispatchArgs<'file.copy'> = [{ autoConfirm: true, onConflict: 'overwrite_all' }]

    expect(noArgs).toEqual([])
    expect(withArgs[0]?.autoConfirm).toBe(true)
  })
})

describe('updateLicenseCommandName', () => {
  it('mutates the license command name in place (the registry stays mutable)', () => {
    updateLicenseCommandName(false)
    expect(commands.find((c) => c.id === 'app.licenseKey')?.name).toBe('Enter license key')

    updateLicenseCommandName(true)
    expect(commands.find((c) => c.id === 'app.licenseKey')?.name).toBe('See license details')
  })
})

describe('fixedKey flag', () => {
  it('marks exactly the commands in FIXED_KEY_COMMAND_IDS', () => {
    // The `fixedKey` registry flag and `FIXED_KEY_COMMAND_IDS` are two views of
    // the same set: the key is hardcoded in the owning component's keydown
    // handler, so the editor must render these read-only and the store must
    // refuse to rebind them. Keying the flag off the list keeps the sites from
    // drifting.
    const flagged = new Set(commands.filter((c) => c.fixedKey).map((c) => c.id))
    const expected = new Set<string>(FIXED_KEY_COMMAND_IDS)
    expect(flagged).toEqual(expected)
  })

  it('every fixed-key command is also dispatch-exempt (Families 2/3)', () => {
    // Fixed-key commands are handler-less by design (their component runs them).
    // The dispatch-exempt list sources Families 2/3 from the same registry list,
    // so this proves the single-source wiring round-trips.
    const exempt = new Set<string>(DISPATCH_EXEMPT_IDS)
    for (const id of FIXED_KEY_COMMAND_IDS) {
      expect(exempt.has(id), `${id} must be dispatch-exempt`).toBe(true)
    }
  })

  it('never overlaps the native set (a command is OS-owned or component-owned, not both)', () => {
    const native = new Set<string>(NATIVE_SHORTCUT_COMMAND_IDS)
    for (const id of FIXED_KEY_COMMAND_IDS) {
      expect(native.has(id), `${id} must not also be native`).toBe(false)
    }
  })

  it('only ever uses `true` for the flag (never `false`/`undefined` noise)', () => {
    for (const cmd of commands) {
      if (cmd.fixedKey !== undefined) {
        expect(cmd.fixedKey).toBe(true)
      }
    }
  })
})

describe('operation-log shortcut binding', () => {
  // M7: the alpha "Operation log" command opens the dialog from the View menu and
  // via a configurable default shortcut. The plan's first pick (⌥⌘O) is taken by
  // `file.showInFinder`, so the default is ⌘⌥L. Guard against a silent double-bind.
  it('binds log.operationLog to exactly ⌘⌥L', () => {
    const cmd = commands.find((c) => c.id === 'log.operationLog')
    expect(cmd, 'log.operationLog must be registered').toBeDefined()
    expect(cmd?.shortcuts).toEqual(['⌘⌥L'])
  })

  it('does not double-bind ⌘⌥L (its default) or ⌥⌘O (Show in Finder, the plan pick it avoided)', () => {
    const claimants = (shortcut: string): CommandId[] =>
      commands.filter((c) => c.shortcuts.includes(shortcut)).map((c) => c.id)

    expect(claimants('⌘⌥L')).toEqual(['log.operationLog'])
    expect(claimants('⌥⌘O')).toEqual(['file.showInFinder'])
  })
})
