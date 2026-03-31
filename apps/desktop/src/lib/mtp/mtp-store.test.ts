/**
 * Tests for MTP store reactive behavior and device state management.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', async () => {
  const { getMtpDeviceDisplayName } = await import('$lib/tauri-commands/mtp')
  return {
    getMtpDeviceDisplayName,
    listMtpDevices: vi.fn(),
    connectMtpDevice: vi.fn(),
    disconnectMtpDevice: vi.fn(),
    onMtpDeviceConnected: vi.fn(),
    onMtpDeviceDisconnected: vi.fn(),
    onMtpExclusiveAccessError: vi.fn(),
    onMtpPermissionError: vi.fn(),
    onMtpDeviceDetected: vi.fn(),
    onMtpDeviceRemoved: vi.fn(),
  }
})

import type { MtpDeviceInfo, MtpStorageInfo, ConnectedMtpDeviceInfo } from '$lib/tauri-commands'
import {
  listMtpDevices,
  connectMtpDevice,
  disconnectMtpDevice,
  onMtpDeviceConnected,
  onMtpDeviceDisconnected,
  onMtpExclusiveAccessError,
  onMtpPermissionError,
  onMtpDeviceDetected,
  onMtpDeviceRemoved,
} from '$lib/tauri-commands'
import {
  getDevices,
  getDevice,
  getConnectedDevices,
  hasConnectedDevices,
  isInitialized,
  isScanning,
  scanDevices,
  connect,
  disconnect,
  initialize,
  cleanup,
  getMtpVolumes,
  resetForTesting,
} from './mtp-store.svelte'

const mockDevice: MtpDeviceInfo = {
  id: 'mtp-336592896',
  locationId: 336592896,
  vendorId: 0x18d1,
  productId: 0x4ee1,
  manufacturer: 'Google',
  product: 'Pixel 8',
}

const mockStorage: MtpStorageInfo = {
  id: 65537,
  name: 'Internal shared storage',
  totalBytes: 128_000_000_000,
  availableBytes: 64_000_000_000,
  storageType: 'FixedRAM',
  isReadOnly: false,
}

const mockConnectedInfo: ConnectedMtpDeviceInfo = {
  device: mockDevice,
  storages: [mockStorage],
}

describe('mtp-store', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetForTesting()

    // Default mock for event listeners - return unlisten functions
    vi.mocked(onMtpDeviceConnected).mockResolvedValue(vi.fn())
    vi.mocked(onMtpDeviceDisconnected).mockResolvedValue(vi.fn())
    vi.mocked(onMtpExclusiveAccessError).mockResolvedValue(vi.fn())
    vi.mocked(onMtpPermissionError).mockResolvedValue(vi.fn())
    vi.mocked(onMtpDeviceDetected).mockResolvedValue(vi.fn())
    vi.mocked(onMtpDeviceRemoved).mockResolvedValue(vi.fn())
  })

  describe('initial state', () => {
    it('returns empty devices before initialization', () => {
      expect(getDevices()).toEqual([])
      expect(isInitialized()).toBe(false)
    })

    it('has no connected devices initially', () => {
      expect(hasConnectedDevices()).toBe(false)
      expect(getConnectedDevices()).toEqual([])
    })
  })

  describe('scanDevices', () => {
    it('scans and adds new devices, then auto-connects', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await scanDevices()
      // Wait for auto-connect to complete (it runs asynchronously)
      await vi.waitFor(() => {
        expect(getDevice('mtp-336592896')?.connectionState).toBe('connected')
      })

      const devices = getDevices()
      expect(devices).toHaveLength(1)
      expect(devices[0].device.id).toBe('mtp-336592896')
      expect(devices[0].connectionState).toBe('connected')
      expect(devices[0].displayName).toBe('Pixel 8')

      const device = getDevice('mtp-336592896')
      expect(device).toBeDefined()
      expect(device?.device.product).toBe('Pixel 8')
    })

    it('preserves connection state for known devices', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await scanDevices()
      await connect('mtp-336592896')

      // Scan again
      await scanDevices()

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('connected')
      expect(device?.storages).toHaveLength(1)
    })

    it('skips scan if already scanning', async () => {
      vi.mocked(listMtpDevices).mockImplementation(
        () =>
          new Promise((resolve) => {
            setTimeout(() => {
              resolve([mockDevice])
            }, 100)
          }),
      )

      const promise1 = scanDevices()
      expect(isScanning()).toBe(true)

      const promise2 = scanDevices()
      await Promise.all([promise1, promise2])

      // Should only have called listMtpDevices once
      expect(listMtpDevices).toHaveBeenCalledTimes(1)
    })

    it('handles scan errors gracefully', async () => {
      vi.mocked(listMtpDevices).mockRejectedValue(new Error('USB error'))

      await scanDevices()

      expect(getDevices()).toEqual([])
      expect(isScanning()).toBe(false)
    })

    it('removes devices no longer present after scan', async () => {
      vi.mocked(listMtpDevices).mockResolvedValueOnce([mockDevice])

      await scanDevices()
      expect(getDevices()).toHaveLength(1)

      // Device was unplugged
      vi.mocked(listMtpDevices).mockResolvedValueOnce([])
      await scanDevices()

      expect(getDevices()).toHaveLength(0)
    })
  })

  describe('connect', () => {
    it('auto-connects devices after scan and updates state', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await scanDevices()
      // Wait for auto-connect to complete
      await vi.waitFor(() => {
        expect(getDevice('mtp-336592896')?.connectionState).toBe('connected')
      })

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('connected')
      expect(device?.storages).toEqual([mockStorage])

      expect(hasConnectedDevices()).toBe(true)
      expect(getConnectedDevices()).toHaveLength(1)
      expect(connectMtpDevice).toHaveBeenCalledTimes(1)
    })

    it('returns undefined for unknown device', async () => {
      const result = await connect('mtp-unknown')

      expect(result).toBeUndefined()
    })

    it('returns existing info for already connected device', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await scanDevices()
      await connect('mtp-336592896')

      // Try to connect again
      const result = await connect('mtp-336592896')

      expect(result).toBeDefined()
      expect(connectMtpDevice).toHaveBeenCalledTimes(1) // Should not call again
    })

    it('sets error state on auto-connect failure', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockRejectedValue(new Error('Exclusive access error'))

      await scanDevices()
      // Wait for auto-connect to fail
      await vi.waitFor(() => {
        expect(getDevice('mtp-336592896')?.connectionState).toBe('error')
      })

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('error')
      expect(device?.error).toBe('Exclusive access error')
    })
  })

  describe('disconnect', () => {
    it('disconnects from a device and clears storages', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
      vi.mocked(disconnectMtpDevice).mockResolvedValue(undefined)

      await scanDevices()
      await connect('mtp-336592896')
      expect(hasConnectedDevices()).toBe(true)

      await disconnect('mtp-336592896')

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('disconnected')
      expect(device?.storages).toEqual([])
      expect(hasConnectedDevices()).toBe(false)
    })

    it('handles disconnect for unknown device gracefully', async () => {
      // Should not throw
      await disconnect('mtp-unknown')
    })

    it('handles double disconnect gracefully (only calls backend once)', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
      vi.mocked(disconnectMtpDevice).mockResolvedValue(undefined)

      await scanDevices()
      // Wait for auto-connect to complete
      await vi.waitFor(() => {
        expect(getDevice('mtp-336592896')?.connectionState).toBe('connected')
      })

      // First disconnect
      await disconnect('mtp-336592896')
      expect(disconnectMtpDevice).toHaveBeenCalledTimes(1)

      // Second disconnect - should not call backend again
      await disconnect('mtp-336592896')
      expect(disconnectMtpDevice).toHaveBeenCalledTimes(1)
    })
  })

  describe('initialize', () => {
    it('sets up event listeners and scans devices', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()

      expect(isInitialized()).toBe(true)
      expect(getDevices()).toHaveLength(1)
      expect(onMtpDeviceConnected).toHaveBeenCalledWith(expect.any(Function))
      expect(onMtpDeviceDisconnected).toHaveBeenCalledWith(expect.any(Function))
      expect(onMtpExclusiveAccessError).toHaveBeenCalledWith(expect.any(Function))
      expect(onMtpPermissionError).toHaveBeenCalledWith(expect.any(Function))
      expect(onMtpDeviceDetected).toHaveBeenCalledWith(expect.any(Function))
      expect(onMtpDeviceRemoved).toHaveBeenCalledWith(expect.any(Function))
    })

    it('is idempotent (only initializes once)', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()
      await initialize()

      expect(listMtpDevices).toHaveBeenCalledTimes(1)
    })
  })

  describe('cleanup', () => {
    it('unregisters event listeners and resets state', async () => {
      const unlistenConnected = vi.fn()
      const unlistenDisconnected = vi.fn()
      const unlistenExclusive = vi.fn()
      const unlistenPermission = vi.fn()
      const unlistenDetected = vi.fn()
      const unlistenRemoved = vi.fn()

      vi.mocked(onMtpDeviceConnected).mockResolvedValue(unlistenConnected)
      vi.mocked(onMtpDeviceDisconnected).mockResolvedValue(unlistenDisconnected)
      vi.mocked(onMtpExclusiveAccessError).mockResolvedValue(unlistenExclusive)
      vi.mocked(onMtpPermissionError).mockResolvedValue(unlistenPermission)
      vi.mocked(onMtpDeviceDetected).mockResolvedValue(unlistenDetected)
      vi.mocked(onMtpDeviceRemoved).mockResolvedValue(unlistenRemoved)
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()
      expect(isInitialized()).toBe(true)
      expect(getDevices()).toHaveLength(1)

      cleanup()

      expect(unlistenConnected).toHaveBeenCalled()
      expect(unlistenDisconnected).toHaveBeenCalled()
      expect(unlistenExclusive).toHaveBeenCalled()
      expect(unlistenPermission).toHaveBeenCalled()
      expect(unlistenDetected).toHaveBeenCalled()
      expect(unlistenRemoved).toHaveBeenCalled()
      expect(isInitialized()).toBe(false)
      expect(getDevices()).toHaveLength(0)
    })
  })

  describe('getMtpVolumes', () => {
    it('returns empty array when no devices', () => {
      expect(getMtpVolumes()).toEqual([])
    })

    it('returns single volume for disconnected device', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await scanDevices()

      const volumes = getMtpVolumes()
      expect(volumes).toHaveLength(1)
      expect(volumes[0].id).toBe('mtp-336592896')
      expect(volumes[0].deviceId).toBe('mtp-336592896')
      expect(volumes[0].storageId).toBe(0)
      expect(volumes[0].name).toBe('Pixel 8')
      expect(volumes[0].isConnected).toBe(false)
    })

    it('returns one volume per storage for connected device', async () => {
      const multiStorageInfo: ConnectedMtpDeviceInfo = {
        device: mockDevice,
        storages: [
          mockStorage,
          {
            id: 65538,
            name: 'SD Card',
            totalBytes: 64_000_000_000,
            availableBytes: 32_000_000_000,
            storageType: 'RemovableRAM',
            isReadOnly: false,
          },
        ],
      }
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(multiStorageInfo)

      await scanDevices()
      await connect('mtp-336592896')

      const volumes = getMtpVolumes()
      expect(volumes).toHaveLength(2)
      expect(volumes[0].id).toBe('mtp-336592896:65537')
      expect(volumes[0].name).toBe('Pixel 8 - Internal shared storage')
      expect(volumes[0].storageId).toBe(65537)
      expect(volumes[0].isConnected).toBe(true)
      expect(volumes[1].id).toBe('mtp-336592896:65538')
      expect(volumes[1].name).toBe('Pixel 8 - SD Card')
      expect(volumes[1].storageId).toBe(65538)
    })

    it('propagates isReadOnly flag from storage to volume', async () => {
      const readOnlyStorageInfo: ConnectedMtpDeviceInfo = {
        device: mockDevice,
        storages: [
          {
            id: 65537,
            name: 'Camera Storage',
            totalBytes: 32_000_000_000,
            availableBytes: 16_000_000_000,
            storageType: 'FixedRAM',
            isReadOnly: true,
          },
        ],
      }
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(readOnlyStorageInfo)

      await scanDevices()
      await connect('mtp-336592896')

      const volumes = getMtpVolumes()
      expect(volumes).toHaveLength(1)
      expect(volumes[0].isReadOnly).toBe(true)
    })

    it('uses device name for single storage device', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await scanDevices()
      await connect('mtp-336592896')

      const volumes = getMtpVolumes()
      expect(volumes).toHaveLength(1)
      // Single storage: use device name, not storage name
      expect(volumes[0].name).toBe('Pixel 8')
    })
  })

  describe('event handling', () => {
    it('updates state on mtp-device-connected event', async () => {
      let connectedCallback: ((event: { deviceId: string; storages: MtpStorageInfo[] }) => void) | undefined
      vi.mocked(onMtpDeviceConnected).mockImplementation((callback) => {
        connectedCallback = callback
        return Promise.resolve(vi.fn())
      })
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()

      // Simulate event from backend
      connectedCallback?.({ deviceId: 'mtp-336592896', storages: [mockStorage] })

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('connected')
      expect(device?.storages).toEqual([mockStorage])
    })

    it('updates state on mtp-device-disconnected event', async () => {
      let disconnectedCallback: ((event: { deviceId: string; reason: 'user' | 'disconnected' }) => void) | undefined
      vi.mocked(onMtpDeviceDisconnected).mockImplementation((callback) => {
        disconnectedCallback = callback
        return Promise.resolve(vi.fn())
      })
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
      vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)

      await initialize()
      await connect('mtp-336592896')
      expect(getDevice('mtp-336592896')?.connectionState).toBe('connected')

      // Simulate event from backend
      disconnectedCallback?.({ deviceId: 'mtp-336592896', reason: 'disconnected' })

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('disconnected')
      expect(device?.storages).toEqual([])
    })

    it('sets error state on mtp-exclusive-access-error event', async () => {
      let exclusiveCallback: ((event: { deviceId: string; blockingProcess?: string }) => void) | undefined
      vi.mocked(onMtpExclusiveAccessError).mockImplementation((callback) => {
        exclusiveCallback = callback
        return Promise.resolve(vi.fn())
      })
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()

      // Simulate event from backend
      exclusiveCallback?.({ deviceId: 'mtp-336592896', blockingProcess: 'ptpcamerad' })

      const device = getDevice('mtp-336592896')
      expect(device?.connectionState).toBe('error')
      expect(device?.error).toContain('ptpcamerad')
    })

    it('rescans on mtp-device-detected event', async () => {
      let detectedCallback:
        | ((event: { deviceId: string; name?: string; vendorId: number; productId: number }) => void)
        | undefined
      vi.mocked(onMtpDeviceDetected).mockImplementation((callback) => {
        detectedCallback = callback
        return Promise.resolve(vi.fn())
      })
      vi.mocked(listMtpDevices).mockResolvedValue([])

      await initialize()
      expect(listMtpDevices).toHaveBeenCalledTimes(1)

      // Simulate device hotplug
      detectedCallback?.({ deviceId: 'mtp-336592896', vendorId: 0x18d1, productId: 0x4ee1 })

      // Wait for async rescan
      await new Promise((resolve) => setTimeout(resolve, 10))
      expect(listMtpDevices).toHaveBeenCalledTimes(2)
    })

    it('removes device and rescans on mtp-device-removed event', async () => {
      let removedCallback: ((event: { deviceId: string }) => void) | undefined
      vi.mocked(onMtpDeviceRemoved).mockImplementation((callback) => {
        removedCallback = callback
        return Promise.resolve(vi.fn())
      })
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await initialize()
      expect(getDevice('mtp-336592896')).toBeDefined()

      // Simulate device removal
      vi.mocked(listMtpDevices).mockResolvedValue([])
      removedCallback?.({ deviceId: 'mtp-336592896' })

      // Device should be removed immediately
      expect(getDevice('mtp-336592896')).toBeUndefined()
    })
  })

  describe('display name generation', () => {
    it('uses product name when available', async () => {
      vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

      await scanDevices()

      expect(getDevice('mtp-336592896')?.displayName).toBe('Pixel 8')
    })

    it('uses manufacturer name when product is missing', async () => {
      const deviceWithoutProduct: MtpDeviceInfo = {
        id: 'mtp-336592897',
        locationId: 336592897,
        vendorId: 0x04e8,
        productId: 0x6860,
        manufacturer: 'Samsung',
      }
      vi.mocked(listMtpDevices).mockResolvedValue([deviceWithoutProduct])

      await scanDevices()

      expect(getDevice('mtp-336592897')?.displayName).toBe('Samsung device')
    })

    it('uses vendor:product format as fallback', async () => {
      const deviceWithoutNames: MtpDeviceInfo = {
        id: 'mtp-336592898',
        locationId: 336592898,
        vendorId: 0x1234,
        productId: 0x5678,
      }
      vi.mocked(listMtpDevices).mockResolvedValue([deviceWithoutNames])

      await scanDevices()

      expect(getDevice('mtp-336592898')?.displayName).toBe('MTP device (1234:5678)')
    })
  })
})
