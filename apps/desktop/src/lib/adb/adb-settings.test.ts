/**
 * The ADB settings push: both settings travel together, and a blank path means
 * "look for `adb` the usual way" rather than a path of spaces.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/settings', () => ({ getSetting: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ setAdbSettings: vi.fn() }))

import { getSetting } from '$lib/settings'
import { setAdbSettings } from '$lib/tauri-commands'
import { ADB_BINARY_PATH_SETTING_KEY, ADB_ENABLED_SETTING_KEY, pushAdbConfigToBackend } from './adb-settings'

const mockedGet = getSetting as unknown as ReturnType<typeof vi.fn>
const mockedPush = setAdbSettings as unknown as ReturnType<typeof vi.fn>

/** Answers the two ADB settings and nothing else, the way the store would. */
function settings(enabled: boolean, binaryPath: string) {
  mockedGet.mockImplementation((id: string) => {
    if (id === ADB_ENABLED_SETTING_KEY) return enabled
    if (id === ADB_BINARY_PATH_SETTING_KEY) return binaryPath
    throw new Error(`unexpected setting ${id}`)
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  mockedPush.mockResolvedValue(undefined)
})

describe('pushAdbConfigToBackend', () => {
  it('sends both settings, because the tracker restarts under whichever binary the path names', async () => {
    settings(true, '/opt/sdk/platform-tools/adb')
    await pushAdbConfigToBackend()
    expect(mockedPush).toHaveBeenCalledWith(true, '/opt/sdk/platform-tools/adb')
  })

  it('turns a blank path into null, so an emptied field means the platform search', async () => {
    settings(true, '   ')
    await pushAdbConfigToBackend()
    expect(mockedPush).toHaveBeenCalledWith(true, null)
  })

  it('trims a pasted path, which usually arrives with a trailing newline', async () => {
    settings(false, '  /usr/local/bin/adb\n')
    await pushAdbConfigToBackend()
    expect(mockedPush).toHaveBeenCalledWith(false, '/usr/local/bin/adb')
  })

  it('swallows a refusal: a settings write must never surface a backend error to the user', async () => {
    settings(true, '')
    mockedPush.mockRejectedValueOnce(new Error('the backend said no'))
    await expect(pushAdbConfigToBackend()).resolves.toBeUndefined()
  })
})
