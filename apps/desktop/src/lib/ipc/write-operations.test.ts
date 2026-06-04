/**
 * IPC contract tests for the destructive write commands: `copy_files`, `move_files`,
 * `delete_files`, `trash_files`.
 *
 * These verify the **boundary**: that the typed bindings send the right snake_case
 * command name, with the camelCase payload shape the Rust signatures expect, including
 * the optional config object (which contains `progressIntervalMs`, `conflictResolution`,
 * `dryRun`, etc.). The business logic (actually copying bytes) lives in `*_core`
 * helpers and is owned by the Rust unit tests.
 *
 * Error paths use `WriteOperationError` variants (`source_not_found`, `destination_full`,
 * etc.) so the test pins the **shape** of the typed-error discriminator that the FE
 * branches on. The `LoadingDialog` / `ConflictResolution` UI relies on this shape.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import type { WriteOperationStartResult } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

const happyResult: WriteOperationStartResult = {
  operationId: 'op-1',
  operationType: 'copy',
}

describe('commands.copyFiles', () => {
  it('invokes copy_files with snake_case payload keys (sources, destination, config)', async () => {
    const ipc = installIpcMock()
    ipc.mock('copy_files', () => happyResult)

    const sources = ['/a/foo.txt', '/a/bar.txt']
    const destination = '/b'
    const config = {
      progressIntervalMs: 250,
      conflictResolution: 'rename' as const,
      dryRun: false,
      maxConflictsToShow: 50,
    }

    const result = await commands.copyFiles(sources, destination, config)

    expect(result).toEqual({ status: 'ok', data: happyResult })
    expect(ipc.calls).toHaveLength(1)
    expect(ipc.calls[0]).toEqual({
      command: 'copy_files',
      payload: { sources, destination, config },
    })
  })

  it('passes null config through as null (not omitted)', async () => {
    const ipc = installIpcMock()
    ipc.mock('copy_files', () => ({ ...happyResult, operationType: 'copy' as const }))

    await commands.copyFiles(['/a'], '/b', null)

    expect(ipc.lastCall('copy_files')?.payload).toEqual({
      sources: ['/a'],
      destination: '/b',
      config: null,
    })
  })

  it('surfaces a WriteOperationError variant on the error branch', async () => {
    const ipc = installIpcMock()
    ipc.mock('copy_files', () => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- mockIPC requires throwing the raw typed-error shape to test the wire contract
      throw { type: 'source_not_found', path: '/a/missing.txt' }
    })

    const result = await commands.copyFiles(['/a/missing.txt'], '/b', null)

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      expect(result.error).toEqual({ type: 'source_not_found', path: '/a/missing.txt' })
    }
  })
})

describe('commands.moveFiles', () => {
  it('invokes move_files with the same payload shape as copy_files', async () => {
    const ipc = installIpcMock()
    ipc.mock('move_files', () => ({ operationId: 'op-2', operationType: 'move' as const }))

    await commands.moveFiles(['/a/x'], '/b', null)

    expect(ipc.lastCall('move_files')?.payload).toEqual({
      sources: ['/a/x'],
      destination: '/b',
      config: null,
    })
  })
})

describe('commands.deleteFiles', () => {
  it('passes volumeId (snake_case wire key: volume_id is camelCased on the FE) through correctly', async () => {
    const ipc = installIpcMock()
    ipc.mock('delete_files', () => ({ operationId: 'op-3', operationType: 'delete' as const }))

    const volumeId = 'smb://server/share'
    await commands.deleteFiles(['/x', '/y'], volumeId, { dryRun: true })

    // Note the camelCase `volumeId` payload key. Tauri-Specta sends in camelCase and the
    // Rust side deserializes via the standard serde camelCase rename on the IPC layer.
    expect(ipc.lastCall('delete_files')?.payload).toEqual({
      sources: ['/x', '/y'],
      volumeId,
      config: { dryRun: true },
    })
  })

  it('treats a null volumeId as "use std::fs" (no volume coercion)', async () => {
    const ipc = installIpcMock()
    ipc.mock('delete_files', () => ({ operationId: 'op-4', operationType: 'delete' as const }))

    await commands.deleteFiles(['/x'], null, null)

    expect(ipc.lastCall('delete_files')?.payload).toEqual({
      sources: ['/x'],
      volumeId: null,
      config: null,
    })
  })
})

describe('commands.trashFiles', () => {
  it('forwards optional itemSizes array for accurate progress reporting', async () => {
    const ipc = installIpcMock()
    ipc.mock('trash_files', () => ({ operationId: 'op-5', operationType: 'trash' as const }))

    await commands.trashFiles(['/x', '/y'], [123, 456], null)

    expect(ipc.lastCall('trash_files')?.payload).toEqual({
      sources: ['/x', '/y'],
      itemSizes: [123, 456],
      config: null,
    })
  })

  it('accepts a null itemSizes for the "scan during op" path', async () => {
    const ipc = installIpcMock()
    ipc.mock('trash_files', () => ({ operationId: 'op-6', operationType: 'trash' as const }))

    await commands.trashFiles(['/x'], null, null)

    const payload = ipc.lastCall('trash_files')?.payload as Record<string, unknown> | undefined
    expect(payload?.itemSizes).toBeNull()
  })
})

describe('commands.cancelWriteOperation', () => {
  it('forwards both operationId and rollback flag as positional args', async () => {
    const ipc = installIpcMock()
    ipc.mock('cancel_write_operation', () => undefined)

    const rollback = true
    await commands.cancelWriteOperation('op-7', rollback)

    expect(ipc.lastCall('cancel_write_operation')?.payload).toEqual({
      operationId: 'op-7',
      rollback,
    })
  })
})

describe('commands.scanVolumeForConflicts', () => {
  // Five positional args, the last two added to let the backend resolve real
  // per-item types/sizes from the source volume. Pin the ordering so a future
  // arg swap (sourceVolumeId ↔ sourcePaths, or either landing in destPath)
  // fails loudly rather than silently degrading the dir-merge classification.
  it('sends all five args in order with the source-resolution pair populated', async () => {
    const ipc = installIpcMock()
    ipc.mock('scan_volume_for_conflicts', () => [])

    const volumeId = 'ext'
    const sourceItems = [{ name: 'photos', size: 0, modified: null, isDirectory: false }]
    const destPath = '/dest'
    const sourceVolumeId = 'root'
    const sourcePaths = ['/Users/test/photos']

    await commands.scanVolumeForConflicts(volumeId, sourceItems, destPath, sourceVolumeId, sourcePaths)

    expect(ipc.lastCall('scan_volume_for_conflicts')?.payload).toEqual({
      volumeId,
      sourceItems,
      destPath,
      sourceVolumeId,
      sourcePaths,
    })
  })

  it('passes the source-resolution pair as null when omitted (back-compat)', async () => {
    const ipc = installIpcMock()
    ipc.mock('scan_volume_for_conflicts', () => [])

    await commands.scanVolumeForConflicts('ext', [], '/dest', null, null)

    expect(ipc.lastCall('scan_volume_for_conflicts')?.payload).toEqual({
      volumeId: 'ext',
      sourceItems: [],
      destPath: '/dest',
      sourceVolumeId: null,
      sourcePaths: null,
    })
  })
})
