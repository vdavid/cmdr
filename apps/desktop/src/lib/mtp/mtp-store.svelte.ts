/**
 * Reactive store for MTP (Android device) state management.
 * Tracks connected devices, their connection status, and storages.
 */

import { SvelteMap } from 'svelte/reactivity'
import {
    type ConnectedMtpDeviceInfo,
    type MtpDeviceInfo,
    type MtpStorageInfo,
    type UnlistenFn,
    connectMtpDevice,
    disconnectMtpDevice,
    getMtpDeviceDisplayName,
    listMtpDevices,
    onMtpDeviceConnected,
    onMtpDeviceDisconnected,
    onMtpExclusiveAccessError,
} from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logger'

const logger = getAppLogger('mtp')

/** Connection state for a device. */
export type DeviceConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error'

/** Extended device info with connection state. */
export interface MtpDeviceState {
    device: MtpDeviceInfo
    connectionState: DeviceConnectionState
    storages: MtpStorageInfo[]
    /** Error message if connectionState is 'error'. */
    error?: string
    /** Display name for the device. */
    displayName: string
}

/** Store state. */
interface MtpStoreState {
    /** Map of device ID to device state. */
    devices: SvelteMap<string, MtpDeviceState>
    /** Whether the store has been initialized. */
    initialized: boolean
    /** Whether a device scan is in progress. */
    scanning: boolean
}

// Reactive state using Svelte 5 runes
let state = $state<MtpStoreState>({
    devices: new SvelteMap(),
    initialized: false,
    scanning: false,
})

// Event listeners
let unlistenConnected: UnlistenFn | undefined
let unlistenDisconnected: UnlistenFn | undefined
let unlistenExclusiveAccess: UnlistenFn | undefined

/**
 * Gets all devices as an array (for iteration in components).
 */
export function getDevices(): MtpDeviceState[] {
    return Array.from(state.devices.values())
}

/**
 * Gets a specific device by ID.
 */
export function getDevice(deviceId: string): MtpDeviceState | undefined {
    return state.devices.get(deviceId)
}

/**
 * Gets all connected devices.
 */
export function getConnectedDevices(): MtpDeviceState[] {
    return getDevices().filter((d) => d.connectionState === 'connected')
}

/**
 * Checks if any device is connected.
 */
export function hasConnectedDevices(): boolean {
    return getConnectedDevices().length > 0
}

/**
 * Checks if the store has been initialized.
 */
export function isInitialized(): boolean {
    return state.initialized
}

/**
 * Checks if a scan is in progress.
 */
export function isScanning(): boolean {
    return state.scanning
}

/**
 * Scans for connected MTP devices and updates the store.
 * Preserves connection state for already-known devices.
 */
export async function scanDevices(): Promise<void> {
    if (state.scanning) return

    state.scanning = true
    try {
        const devices = await listMtpDevices()
        const newDevices = new SvelteMap<string, MtpDeviceState>()

        for (const device of devices) {
            const existing = state.devices.get(device.id)
            if (existing) {
                // Preserve connection state and storages for known devices
                newDevices.set(device.id, {
                    ...existing,
                    device, // Update device info in case it changed
                    displayName: getMtpDeviceDisplayName(device),
                })
            } else {
                // New device, start disconnected
                newDevices.set(device.id, {
                    device,
                    connectionState: 'disconnected',
                    storages: [],
                    displayName: getMtpDeviceDisplayName(device),
                })
            }
        }

        state.devices = newDevices
        logger.info('Scanned {count} MTP device(s)', { count: devices.length })
    } catch (error) {
        logger.error('Failed to scan MTP devices: {error}', { error: String(error) })
    } finally {
        state.scanning = false
    }
}

/**
 * Connects to an MTP device.
 * Updates the store with connection state and storages.
 */
export async function connect(deviceId: string): Promise<ConnectedMtpDeviceInfo | undefined> {
    const deviceState = state.devices.get(deviceId)
    if (!deviceState) {
        logger.warn('Cannot connect: device {deviceId} not found in store', { deviceId })
        return undefined
    }

    if (deviceState.connectionState === 'connected') {
        logger.debug('Device {deviceId} already connected', { deviceId })
        return { device: deviceState.device, storages: deviceState.storages }
    }

    if (deviceState.connectionState === 'connecting') {
        logger.debug('Device {deviceId} connection already in progress', { deviceId })
        return undefined
    }

    // Update state to connecting
    state.devices.set(deviceId, {
        ...deviceState,
        connectionState: 'connecting',
        error: undefined,
    })

    try {
        const result = await connectMtpDevice(deviceId)

        // Update state with connected info
        state.devices.set(deviceId, {
            ...deviceState,
            device: result.device,
            connectionState: 'connected',
            storages: result.storages,
            displayName: getMtpDeviceDisplayName(result.device),
            error: undefined,
        })

        logger.info('Connected to MTP device: {displayName}', { displayName: deviceState.displayName })
        return result
    } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error)

        state.devices.set(deviceId, {
            ...deviceState,
            connectionState: 'error',
            error: errorMessage,
        })

        logger.error('Failed to connect to {displayName}: {error}', {
            displayName: deviceState.displayName,
            error: errorMessage,
        })
        throw error
    }
}

/**
 * Disconnects from an MTP device.
 */
export async function disconnect(deviceId: string): Promise<void> {
    const deviceState = state.devices.get(deviceId)
    if (!deviceState) {
        logger.warn('Cannot disconnect: device {deviceId} not found in store', { deviceId })
        return
    }

    if (deviceState.connectionState === 'disconnected') {
        logger.debug('Device {deviceId} already disconnected', { deviceId })
        return
    }

    try {
        await disconnectMtpDevice(deviceId)

        state.devices.set(deviceId, {
            ...deviceState,
            connectionState: 'disconnected',
            storages: [],
            error: undefined,
        })

        logger.info('Disconnected from MTP device: {displayName}', { displayName: deviceState.displayName })
    } catch (error) {
        logger.error('Failed to disconnect from {displayName}: {error}', {
            displayName: deviceState.displayName,
            error: String(error),
        })
        throw error
    }
}

/**
 * Initializes the MTP store.
 * Sets up event listeners and performs initial device scan.
 * Should be called once when the app starts.
 */
export async function initialize(): Promise<void> {
    if (state.initialized) return

    // Set up event listeners
    unlistenConnected = await onMtpDeviceConnected((event) => {
        const deviceState = state.devices.get(event.deviceId)
        if (deviceState) {
            state.devices.set(event.deviceId, {
                ...deviceState,
                connectionState: 'connected',
                storages: event.storages,
            })
        }
    })

    unlistenDisconnected = await onMtpDeviceDisconnected((event) => {
        const deviceState = state.devices.get(event.deviceId)
        if (deviceState) {
            state.devices.set(event.deviceId, {
                ...deviceState,
                connectionState: 'disconnected',
                storages: [],
            })
            logger.info('Device {displayName} disconnected ({reason})', {
                displayName: deviceState.displayName,
                reason: event.reason,
            })
        }
    })

    unlistenExclusiveAccess = await onMtpExclusiveAccessError((event) => {
        const deviceState = state.devices.get(event.deviceId)
        if (deviceState) {
            const blockingInfo = event.blockingProcess ? ` (blocked by ${event.blockingProcess})` : ''
            state.devices.set(event.deviceId, {
                ...deviceState,
                connectionState: 'error',
                error: `Another process has exclusive access${blockingInfo}`,
            })
        }
    })

    // Initial scan
    await scanDevices()

    state.initialized = true
    logger.info('MTP store initialized')
}

/**
 * Cleans up the MTP store.
 * Should be called when the app is shutting down.
 */
export function cleanup(): void {
    unlistenConnected?.()
    unlistenDisconnected?.()
    unlistenExclusiveAccess?.()

    state = {
        devices: new SvelteMap(),
        initialized: false,
        scanning: false,
    }
}

/**
 * Gets device state for use in reactive contexts.
 * This is a helper that returns the raw state for components.
 */
export function getMtpState(): MtpStoreState {
    return state
}
