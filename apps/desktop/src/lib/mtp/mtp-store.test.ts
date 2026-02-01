/**
 * Tests for MTP store reactive behavior and device state management.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/tauri-commands', async (importOriginal) => {
    const original = await importOriginal<typeof import('$lib/tauri-commands')>()
    return {
        ...original,
        listMtpDevices: vi.fn(),
        connectMtpDevice: vi.fn(),
        disconnectMtpDevice: vi.fn(),
        getMtpDeviceDisplayName: original.getMtpDeviceDisplayName, // Use real implementation
        onMtpDeviceConnected: vi.fn(),
        onMtpDeviceDisconnected: vi.fn(),
        onMtpExclusiveAccessError: vi.fn(),
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
    onMtpDeviceDetected,
    onMtpDeviceRemoved,
} from '$lib/tauri-commands'

const mockDevice: MtpDeviceInfo = {
    id: 'mtp-1-5',
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
}

const mockConnectedInfo: ConnectedMtpDeviceInfo = {
    device: mockDevice,
    storages: [mockStorage],
}

describe('mtp-store', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.resetModules()

        // Default mock for event listeners - return unlisten functions
        vi.mocked(onMtpDeviceConnected).mockResolvedValue(vi.fn())
        vi.mocked(onMtpDeviceDisconnected).mockResolvedValue(vi.fn())
        vi.mocked(onMtpExclusiveAccessError).mockResolvedValue(vi.fn())
        vi.mocked(onMtpDeviceDetected).mockResolvedValue(vi.fn())
        vi.mocked(onMtpDeviceRemoved).mockResolvedValue(vi.fn())
    })

    async function loadModule() {
        return await import('./mtp-store.svelte')
    }

    describe('initial state', () => {
        it('returns empty devices before initialization', async () => {
            const { getDevices, isInitialized } = await loadModule()
            expect(getDevices()).toEqual([])
            expect(isInitialized()).toBe(false)
        })

        it('has no connected devices initially', async () => {
            const { hasConnectedDevices, getConnectedDevices } = await loadModule()
            expect(hasConnectedDevices()).toBe(false)
            expect(getConnectedDevices()).toEqual([])
        })
    })

    describe('scanDevices', () => {
        it('scans and adds new devices', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { scanDevices, getDevices, getDevice } = await loadModule()

            await scanDevices()

            const devices = getDevices()
            expect(devices).toHaveLength(1)
            expect(devices[0].device.id).toBe('mtp-1-5')
            expect(devices[0].connectionState).toBe('disconnected')
            expect(devices[0].displayName).toBe('Pixel 8')

            const device = getDevice('mtp-1-5')
            expect(device).toBeDefined()
            expect(device?.device.product).toBe('Pixel 8')
        })

        it('preserves connection state for known devices', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            const { scanDevices, connect, getDevice } = await loadModule()

            await scanDevices()
            await connect('mtp-1-5')

            // Scan again
            await scanDevices()

            const device = getDevice('mtp-1-5')
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
            const { scanDevices, isScanning } = await loadModule()

            const promise1 = scanDevices()
            expect(isScanning()).toBe(true)

            const promise2 = scanDevices()
            await Promise.all([promise1, promise2])

            // Should only have called listMtpDevices once
            expect(listMtpDevices).toHaveBeenCalledTimes(1)
        })

        it('handles scan errors gracefully', async () => {
            vi.mocked(listMtpDevices).mockRejectedValue(new Error('USB error'))
            const { scanDevices, getDevices, isScanning } = await loadModule()

            await scanDevices()

            expect(getDevices()).toEqual([])
            expect(isScanning()).toBe(false)
        })

        it('removes devices no longer present after scan', async () => {
            vi.mocked(listMtpDevices).mockResolvedValueOnce([mockDevice])
            const { scanDevices, getDevices } = await loadModule()

            await scanDevices()
            expect(getDevices()).toHaveLength(1)

            // Device was unplugged
            vi.mocked(listMtpDevices).mockResolvedValueOnce([])
            await scanDevices()

            expect(getDevices()).toHaveLength(0)
        })
    })

    describe('connect', () => {
        it('connects to a device and updates state', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            const { scanDevices, connect, getDevice, getConnectedDevices, hasConnectedDevices } = await loadModule()

            await scanDevices()
            const result = await connect('mtp-1-5')

            expect(result).toBeDefined()
            expect(result?.device.id).toBe('mtp-1-5')
            expect(result?.storages).toHaveLength(1)

            const device = getDevice('mtp-1-5')
            expect(device?.connectionState).toBe('connected')
            expect(device?.storages).toEqual([mockStorage])

            expect(hasConnectedDevices()).toBe(true)
            expect(getConnectedDevices()).toHaveLength(1)
        })

        it('returns undefined for unknown device', async () => {
            const { connect } = await loadModule()

            const result = await connect('mtp-unknown')

            expect(result).toBeUndefined()
        })

        it('returns existing info for already connected device', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            const { scanDevices, connect } = await loadModule()

            await scanDevices()
            await connect('mtp-1-5')

            // Try to connect again
            const result = await connect('mtp-1-5')

            expect(result).toBeDefined()
            expect(connectMtpDevice).toHaveBeenCalledTimes(1) // Should not call again
        })

        it('sets error state on connection failure', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockRejectedValue(new Error('Exclusive access error'))
            const { scanDevices, connect, getDevice } = await loadModule()

            await scanDevices()

            await expect(connect('mtp-1-5')).rejects.toThrow('Exclusive access error')

            const device = getDevice('mtp-1-5')
            expect(device?.connectionState).toBe('error')
            expect(device?.error).toBe('Exclusive access error')
        })
    })

    describe('disconnect', () => {
        it('disconnects from a device and clears storages', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            vi.mocked(disconnectMtpDevice).mockResolvedValue(undefined)
            const { scanDevices, connect, disconnect, getDevice, hasConnectedDevices } = await loadModule()

            await scanDevices()
            await connect('mtp-1-5')
            expect(hasConnectedDevices()).toBe(true)

            await disconnect('mtp-1-5')

            const device = getDevice('mtp-1-5')
            expect(device?.connectionState).toBe('disconnected')
            expect(device?.storages).toEqual([])
            expect(hasConnectedDevices()).toBe(false)
        })

        it('handles disconnect for unknown device gracefully', async () => {
            const { disconnect } = await loadModule()

            // Should not throw
            await disconnect('mtp-unknown')
        })

        it('handles disconnect for already disconnected device', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { scanDevices, disconnect } = await loadModule()

            await scanDevices()

            // Should not throw or call backend
            await disconnect('mtp-1-5')
            expect(disconnectMtpDevice).not.toHaveBeenCalled()
        })
    })

    describe('initialize', () => {
        it('sets up event listeners and scans devices', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { initialize, isInitialized, getDevices } = await loadModule()

            await initialize()

            expect(isInitialized()).toBe(true)
            expect(getDevices()).toHaveLength(1)
            expect(onMtpDeviceConnected).toHaveBeenCalledWith(expect.any(Function))
            expect(onMtpDeviceDisconnected).toHaveBeenCalledWith(expect.any(Function))
            expect(onMtpExclusiveAccessError).toHaveBeenCalledWith(expect.any(Function))
            expect(onMtpDeviceDetected).toHaveBeenCalledWith(expect.any(Function))
            expect(onMtpDeviceRemoved).toHaveBeenCalledWith(expect.any(Function))
        })

        it('is idempotent (only initializes once)', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { initialize } = await loadModule()

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
            const unlistenDetected = vi.fn()
            const unlistenRemoved = vi.fn()

            vi.mocked(onMtpDeviceConnected).mockResolvedValue(unlistenConnected)
            vi.mocked(onMtpDeviceDisconnected).mockResolvedValue(unlistenDisconnected)
            vi.mocked(onMtpExclusiveAccessError).mockResolvedValue(unlistenExclusive)
            vi.mocked(onMtpDeviceDetected).mockResolvedValue(unlistenDetected)
            vi.mocked(onMtpDeviceRemoved).mockResolvedValue(unlistenRemoved)
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])

            const { initialize, cleanup, isInitialized, getDevices } = await loadModule()

            await initialize()
            expect(isInitialized()).toBe(true)
            expect(getDevices()).toHaveLength(1)

            cleanup()

            expect(unlistenConnected).toHaveBeenCalled()
            expect(unlistenDisconnected).toHaveBeenCalled()
            expect(unlistenExclusive).toHaveBeenCalled()
            expect(unlistenDetected).toHaveBeenCalled()
            expect(unlistenRemoved).toHaveBeenCalled()
            expect(isInitialized()).toBe(false)
            expect(getDevices()).toHaveLength(0)
        })
    })

    describe('getMtpVolumes', () => {
        it('returns empty array when no devices', async () => {
            const { getMtpVolumes } = await loadModule()

            expect(getMtpVolumes()).toEqual([])
        })

        it('returns single volume for disconnected device', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { scanDevices, getMtpVolumes } = await loadModule()

            await scanDevices()

            const volumes = getMtpVolumes()
            expect(volumes).toHaveLength(1)
            expect(volumes[0].id).toBe('mtp-1-5')
            expect(volumes[0].deviceId).toBe('mtp-1-5')
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
                    },
                ],
            }
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(multiStorageInfo)
            const { scanDevices, connect, getMtpVolumes } = await loadModule()

            await scanDevices()
            await connect('mtp-1-5')

            const volumes = getMtpVolumes()
            expect(volumes).toHaveLength(2)
            expect(volumes[0].id).toBe('mtp-1-5:65537')
            expect(volumes[0].name).toBe('Pixel 8 - Internal shared storage')
            expect(volumes[0].storageId).toBe(65537)
            expect(volumes[0].isConnected).toBe(true)
            expect(volumes[1].id).toBe('mtp-1-5:65538')
            expect(volumes[1].name).toBe('Pixel 8 - SD Card')
            expect(volumes[1].storageId).toBe(65538)
        })

        it('uses storage name only for single storage device', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            const { scanDevices, connect, getMtpVolumes } = await loadModule()

            await scanDevices()
            await connect('mtp-1-5')

            const volumes = getMtpVolumes()
            expect(volumes).toHaveLength(1)
            // Single storage: use storage name, not "Device - Storage"
            expect(volumes[0].name).toBe('Internal shared storage')
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
            const { initialize, getDevice } = await loadModule()

            await initialize()

            // Simulate event from backend
            connectedCallback?.({ deviceId: 'mtp-1-5', storages: [mockStorage] })

            const device = getDevice('mtp-1-5')
            expect(device?.connectionState).toBe('connected')
            expect(device?.storages).toEqual([mockStorage])
        })

        it('updates state on mtp-device-disconnected event', async () => {
            let disconnectedCallback:
                | ((event: { deviceId: string; reason: 'user' | 'disconnected' }) => void)
                | undefined
            vi.mocked(onMtpDeviceDisconnected).mockImplementation((callback) => {
                disconnectedCallback = callback
                return Promise.resolve(vi.fn())
            })
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            vi.mocked(connectMtpDevice).mockResolvedValue(mockConnectedInfo)
            const { initialize, connect, getDevice } = await loadModule()

            await initialize()
            await connect('mtp-1-5')
            expect(getDevice('mtp-1-5')?.connectionState).toBe('connected')

            // Simulate event from backend
            disconnectedCallback?.({ deviceId: 'mtp-1-5', reason: 'disconnected' })

            const device = getDevice('mtp-1-5')
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
            const { initialize, getDevice } = await loadModule()

            await initialize()

            // Simulate event from backend
            exclusiveCallback?.({ deviceId: 'mtp-1-5', blockingProcess: 'ptpcamerad' })

            const device = getDevice('mtp-1-5')
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
            const { initialize } = await loadModule()

            await initialize()
            expect(listMtpDevices).toHaveBeenCalledTimes(1)

            // Simulate device hotplug
            detectedCallback?.({ deviceId: 'mtp-1-5', vendorId: 0x18d1, productId: 0x4ee1 })

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
            const { initialize, getDevice } = await loadModule()

            await initialize()
            expect(getDevice('mtp-1-5')).toBeDefined()

            // Simulate device removal
            vi.mocked(listMtpDevices).mockResolvedValue([])
            removedCallback?.({ deviceId: 'mtp-1-5' })

            // Device should be removed immediately
            expect(getDevice('mtp-1-5')).toBeUndefined()
        })
    })

    describe('display name generation', () => {
        it('uses product name when available', async () => {
            vi.mocked(listMtpDevices).mockResolvedValue([mockDevice])
            const { scanDevices, getDevice } = await loadModule()

            await scanDevices()

            expect(getDevice('mtp-1-5')?.displayName).toBe('Pixel 8')
        })

        it('uses manufacturer name when product is missing', async () => {
            const deviceWithoutProduct: MtpDeviceInfo = {
                id: 'mtp-2-6',
                vendorId: 0x04e8,
                productId: 0x6860,
                manufacturer: 'Samsung',
            }
            vi.mocked(listMtpDevices).mockResolvedValue([deviceWithoutProduct])
            const { scanDevices, getDevice } = await loadModule()

            await scanDevices()

            expect(getDevice('mtp-2-6')?.displayName).toBe('Samsung device')
        })

        it('uses vendor:product format as fallback', async () => {
            const deviceWithoutNames: MtpDeviceInfo = {
                id: 'mtp-3-7',
                vendorId: 0x1234,
                productId: 0x5678,
            }
            vi.mocked(listMtpDevices).mockResolvedValue([deviceWithoutNames])
            const { scanDevices, getDevice } = await loadModule()

            await scanDevices()

            expect(getDevice('mtp-3-7')?.displayName).toBe('MTP device (1234:5678)')
        })
    })
})
