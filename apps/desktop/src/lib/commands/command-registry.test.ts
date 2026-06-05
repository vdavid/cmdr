import { describe, it, expect } from 'vitest'
import { commands, updateLicenseCommandName } from './command-registry'
import { COMMAND_IDS, isCommandId, type CommandId } from './command-ids'
import type { CommandArgs, CommandDispatchArgs } from './types'

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
    const noArgs: CommandDispatchArgs<'file.rename'> = []
    // The arg map entry for an arg-less command is the `NoCommandArgs` marker.
    const argValue: CommandArgs['file.rename'] = undefined

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
