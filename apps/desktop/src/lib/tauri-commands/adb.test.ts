/**
 * The ADB wrappers: the device list passes through, and a connect refusal keeps
 * its typed reason instead of collapsing into a stringified blob.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    listAdbDevices: vi.fn(),
    connectAdbDevice: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { AdbConnectFailure, asAdbConnectError, connectAdbDevice, listAdbDevices, type AdbDevice } from './adb'

// The shim casts `commands`; the mock carries the two ADB commands.
const mocked = commands as unknown as {
  listAdbDevices: ReturnType<typeof vi.fn>
  connectAdbDevice: ReturnType<typeof vi.fn>
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
    expect(await connectAdbDevice('R58M12345')).toBe('adb-pixel-7-a1b2c3d')
    expect(mocked.connectAdbDevice).toHaveBeenCalledWith('R58M12345')
  })

  it('throws a typed failure that a catch site can read back', async () => {
    mocked.connectAdbDevice.mockResolvedValueOnce({ status: 'error', error: { type: 'unauthorized' } })
    let caught: unknown
    try {
      await connectAdbDevice('R58M12345')
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
