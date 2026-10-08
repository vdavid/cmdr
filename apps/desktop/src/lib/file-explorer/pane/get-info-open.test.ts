/**
 * Get info: a Finder the user locked Cmdr out of gets the toast with the way back,
 * any other refusal gets a plain one, and a normal press says nothing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { GetInfoError } from '$lib/tauri-commands'

const m = vi.hoisted(() => ({
  getInfo: vi.fn<(path: string) => Promise<void>>(),
  addToast: vi.fn(),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: m.addToast }))
vi.mock('$lib/tauri-commands', async () => {
  const actual = await vi.importActual<typeof import('$lib/tauri-commands/file-actions')>(
    '$lib/tauri-commands/file-actions',
  )
  return { getInfo: m.getInfo, asGetInfoError: actual.asGetInfoError, GetInfoFailure: actual.GetInfoFailure }
})
vi.mock('./FinderAutomationOffToastContent.svelte', () => ({ default: 'FinderAutomationOffToastContent' }))

import { GetInfoFailure } from '$lib/tauri-commands/file-actions'
import { openGetInfoOrExplain } from './get-info-open'

const { getInfo, addToast } = m

function refuseWith(failure: GetInfoError): void {
  getInfo.mockRejectedValue(new GetInfoFailure(failure))
}

beforeEach(() => {
  vi.clearAllMocks()
  getInfo.mockResolvedValue(undefined)
})

describe('openGetInfoOrExplain', () => {
  it('asks for the window and says nothing when Finder takes the ask', async () => {
    await openGetInfoOrExplain('/Users/dave/notes.txt')

    expect(getInfo).toHaveBeenCalledExactlyOnceWith('/Users/dave/notes.txt')
    expect(addToast).not.toHaveBeenCalled()
  })

  it('shows the Automation toast when the user turned off control of Finder', async () => {
    refuseWith({ type: 'automationDenied' })

    await openGetInfoOrExplain('/Users/dave/notes.txt')

    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'FinderAutomationOffToastContent',
      expect.objectContaining({ id: 'get-info', dismissal: 'persistent' }),
    )
  })

  it.each<GetInfoError>([{ type: 'launchRefused', errno: 2 }, { type: 'timedOut' }])(
    'says it couldn’t reach Finder for $type',
    async (failure) => {
      refuseWith(failure)

      await openGetInfoOrExplain('/Users/dave/notes.txt')

      expect(addToast).toHaveBeenCalledExactlyOnceWith(
        expect.any(String),
        expect.objectContaining({ id: 'get-info', level: 'error' }),
      )
    },
  )
})
