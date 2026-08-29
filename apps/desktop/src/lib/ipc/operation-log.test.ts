/**
 * IPC contract test for `rollback_operation`, the destructive operation-log command:
 * pressing Roll back in the history dialog moves the user's files, and undoing a
 * copy DELETES what that copy wrote. So the boundary is pinned here — the snake_case
 * command name, the single `operationId` payload key, and the shape of the typed
 * `RollbackRefusal` the dialog branches on to pick a sentence.
 *
 * The reversal itself (what an inverse does, the snapshot recheck, the skip reasons)
 * belongs to the Rust engine and its unit tests; nothing about it is asserted here.
 *
 * The read commands next door (`get_recent_operation_log_entries`,
 * `get_operation_log_detail`) are thin getters with ≤ 2 args, which
 * `apps/desktop/src/lib/ipc/DETAILS.md` says to leave alone. `undo_operations` sends
 * one array and is covered where its ORDER matters, in
 * `$lib/tauri-commands/operation-log.test.ts`.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import type { RollbackDispatch } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

describe('commands.rollbackOperation', () => {
  it('invokes rollback_operation with the operationId payload key', async () => {
    const ipc = installIpcMock()
    const dispatch: RollbackDispatch = { inverseOpId: 'op-inverse-1' }
    ipc.mock('rollback_operation', () => dispatch)

    const result = await commands.rollbackOperation('op-1')

    expect(result).toEqual({ status: 'ok', data: dispatch })
    expect(ipc.calls).toEqual([{ command: 'rollback_operation', payload: { operationId: 'op-1' } }])
  })

  it('reaches IPC exactly once per press, so a confirmed rollback never double-dispatches', async () => {
    const ipc = installIpcMock()
    ipc.mock('rollback_operation', () => ({ inverseOpId: 'op-inverse-1' }))

    await commands.rollbackOperation('op-1')

    expect(ipc.callCount('rollback_operation')).toBe(1)
  })

  it('surfaces a unit-variant refusal with its `kind` discriminator intact', async () => {
    const ipc = installIpcMock()
    ipc.mock('rollback_operation', () => {
      throw { kind: 'alreadyRollingBack' }
    })

    const result = await commands.rollbackOperation('op-1')

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      // The dialog switches on `kind`; a flattened string here would leave it
      // guessing which of five reasons to word.
      expect(result.error).toEqual({ kind: 'alreadyRollingBack' })
    }
  })

  it('surfaces a data-carrying refusal with its camelCase `detail` payload', async () => {
    const ipc = installIpcMock()
    ipc.mock('rollback_operation', () => {
      throw { kind: 'volumeUnavailable', detail: { volumeId: 'smb-nas-photos' } }
    })

    const result = await commands.rollbackOperation('op-1')

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      // Adjacently tagged (`tag = "kind"`, `content = "detail"`) with camelCase
      // fields: a serde attribute drifting off `RollbackRefusal` would land here.
      expect(result.error).toEqual({ kind: 'volumeUnavailable', detail: { volumeId: 'smb-nas-photos' } })
    }
  })
})
