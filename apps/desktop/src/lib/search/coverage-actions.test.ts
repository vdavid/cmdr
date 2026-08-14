/**
 * Pressing "Index this drive" has four outcomes, and each one owes the user a
 * different sentence. The one that matters most is `indexing_disabled`: the master
 * switch outranks every per-drive gate, so without its own answer the button would
 * quietly do nothing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { tString } from '$lib/intl/messages.svelte'
import { indexUncoveredDrive } from './coverage-actions'

const { enableDriveIndexMock, addToastMock } = vi.hoisted(() => ({
  enableDriveIndexMock: vi.fn(),
  addToastMock: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ enableDriveIndex: enableDriveIndexMock }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock }))

beforeEach(() => {
  enableDriveIndexMock.mockReset()
  addToastMock.mockReset()
})

/** The message of the one toast the call raised, plus its level. */
function toast(): { message: unknown; level: unknown } {
  expect(addToastMock).toHaveBeenCalledTimes(1)
  const [message, opts] = addToastMock.mock.calls[0] as [unknown, { level: unknown }]
  return { message, level: opts.level }
}

describe('indexUncoveredDrive', () => {
  it('confirms the scan started, naming the drive', async () => {
    enableDriveIndexMock.mockResolvedValue({ status: 'ok', data: { status: 'started' } })
    await indexUncoveredDrive('smb-naspi', 'Naspolya')

    expect(enableDriveIndexMock).toHaveBeenCalledWith('smb-naspi')
    expect(toast()).toEqual({
      message: tString('search.coverage.toast.started', { drive: 'Naspolya' }),
      level: 'info',
    })
  })

  it('points at the master switch when it is what refused', async () => {
    enableDriveIndexMock.mockResolvedValue({ status: 'ok', data: { status: 'indexing_disabled' } })
    await indexUncoveredDrive('root', 'Macintosh HD')

    expect(toast()).toEqual({ message: tString('search.coverage.toast.indexingOff'), level: 'info' })
  })

  it('points at the drive menu for a typed per-drive refusal', async () => {
    enableDriveIndexMock.mockResolvedValue({
      status: 'ok',
      data: { status: 'refused', reason: 'credentials_needed' },
    })
    await indexUncoveredDrive('smb-naspi', 'Naspolya')

    expect(toast()).toEqual({
      message: tString('search.coverage.toast.notStarted', { drive: 'Naspolya' }),
      level: 'warn',
    })
  })

  it('says the same when the IPC itself fails or errors, never nothing', async () => {
    enableDriveIndexMock.mockResolvedValue({ status: 'error', error: 'boom' })
    await indexUncoveredDrive('smb-naspi', 'Naspolya')
    expect(toast().level).toBe('warn')

    addToastMock.mockReset()
    enableDriveIndexMock.mockRejectedValue(new Error('boom'))
    await indexUncoveredDrive('smb-naspi', 'Naspolya')
    expect(toast().level).toBe('warn')
  })

  it('promises the scan when a live search is what stands in its way, rather than warning it did not start', async () => {
    // The backend remembers the request and runs it when the walk ends, so the
    // button did what it says. Reporting it as "can't index right now" would be
    // the opposite of true.
    enableDriveIndexMock.mockResolvedValue({ status: 'ok', data: { status: 'deferred_until_search_ends' } })
    await indexUncoveredDrive('smb-naspi', 'Naspolya')

    expect(toast()).toEqual({
      message: tString('search.coverage.toast.deferredUntilSearchEnds', { drive: 'Naspolya' }),
      level: 'info',
    })
  })

  it('falls back to a generic drive name when the drive is not in the volume list', async () => {
    enableDriveIndexMock.mockResolvedValue({ status: 'ok', data: { status: 'started' } })
    await indexUncoveredDrive('smb-gone', '')

    expect(toast().message).toBe(
      tString('search.coverage.toast.started', { drive: tString('search.coverage.unnamedDrive') }),
    )
  })
})
