/**
 * Unit tests for the operation-log IPC wrappers: they forward to the typed
 * `commands.*` bindings and unwrap the `Result<T, string>` shape (ok → data,
 * error → throw). The dialog relies on the throw so a read failure surfaces as a
 * caught error, not a silent empty result.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

/** The tauri-specta `Result<T, E>` wire shape the bindings return. */
type Res<T> = { status: 'ok'; data: T } | { status: 'error'; error: string }

const getRecentMock = vi.fn<(limit: number, offset: number) => Promise<Res<unknown>>>()
const getDetailMock = vi.fn<(id: string, l: number, o: number) => Promise<Res<unknown>>>()
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getRecentOperationLogEntries: (limit: number, offset: number) => getRecentMock(limit, offset),
    getOperationLogDetail: (id: string, l: number, o: number) => getDetailMock(id, l, o),
  },
}))

import { getRecentOperationLogEntries, getOperationLogDetail } from './operation-log'

describe('getRecentOperationLogEntries', () => {
  beforeEach(() => getRecentMock.mockReset())

  it('forwards limit/offset and returns the data on ok', async () => {
    getRecentMock.mockResolvedValue({ status: 'ok', data: [{ opId: 'a' }] })
    const rows = await getRecentOperationLogEntries(50, 100)
    expect(getRecentMock).toHaveBeenCalledWith(50, 100)
    expect(rows).toEqual([{ opId: 'a' }])
  })

  it('throws on an error result', async () => {
    getRecentMock.mockResolvedValue({ status: 'error', error: 'db locked' })
    await expect(getRecentOperationLogEntries(50, 0)).rejects.toThrow('db locked')
  })
})

describe('getOperationLogDetail', () => {
  beforeEach(() => getDetailMock.mockReset())

  it('returns the detail on ok (and null when the op is absent)', async () => {
    getDetailMock.mockResolvedValue({ status: 'ok', data: null })
    expect(await getOperationLogDetail('op-1', 200, 0)).toBeNull()
    expect(getDetailMock).toHaveBeenCalledWith('op-1', 200, 0)

    const detail = { operation: { opId: 'op-1' }, items: [], totalItems: 0 }
    getDetailMock.mockResolvedValue({ status: 'ok', data: detail })
    expect(await getOperationLogDetail('op-1', 200, 0)).toEqual(detail)
  })

  it('throws on an error result', async () => {
    getDetailMock.mockResolvedValue({ status: 'error', error: 'gone' })
    await expect(getOperationLogDetail('op-1', 200, 0)).rejects.toThrow('gone')
  })
})
