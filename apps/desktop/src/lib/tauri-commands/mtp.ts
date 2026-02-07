// MTP (Android device) support (macOS only)

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ConflictResolution, FileEntry, WriteOperationStartResult } from '../file-explorer/types'

/** Information about a connected MTP device. */
export interface MtpDeviceInfo {
    /** Unique identifier for the device (format: "mtp-{locationId}"). */
    id: string
    /** Physical USB location identifier. Stable for a given port. */
    locationId: number
    /** USB vendor ID (for example, 0x18d1 for Google). */
    vendorId: number
    /** USB product ID. */
    productId: number
    /** Device manufacturer name, if available. */
    manufacturer?: string
    /** Device product name, if available. */
    product?: string
    /** USB serial number, if available. */
    serialNumber?: string
}

/**
 * Gets a display name for an MTP device.
 * Prefers product name, falls back to manufacturer, then vendor:product ID.
 */
export function getMtpDeviceDisplayName(device: MtpDeviceInfo): string {
    if (device.product) {
        return device.product
    }
    if (device.manufacturer) {
        return `${device.manufacturer} device`
    }
    return `MTP device (${device.vendorId.toString(16).padStart(4, '0')}:${device.productId.toString(16).padStart(4, '0')})`
}

/**
 * Lists all connected MTP devices.
 * Only available on macOS.
 * @returns Array of MtpDeviceInfo objects
 */
export async function listMtpDevices(): Promise<MtpDeviceInfo[]> {
    try {
        return await invoke<MtpDeviceInfo[]>('list_mtp_devices')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}

/** Information about a storage area on an MTP device. */
export interface MtpStorageInfo {
    /** Storage ID (MTP storage handle). */
    id: number
    /** Display name (like "Internal shared storage"). */
    name: string
    /** Total capacity in bytes. */
    totalBytes: number
    /** Available space in bytes. */
    availableBytes: number
    /** Storage type description (like "FixedROM", "RemovableRAM"). */
    storageType?: string
    /** Whether this storage is read-only (for example, PTP cameras). */
    isReadOnly: boolean
}

/** Information about a connected MTP device including its storages. */
export interface ConnectedMtpDeviceInfo {
    /** Device information. */
    device: MtpDeviceInfo
    /** Available storages on the device. */
    storages: MtpStorageInfo[]
}

/** Error types for MTP connection operations. */
export type MtpConnectionError =
    | { type: 'deviceNotFound'; deviceId: string }
    | { type: 'notConnected'; deviceId: string }
    | { type: 'exclusiveAccess'; deviceId: string; blockingProcess?: string }
    | { type: 'timeout'; deviceId: string }
    | { type: 'disconnected'; deviceId: string }
    | { type: 'protocol'; deviceId: string; message: string }
    | { type: 'other'; deviceId: string; message: string }
    | { type: 'notSupported'; message: string }

/**
 * Checks if an error is an MTP connection error.
 */
export function isMtpConnectionError(error: unknown): error is MtpConnectionError {
    return (
        typeof error === 'object' &&
        error !== null &&
        'type' in error &&
        typeof (error as { type: unknown }).type === 'string'
    )
}

/**
 * Connects to an MTP device by ID.
 * Opens an MTP session and retrieves storage information.
 * If another process has exclusive access, an 'mtp-exclusive-access-error' event is emitted.
 * @param deviceId - The device ID from listMtpDevices
 * @returns Information about the connected device including storages
 */
export async function connectMtpDevice(deviceId: string): Promise<ConnectedMtpDeviceInfo> {
    return invoke<ConnectedMtpDeviceInfo>('connect_mtp_device', { deviceId })
}

/**
 * Disconnects from an MTP device.
 * Closes the MTP session gracefully.
 * @param deviceId - The device ID to disconnect from
 */
export async function disconnectMtpDevice(deviceId: string): Promise<void> {
    await invoke('disconnect_mtp_device', { deviceId })
}

/**
 * Gets information about a connected MTP device.
 * Returns null if the device is not connected.
 * @param deviceId - The device ID to query
 */
export async function getMtpDeviceInfo(deviceId: string): Promise<ConnectedMtpDeviceInfo | null> {
    try {
        return await invoke<ConnectedMtpDeviceInfo | null>('get_mtp_device_info', { deviceId })
    } catch {
        return null
    }
}

/**
 * Gets the ptpcamerad workaround command for macOS.
 * Returns the Terminal command users can run to work around ptpcamerad blocking MTP.
 */
export async function getPtpcameradWorkaroundCommand(): Promise<string> {
    try {
        return await invoke<string>('get_ptpcamerad_workaround_command')
    } catch {
        return ''
    }
}

/**
 * Gets storage information for all storages on a connected device.
 * @param deviceId - The connected device ID
 * @returns Array of storage info, or empty if device is not connected
 */
export async function getMtpStorages(deviceId: string): Promise<MtpStorageInfo[]> {
    try {
        return await invoke<MtpStorageInfo[]>('get_mtp_storages', { deviceId })
    } catch {
        return []
    }
}

/** Event payload for mtp-device-detected (USB hotplug). */
export interface MtpDeviceDetectedEvent {
    deviceId: string
    name?: string
    vendorId: number
    productId: number
}

/** Event payload for mtp-device-removed (USB hotplug). */
export interface MtpDeviceRemovedEvent {
    deviceId: string
}

/** Event payload for mtp-exclusive-access-error. */
export interface MtpExclusiveAccessErrorEvent {
    deviceId: string
    blockingProcess?: string
}

/** Event payload for mtp-device-connected. */
export interface MtpDeviceConnectedEvent {
    deviceId: string
    storages: MtpStorageInfo[]
}

/** Event payload for mtp-device-disconnected. */
export interface MtpDeviceDisconnectedEvent {
    deviceId: string
    reason: 'user' | 'disconnected'
}

/**
 * Subscribes to MTP device detected events (USB hotplug).
 * Emitted when an MTP device is connected to the system.
 */
export async function onMtpDeviceDetected(callback: (event: MtpDeviceDetectedEvent) => void): Promise<UnlistenFn> {
    return listen<MtpDeviceDetectedEvent>('mtp-device-detected', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to MTP device removed events (USB hotplug).
 * Emitted when an MTP device is disconnected from the system.
 */
export async function onMtpDeviceRemoved(callback: (event: MtpDeviceRemovedEvent) => void): Promise<UnlistenFn> {
    return listen<MtpDeviceRemovedEvent>('mtp-device-removed', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to MTP exclusive access error events.
 * Emitted when connecting fails because another process (like ptpcamerad) has the device.
 */
export async function onMtpExclusiveAccessError(
    callback: (event: MtpExclusiveAccessErrorEvent) => void,
): Promise<UnlistenFn> {
    return listen<MtpExclusiveAccessErrorEvent>('mtp-exclusive-access-error', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to MTP device connected events.
 */
export async function onMtpDeviceConnected(callback: (event: MtpDeviceConnectedEvent) => void): Promise<UnlistenFn> {
    return listen<MtpDeviceConnectedEvent>('mtp-device-connected', (event) => {
        callback(event.payload)
    })
}

/**
 * Subscribes to MTP device disconnected events.
 */
export async function onMtpDeviceDisconnected(
    callback: (event: MtpDeviceDisconnectedEvent) => void,
): Promise<UnlistenFn> {
    return listen<MtpDeviceDisconnectedEvent>('mtp-device-disconnected', (event) => {
        callback(event.payload)
    })
}

// NOTE: MTP file watching now uses the unified directory-diff event system (same as local volumes).
// The mtp-directory-changed event and onMtpDirectoryChanged function have been removed.
// MTP events are now handled by the existing directory-diff listener in FilePane.svelte.

/**
 * Lists the contents of a directory on a connected MTP device.
 * Returns file entries in the same format as local directory listings.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param path - Virtual path to list (for example, "/" or "/DCIM")
 * @returns Array of FileEntry objects, sorted with directories first
 */
export async function listMtpDirectory(deviceId: string, storageId: number, path: string): Promise<FileEntry[]> {
    return invoke<FileEntry[]>('list_mtp_directory', { deviceId, storageId, path })
}

// ============================================================================
// MTP File Operations (Phase 4)
// ============================================================================

/** Result of a successful MTP operation. */
export interface MtpOperationResult {
    /** Operation ID for tracking. */
    operationId: string
    /** Number of files processed. */
    filesProcessed: number
    /** Total bytes transferred. */
    bytesTransferred: number
}

/** Information about an object on the device. */
export interface MtpObjectInfo {
    /** Object handle. */
    handle: number
    /** Object name. */
    name: string
    /** Virtual path on device. */
    path: string
    /** Whether it's a directory. */
    isDirectory: boolean
    /** Size in bytes (undefined for directories). */
    size?: number
}

/** Progress event for MTP file transfers. */
export interface MtpTransferProgress {
    /** Unique operation ID. */
    operationId: string
    /** Device ID. */
    deviceId: string
    /** Type of transfer. */
    transferType: 'download' | 'upload'
    /** Current file being transferred. */
    currentFile: string
    /** Bytes transferred so far. */
    bytesDone: number
    /** Total bytes to transfer. */
    bytesTotal: number
}

/**
 * Downloads a file from an MTP device to the local filesystem.
 * Emits `mtp-transfer-progress` events during the transfer.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param objectPath - Virtual path on the device (for example, "/DCIM/photo.jpg")
 * @param localDest - Local destination path
 * @param operationId - Unique operation ID for progress tracking
 */
export async function downloadMtpFile(
    deviceId: string,
    storageId: number,
    objectPath: string,
    localDest: string,
    operationId: string,
): Promise<MtpOperationResult> {
    return invoke<MtpOperationResult>('download_mtp_file', {
        deviceId,
        storageId,
        objectPath,
        localDest,
        operationId,
    })
}

/**
 * Uploads a file from the local filesystem to an MTP device.
 * Emits `mtp-transfer-progress` events during the transfer.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param localPath - Local file path to upload
 * @param destFolder - Destination folder path on device (for example, "/DCIM")
 * @param operationId - Unique operation ID for progress tracking
 */
export async function uploadToMtp(
    deviceId: string,
    storageId: number,
    localPath: string,
    destFolder: string,
    operationId: string,
): Promise<MtpObjectInfo> {
    return invoke<MtpObjectInfo>('upload_to_mtp', {
        deviceId,
        storageId,
        localPath,
        destFolder,
        operationId,
    })
}

/**
 * Deletes an object (file or folder) from an MTP device.
 * For folders, this recursively deletes all contents first.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param objectPath - Virtual path on the device
 */
export async function deleteMtpObject(deviceId: string, storageId: number, objectPath: string): Promise<void> {
    await invoke('delete_mtp_object', { deviceId, storageId, objectPath })
}

/**
 * Creates a new folder on an MTP device.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param parentPath - Parent folder path (for example, "/DCIM")
 * @param folderName - Name of the new folder
 */
export async function createMtpFolder(
    deviceId: string,
    storageId: number,
    parentPath: string,
    folderName: string,
): Promise<MtpObjectInfo> {
    return invoke<MtpObjectInfo>('create_mtp_folder', { deviceId, storageId, parentPath, folderName })
}

/**
 * Renames an object on an MTP device.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param objectPath - Current path of the object
 * @param newName - New name for the object
 */
export async function renameMtpObject(
    deviceId: string,
    storageId: number,
    objectPath: string,
    newName: string,
): Promise<MtpObjectInfo> {
    return invoke<MtpObjectInfo>('rename_mtp_object', { deviceId, storageId, objectPath, newName })
}

/**
 * Moves an object to a new parent folder on an MTP device.
 * May fail if the device doesn't support MoveObject operation.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param objectPath - Current path of the object
 * @param newParentPath - New parent folder path
 */
export async function moveMtpObject(
    deviceId: string,
    storageId: number,
    objectPath: string,
    newParentPath: string,
): Promise<MtpObjectInfo> {
    return invoke<MtpObjectInfo>('move_mtp_object', { deviceId, storageId, objectPath, newParentPath })
}

/**
 * Subscribes to MTP transfer progress events.
 * Emitted during download and upload operations.
 */
export async function onMtpTransferProgress(callback: (event: MtpTransferProgress) => void): Promise<UnlistenFn> {
    return listen<MtpTransferProgress>('mtp-transfer-progress', (event) => {
        callback(event.payload)
    })
}

/** Result of scanning MTP files/directories for copy operation. */
export interface MtpScanResult {
    fileCount: number
    dirCount: number
    totalBytes: number
}

/**
 * Scans MTP files/directories to get total counts and size before copying.
 * For directories, recursively scans all contents.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param path - Virtual path on the device to scan
 * @returns Scan result with file/dir counts and total bytes
 */
export async function scanMtpForCopy(deviceId: string, storageId: number, path: string): Promise<MtpScanResult> {
    return invoke<MtpScanResult>('scan_mtp_for_copy', { deviceId, storageId, path })
}

// ============================================================================
// Unified volume copy operations
// ============================================================================

/** Space information for a volume. */
export interface VolumeSpaceInfoExtended {
    totalBytes: number
    availableBytes: number
    usedBytes: number
}

/** Conflict information for a file that already exists at destination. */
export interface VolumeConflictInfo {
    sourcePath: string
    destPath: string
    sourceSize: number
    destSize: number
    sourceModified: number | null
    destModified: number | null
}

/** Result of scanning for a volume copy operation. */
export interface VolumeCopyScanResult {
    fileCount: number
    dirCount: number
    totalBytes: number
    destSpace: VolumeSpaceInfoExtended
    conflicts: VolumeConflictInfo[]
}

/** Configuration for volume copy operations. */
export interface VolumeCopyConfig {
    progressIntervalMs?: number
    conflictResolution?: ConflictResolution
    maxConflictsToShow?: number
}

/** Input for source item in conflict scanning. */
export interface SourceItemInput {
    name: string
    size: number
    modified: number | null
}

/**
 * Copies files between any two volumes (local, MTP, etc.).
 * This is the unified copy command that works for all volume types:
 * - Local -> Local (regular file copy)
 * - Local -> MTP (upload to Android device)
 * - MTP -> Local (download from Android device)
 *
 * @param sourceVolumeId - ID of the source volume (like "root" for local filesystem)
 * @param sourcePaths - List of source file/directory paths relative to source volume
 * @param destVolumeId - ID of the destination volume
 * @param destPath - Destination directory path relative to destination volume
 * @param config - Optional copy configuration
 * @returns Operation start result with operation ID
 */
export async function copyBetweenVolumes(
    sourceVolumeId: string,
    sourcePaths: string[],
    destVolumeId: string,
    destPath: string,
    config?: VolumeCopyConfig,
): Promise<WriteOperationStartResult> {
    return invoke<WriteOperationStartResult>('copy_between_volumes', {
        sourceVolumeId,
        sourcePaths,
        destVolumeId,
        destPath,
        config: config ?? {},
    })
}

/**
 * Scans source files for a volume copy operation without executing it.
 * Performs a "pre-flight" scan to determine:
 * - Total file count and bytes to copy
 * - Available space on destination
 * - Any conflicts (files that already exist at destination)
 *
 * @param sourceVolumeId - ID of the source volume
 * @param sourcePaths - List of source file/directory paths
 * @param destVolumeId - ID of the destination volume
 * @param destPath - Destination directory path
 * @param maxConflicts - Maximum number of conflicts to return (default: 100)
 * @returns Scan result with file counts, space info, and conflicts
 */
export async function scanVolumeForCopy(
    sourceVolumeId: string,
    sourcePaths: string[],
    destVolumeId: string,
    destPath: string,
    maxConflicts?: number,
): Promise<VolumeCopyScanResult> {
    return invoke<VolumeCopyScanResult>('scan_volume_for_copy', {
        sourceVolumeId,
        sourcePaths,
        destVolumeId,
        destPath,
        maxConflicts,
    })
}

/**
 * Scans destination volume for conflicts with source items.
 * Checks if any of the source item names already exist at the destination path.
 *
 * @param volumeId - ID of the destination volume to scan
 * @param sourceItems - List of source items to check
 * @param destPath - Destination directory path on the volume
 * @returns List of conflicts found
 */
export async function scanVolumeForConflicts(
    volumeId: string,
    sourceItems: SourceItemInput[],
    destPath: string,
): Promise<VolumeConflictInfo[]> {
    return invoke<VolumeConflictInfo[]>('scan_volume_for_conflicts', {
        volumeId,
        sourceItems,
        destPath,
    })
}
