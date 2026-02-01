/**
 * Utilities for parsing and constructing MTP paths.
 *
 * MTP path format: mtp://{deviceId}/{storageId}/{path}
 * Examples:
 *   - mtp://0-5/65537 (root of storage)
 *   - mtp://0-5/65537/DCIM/Camera (subfolder)
 *
 * Volume ID format: "mtp-{deviceId}-{storageId}" or just "{deviceId}:{storageId}"
 */

/** Parsed MTP path components. */
export interface ParsedMtpPath {
    deviceId: string
    storageId: number
    /** Path within the storage (empty string for root). */
    path: string
}

/**
 * Parses an MTP path into its components.
 * @returns Parsed path, or null if not a valid MTP path.
 */
export function parseMtpPath(path: string): ParsedMtpPath | null {
    if (!path.startsWith('mtp://')) {
        return null
    }

    // Remove the mtp:// prefix
    const rest = path.slice(6)

    // Split by /
    const parts = rest.split('/')

    if (parts.length < 2) {
        return null
    }

    const deviceId = parts[0]
    const storageId = parseInt(parts[1], 10)

    if (isNaN(storageId)) {
        return null
    }

    // Remaining parts form the path within the storage
    const innerPath = parts.slice(2).join('/')

    return {
        deviceId,
        storageId,
        path: innerPath,
    }
}

/**
 * Constructs an MTP path from components.
 */
export function constructMtpPath(deviceId: string, storageId: number, path: string = ''): string {
    const base = `mtp://${deviceId}/${String(storageId)}`
    if (!path || path === '/') {
        return base
    }
    // Ensure path doesn't start with / to avoid double slashes
    const normalizedPath = path.startsWith('/') ? path.slice(1) : path
    return `${base}/${normalizedPath}`
}

/**
 * Parses a volume ID to extract device and storage IDs.
 * Handles both "mtp-{deviceId}-{storageId}" and "{deviceId}:{storageId}" formats.
 * @returns Parsed IDs, or null if not a valid MTP volume ID.
 */
export function parseMtpVolumeId(volumeId: string): { deviceId: string; storageId: number } | null {
    // Format: "deviceId:storageId"
    if (volumeId.includes(':')) {
        const [deviceId, storageIdStr] = volumeId.split(':')
        const storageId = parseInt(storageIdStr, 10)
        if (!isNaN(storageId)) {
            return { deviceId, storageId }
        }
    }

    // Format: "mtp-{deviceId}" (no storage, used for unconnected devices)
    if (volumeId.startsWith('mtp-')) {
        // This is a device-only ID, not a storage-specific one
        return null
    }

    return null
}

/**
 * Checks if a volume ID represents an MTP volume.
 */
export function isMtpVolumeId(volumeId: string): boolean {
    return volumeId.includes(':') || volumeId.startsWith('mtp-')
}

/**
 * Gets the parent path for an MTP path.
 * Returns the storage root if already at root.
 */
export function getMtpParentPath(path: string): string | null {
    const parsed = parseMtpPath(path)
    if (!parsed) return null

    if (!parsed.path || parsed.path === '/') {
        // Already at storage root, no parent
        return null
    }

    const lastSlash = parsed.path.lastIndexOf('/')
    const parentInnerPath = lastSlash > 0 ? parsed.path.slice(0, lastSlash) : ''

    return constructMtpPath(parsed.deviceId, parsed.storageId, parentInnerPath)
}

/**
 * Joins an MTP path with a child folder name.
 */
export function joinMtpPath(basePath: string, childName: string): string {
    const parsed = parseMtpPath(basePath)
    if (!parsed) return basePath

    const newPath = parsed.path ? `${parsed.path}/${childName}` : childName
    return constructMtpPath(parsed.deviceId, parsed.storageId, newPath)
}

/**
 * Gets the display path for an MTP path (the path within the storage).
 * Returns "/" for storage root.
 */
export function getMtpDisplayPath(path: string): string {
    const parsed = parseMtpPath(path)
    if (!parsed) return path
    return parsed.path ? `/${parsed.path}` : '/'
}
