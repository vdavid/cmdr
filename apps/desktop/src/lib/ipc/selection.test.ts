/**
 * IPC contract tests for the Selection dialog commands added in M5.
 *
 * Pins the wire shape (command names + payload field names) so a rename on the Rust
 * side won't silently break the selection dialog. The destructive `clear_recent_selections`
 * and cross-window `apply_recent_selections_max_count` are the priority per
 * `lib/ipc/CLAUDE.md` § "IPC contract testing"; `translate_selection_query` is included
 * because it carries two arguments (prompt + sampleNames) and a Result return type, so
 * any drift on the camelCase payload mapping would be silently broken at runtime.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import type { SelectionHistoryEntry, SelectionTranslateResult } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

const sampleResult: SelectionTranslateResult = {
  pattern: '*.log',
  kind: 'glob',
  sizeMin: null,
  sizeMax: null,
  modifiedAfter: null,
  modifiedBefore: null,
  caveat: null,
  label: 'Log files',
}

const sampleEntry: SelectionHistoryEntry = {
  id: 'abc-123',
  timestamp: 1_700_000_000_000,
  mode: 'filename',
  query: '*.log',
  filters: {
    sizeMin: null,
    sizeMax: null,
    modifiedAfter: null,
    modifiedBefore: null,
  },
  caseSensitive: false,
  matchCount: 42,
}

describe('commands.translateSelectionQuery', () => {
  it('forwards the prompt and sample list verbatim and unwraps the result', async () => {
    const ipc = installIpcMock()
    ipc.mock('translate_selection_query', () => sampleResult)

    const out = await commands.translateSelectionQuery('all log files', ['a.log', 'b.log', 'c.txt'])

    expect(out).toEqual({ status: 'ok', data: sampleResult })
    expect(ipc.lastCall('translate_selection_query')?.payload).toEqual({
      prompt: 'all log files',
      sampleNames: ['a.log', 'b.log', 'c.txt'],
    })
  })

  it("surfaces the backend error string when the cloud provider isn't configured", async () => {
    const ipc = installIpcMock()
    ipc.mock('translate_selection_query', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw wire shape to test the contract
      throw 'AI selection needs a cloud provider. Set one in Settings > AI.'
    })

    const out = await commands.translateSelectionQuery('logs', [])

    expect(out.status).toBe('error')
    if (out.status === 'error') {
      expect(out.error).toContain('cloud provider')
    }
  })
})

describe('commands.getRecentSelections', () => {
  it('passes through the optional limit', async () => {
    const ipc = installIpcMock()
    ipc.mock('get_recent_selections', () => [sampleEntry])

    const out = await commands.getRecentSelections(5)
    expect(out).toEqual([sampleEntry])
    expect(ipc.lastCall('get_recent_selections')?.payload).toEqual({ limit: 5 })
  })

  it('sends null when no limit is given', async () => {
    const ipc = installIpcMock()
    ipc.mock('get_recent_selections', () => [])

    await commands.getRecentSelections(null)
    expect(ipc.lastCall('get_recent_selections')?.payload).toEqual({ limit: null })
  })
})

describe('commands.addRecentSelection', () => {
  it('forwards the entry and cap', async () => {
    const ipc = installIpcMock()
    ipc.mock('add_recent_selection', () => null)

    await commands.addRecentSelection(sampleEntry, 500)
    const call = ipc.lastCall('add_recent_selection')
    expect(call?.payload).toEqual({ entry: sampleEntry, maxCount: 500 })
  })

  it('sends maxCount as null when caller wants the default', async () => {
    const ipc = installIpcMock()
    ipc.mock('add_recent_selection', () => null)

    await commands.addRecentSelection(sampleEntry, null)
    expect(ipc.lastCall('add_recent_selection')?.payload).toEqual({ entry: sampleEntry, maxCount: null })
  })
})

describe('commands.removeRecentSelection', () => {
  it('forwards the id', async () => {
    const ipc = installIpcMock()
    ipc.mock('remove_recent_selection', () => null)

    await commands.removeRecentSelection('abc-123')
    expect(ipc.lastCall('remove_recent_selection')?.payload).toEqual({ id: 'abc-123' })
  })
})

describe('commands.clearRecentSelections', () => {
  // Destructive: pinning the command name protects against a rename silently turning a
  // "clear all" intent into a no-op.
  it('sends no payload and surfaces success', async () => {
    const ipc = installIpcMock()
    ipc.mock('clear_recent_selections', () => null)

    const out = await commands.clearRecentSelections()
    expect(out).toEqual({ status: 'ok', data: null })
    expect(ipc.callCount('clear_recent_selections')).toBe(1)
    // Tauri specta wrappers send `{}` (or no payload) for no-arg commands; we just
    // assert the call happened with no extra fields.
    const call = ipc.lastCall('clear_recent_selections')
    expect(call?.payload ?? {}).toEqual({})
  })

  it('propagates a backend error so the UI can toast it', async () => {
    const ipc = installIpcMock()
    ipc.mock('clear_recent_selections', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw wire shape
      throw "Couldn't write selection history"
    })

    const out = await commands.clearRecentSelections()
    expect(out.status).toBe('error')
  })
})

describe('commands.applyRecentSelectionsMaxCount', () => {
  // Cross-window live-apply: the settings window writes the value, the main window's
  // backend trims the in-memory store. Pinning the camelCase argument name catches
  // drift between the Rust signature and the typed binding.
  it('forwards the cap', async () => {
    const ipc = installIpcMock()
    ipc.mock('apply_recent_selections_max_count', () => null)

    await commands.applyRecentSelectionsMaxCount(250)
    expect(ipc.lastCall('apply_recent_selections_max_count')?.payload).toEqual({ maxCount: 250 })
  })

  it('also accepts the zero (history-disabled) case', async () => {
    const ipc = installIpcMock()
    ipc.mock('apply_recent_selections_max_count', () => null)

    await commands.applyRecentSelectionsMaxCount(0)
    expect(ipc.lastCall('apply_recent_selections_max_count')?.payload).toEqual({ maxCount: 0 })
  })
})
