/**
 * The ADB wrappers: the device list passes through, and a connect refusal keeps
 * its typed reason instead of collapsing into a stringified blob.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    listAdbDevices: vi.fn(),
    cancelAdbConnect: vi.fn(),
    connectAdbDevice: vi.fn(),
    setAdbSettings: vi.fn(),
    getAdbInstallStatus: vi.fn(),
    recheckAdbInstall: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import {
  AdbConnectFailure,
  asAdbConnectError,
  cancelAdbConnect,
  connectAdbDevice,
  getAdbInstallStatus,
  listAdbDevices,
  recheckAdbInstall,
  setAdbSettings,
  type AdbDevice,
} from './adb'

// The shim casts `commands`; the mock carries the two ADB commands.
const mocked = commands as unknown as {
  listAdbDevices: ReturnType<typeof vi.fn>
  cancelAdbConnect: ReturnType<typeof vi.fn>
  connectAdbDevice: ReturnType<typeof vi.fn>
  setAdbSettings: ReturnType<typeof vi.fn>
  getAdbInstallStatus: ReturnType<typeof vi.fn>
  recheckAdbInstall: ReturnType<typeof vi.fn>
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('listAdbDevices', () => {
  it('passes the device list straight through, whatever the states', async () => {
    const devices: AdbDevice[] = [
      { serial: 'R58M12345', state: 'ready', product: null, model: 'Pixel_7', device: null, transportId: 1 },
      {
        serial: '192.168.1.5:5555',
        state: 'unauthorized',
        product: null,
        model: null,
        device: null,
        transportId: null,
      },
    ]
    mocked.listAdbDevices.mockResolvedValueOnce(devices)
    expect(await listAdbDevices()).toEqual(devices)
    expect(mocked.listAdbDevices).toHaveBeenCalledTimes(1)
  })
})

describe('connectAdbDevice', () => {
  it('hands the serial to the command and resolves to the volume id', async () => {
    mocked.connectAdbDevice.mockResolvedValueOnce({ status: 'ok', data: 'adb-pixel-7-a1b2c3d' })
    expect(await connectAdbDevice('R58M12345', 'adb-1')).toBe('adb-pixel-7-a1b2c3d')
    expect(mocked.connectAdbDevice).toHaveBeenCalledWith('R58M12345', 'adb-1')
  })

  it('throws a typed failure that a catch site can read back', async () => {
    mocked.connectAdbDevice.mockResolvedValueOnce({ status: 'error', error: { type: 'unauthorized' } })
    let caught: unknown
    try {
      await connectAdbDevice('R58M12345', 'adb-1')
    } catch (e) {
      caught = e
    }
    expect(caught).toBeInstanceOf(AdbConnectFailure)
    expect(asAdbConnectError(caught)).toEqual({ type: 'unauthorized' })
  })

  it('asAdbConnectError answers null for anything else', () => {
    expect(asAdbConnectError(new Error('boom'))).toBeNull()
    expect(asAdbConnectError(undefined)).toBeNull()
  })
})

describe('cancelAdbConnect', () => {
  it('calls off the dial under the caller\'s own attempt id', async () => {
    mocked.cancelAdbConnect.mockResolvedValueOnce(true)
    expect(await cancelAdbConnect('adb-1')).toBe(true)
    expect(mocked.cancelAdbConnect).toHaveBeenCalledWith('adb-1')
  })

  it('reports a plain false when nothing was dialing, which is ordinary', async () => {
    mocked.cancelAdbConnect.mockResolvedValueOnce(false)
    expect(await cancelAdbConnect('adb-already-finished')).toBe(false)
  })
})

describe('setAdbSettings', () => {
  it('sends both settings together, because the tracker restarts under the named binary', async () => {
    mocked.setAdbSettings.mockResolvedValueOnce(undefined)
    await setAdbSettings(true, '/opt/sdk/platform-tools/adb')
    expect(mocked.setAdbSettings).toHaveBeenCalledWith(true, '/opt/sdk/platform-tools/adb')
  })

  it('sends null for "look for adb the usual way"', async () => {
    mocked.setAdbSettings.mockResolvedValueOnce(undefined)
    await setAdbSettings(false, null)
    expect(mocked.setAdbSettings).toHaveBeenCalledWith(false, null)
  })
})

describe('install status', () => {
  it('reads what is already known without looking again', async () => {
    const status = { binaryPath: '/opt/homebrew/bin/adb', tracking: true }
    mocked.getAdbInstallStatus.mockResolvedValueOnce(status)
    expect(await getAdbInstallStatus()).toEqual(status)
  })

  it('re-checks only when asked, which is the one path allowed to retry the server start', async () => {
    const status = { binaryPath: null, tracking: false }
    mocked.recheckAdbInstall.mockResolvedValueOnce(status)
    expect(await recheckAdbInstall()).toEqual(status)
    expect(mocked.recheckAdbInstall).toHaveBeenCalledTimes(1)
  })
})
