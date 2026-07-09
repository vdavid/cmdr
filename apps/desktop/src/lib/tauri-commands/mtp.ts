// MTP (Android device) support (macOS and Linux)

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type MtpDeviceConnected,
  type MtpDeviceDisconnected,
  type MtpExclusiveAccessError,
  type MtpPermissionError,
  type MtpStorageInfo as MtpStorageInfoBinding,
  type MtpTransferProgress as MtpTransferProgressBinding,
} from '$lib/ipc/bindings'
import type { ConflictResolution, FileEntry, WriteOperationStartResult } from '../file-explorer/types'
import { throwIpcError } from './ipc-types'

/**
 * Enables or disables MTP (Android device) support at runtime.
 * When disabled, stops USB device detection and disconnects all MTP devices.
 */
export async function setMtpEnabled(enabled: boolean): Promise<void> {
  await commands.setMtpEnabled(enabled)
}

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
    return (await commands.listMtpDevices()) as MtpDeviceInfo[]
  } catch {
    // Command not available (non-macOS) - return empty array
    return []
  }
}

/**
 * Information about a storage area on an MTP device.
 * Aliased to the `tauri-specta`-generated type so it matches the
 * `mtp-device-connected` event payload's nested storages exactly.
 */
export type MtpStorageInfo = MtpStorageInfoBinding

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
  | { type: 'permissionDenied'; deviceId: string }
  | { type: 'timeout'; deviceId: string }
  | { type: 'disconnected'; deviceId: string }
  | { type: 'protocol'; deviceId: string; message: string }
  | { type: 'other'; deviceId: string; message: string }
  | { type: 'notSupported'; message: string }

/**
 * Checks if an error is an MTP connection error.
 */
export function isMtpConnectionError(error: unknown): error is MtpConnectionError {
  return typeof error === 'object' && error !== null && 'type' in error && typeof error.type === 'string'
}

/**
 * Connects to an MTP device by ID.
 * Opens an MTP session and retrieves storage information.
 * If another process has exclusive access, an 'mtp-exclusive-access-error' event is emitted.
 * @param deviceId - The device ID from listMtpDevices
 * @returns Information about the connected device including storages
 */
export async function connectMtpDevice(deviceId: string): Promise<ConnectedMtpDeviceInfo> {
  const res = await commands.connectMtpDevice(deviceId)
  if (res.status === 'error') {
    // The error is a tagged-union MtpConnectionError, not an Error object. Callers use
    // `isMtpConnectionError()` to discriminate. Lint can't see that pattern.
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- tagged-union error consumed via isMtpConnectionError() guard
    throw res.error
  }
  return res.data as ConnectedMtpDeviceInfo
}

/**
 * Disconnects from an MTP device.
 * Closes the MTP session gracefully.
 * @param deviceId - The device ID to disconnect from
 */
export async function disconnectMtpDevice(deviceId: string): Promise<void> {
  const res = await commands.disconnectMtpDevice(deviceId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Gets information about a connected MTP device.
 * Returns null if the device is not connected.
 * @param deviceId - The device ID to query
 */
export async function getMtpDeviceInfo(deviceId: string): Promise<ConnectedMtpDeviceInfo | null> {
  try {
    const result = await commands.getMtpDeviceInfo(deviceId)
    return result as ConnectedMtpDeviceInfo | null
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
    return await commands.getPtpcameradWorkaroundCommand()
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
    const result = await commands.getMtpStorages(deviceId)
    return result
  } catch {
    return []
  }
}

// The event payload types are generated by `tauri-specta` into `bindings.ts`. We re-export them
// under their historical `*Event` names so consumers keep a stable import surface.
export type MtpExclusiveAccessErrorEvent = MtpExclusiveAccessError
export type MtpPermissionErrorEvent = MtpPermissionError
export type MtpDeviceConnectedEvent = MtpDeviceConnected
export type MtpDeviceDisconnectedEvent = MtpDeviceDisconnected

/**
 * Subscribes to MTP exclusive access error events.
 * Emitted when connecting fails because another process (like ptpcamerad) has the device.
 */
export async function onMtpExclusiveAccessError(
  callback: (event: MtpExclusiveAccessErrorEvent) => void,
): Promise<UnlistenFn> {
  return events.mtpExclusiveAccessError.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Subscribes to MTP permission error events (Linux only).
 * Emitted when USB device access fails due to missing udev rules.
 */
export async function onMtpPermissionError(callback: (event: MtpPermissionErrorEvent) => void): Promise<UnlistenFn> {
  return events.mtpPermissionError.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Subscribes to MTP device connected events.
 */
export async function onMtpDeviceConnected(callback: (event: MtpDeviceConnectedEvent) => void): Promise<UnlistenFn> {
  return events.mtpDeviceConnected.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Subscribes to MTP device disconnected events.
 */
export async function onMtpDeviceDisconnected(
  callback: (event: MtpDeviceDisconnectedEvent) => void,
): Promise<UnlistenFn> {
  return events.mtpDeviceDisconnected.listen((event) => {
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
  const res = await commands.listMtpDirectory(deviceId, storageId, path)
  if (res.status === 'error') {
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- tagged-union error consumed via isMtpConnectionError() guard
    throw res.error
  }
  return res.data as FileEntry[]
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
  /** Size in bytes (null for directories). */
  size: number | null
}

/** Progress event for MTP file transfers (generated by `tauri-specta`). */
export type MtpTransferProgress = MtpTransferProgressBinding

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
  const res = await commands.downloadMtpFile(deviceId, storageId, objectPath, localDest, operationId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  const res = await commands.uploadToMtp(deviceId, storageId, localPath, destFolder, operationId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Deletes an object (file or folder) from an MTP device.
 * For folders, this recursively deletes all contents first.
 * @param deviceId - The connected device ID
 * @param storageId - The storage ID within the device
 * @param objectPath - Virtual path on the device
 */
export async function deleteMtpObject(deviceId: string, storageId: number, objectPath: string): Promise<void> {
  const res = await commands.deleteMtpObject(deviceId, storageId, objectPath)
  if (res.status === 'error') throwIpcError(res.error)
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
  const res = await commands.createMtpFolder(deviceId, storageId, parentPath, folderName)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  const res = await commands.renameMtpObject(deviceId, storageId, objectPath, newName)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  const res = await commands.moveMtpObject(deviceId, storageId, objectPath, newParentPath)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Subscribes to MTP transfer progress events.
 * Emitted during download and upload operations.
 */
export async function onMtpTransferProgress(callback: (event: MtpTransferProgress) => void): Promise<UnlistenFn> {
  return events.mtpTransferProgress.listen((event) => {
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
  const res = await commands.scanMtpForCopy(deviceId, storageId, path)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  /** `true` when the source item is a directory. Lets the FE classify a
   *  dir-vs-dir collision as a silent merge ("will merge") instead of a
   *  conflict. */
  sourceIsDirectory: boolean
  /** `true` when the destination item is a directory. See `sourceIsDirectory`. */
  destIsDirectory: boolean
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
  progressIntervalMs: number
  conflictResolution: ConflictResolution
  maxConflictsToShow: number
  /** Preview scan ID to reuse cached scan results (from startScanPreview). */
  previewId?: string | null
  /**
   * Source filenames already known to conflict at the destination (from the
   * pre-flight `scanVolumeForConflicts` call). When `conflictResolution` is
   * `'skip'`, the backend bulk-skips these upfront so the progress bar
   * reflects them immediately instead of advancing one-per-conflict as the
   * loop re-discovers them. Ignored for other resolution modes.
   */
  preKnownConflicts?: string[]
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
  const res = await commands.copyBetweenVolumes(sourceVolumeId, sourcePaths, destVolumeId, destPath, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Moves files between any two volumes (local, MTP, etc.).
 * The backend picks the best strategy:
 * - Same volume: native rename/move (instant for MTP MoveObject)
 * - Both local: native move (rename for same-fs, copy+delete for cross-fs)
 * - Cross-volume: copy to destination, then delete source
 */
export async function moveBetweenVolumes(
  sourceVolumeId: string,
  sourcePaths: string[],
  destVolumeId: string,
  destPath: string,
  config?: VolumeCopyConfig,
): Promise<WriteOperationStartResult> {
  const res = await commands.moveBetweenVolumes(sourceVolumeId, sourcePaths, destVolumeId, destPath, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Compresses files into a NEW zip at `destZipPath` on `destVolumeId`, reusing the
 * archive-edit machinery (seed a valid empty zip, then pack the sources in). Same
 * events as `copyBetweenVolumes`. Local destination only in v1 — a remote parent
 * rejects with `RemoteArchiveCreationUnsupported`.
 *
 * @param sourceVolumeId - ID of the source volume (like "root" for local filesystem)
 * @param sourcePaths - Source file/directory paths relative to the source volume
 * @param destVolumeId - ID of the destination (parent) volume holding the new zip
 * @param destZipPath - Full path of the new `.zip` on the destination volume
 * @param config - Optional copy configuration
 * @returns Operation start result with operation ID
 */
export async function compressFiles(
  sourceVolumeId: string,
  sourcePaths: string[],
  destVolumeId: string,
  destZipPath: string,
  config?: VolumeCopyConfig,
): Promise<WriteOperationStartResult> {
  const res = await commands.compressFiles(sourceVolumeId, sourcePaths, destVolumeId, destZipPath, config ?? null)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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
  const res = await commands.scanVolumeForCopy(
    sourceVolumeId,
    sourcePaths,
    destVolumeId,
    destPath,
    maxConflicts ?? null,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/**
 * Scans destination volume for conflicts with source items.
 * Checks if any of the source item names already exist at the destination path.
 *
 * When `sourceVolumeId` and `sourcePaths` are both provided, the backend
 * resolves each item's real `is_directory` and size authoritatively from the
 * source volume (one batched stat, never a subtree walk), so the FE can
 * classify dir-vs-dir collisions as silent merges. Omit them for the legacy
 * name-only check.
 *
 * @param volumeId - ID of the destination volume to scan
 * @param sourceItems - List of source items to check
 * @param destPath - Destination directory path on the volume
 * @param sourceVolumeId - ID of the source volume, for authoritative type/size resolution
 * @param sourcePaths - Source paths on `sourceVolumeId`, aligned with `sourceItems` by name
 * @returns List of conflicts found
 */
export async function scanVolumeForConflicts(
  volumeId: string,
  sourceItems: SourceItemInput[],
  destPath: string,
  sourceVolumeId?: string,
  sourcePaths?: string[],
): Promise<VolumeConflictInfo[]> {
  const res = await commands.scanVolumeForConflicts(
    volumeId,
    sourceItems,
    destPath,
    sourceVolumeId ?? null,
    sourcePaths ?? null,
  )
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
