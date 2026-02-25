// Volume management, space, permissions (macOS only)

import { invoke } from '@tauri-apps/api/core'
import type { VolumeInfo } from '../file-explorer/types'

/** Default volume ID for the root filesystem */
export const DEFAULT_VOLUME_ID = 'root'

/**
 * Lists all mounted volumes.
 * Only available on macOS.
 * @returns Array of VolumeInfo objects, sorted with root first
 */
export async function listVolumes(): Promise<VolumeInfo[]> {
    try {
        return await invoke<VolumeInfo[]>('list_volumes')
    } catch {
        // Command not available (non-macOS) - return empty array
        return []
    }
}

/**
 * Gets the default volume ID (root filesystem).
 * @returns The default volume ID string
 */
export async function getDefaultVolumeId(): Promise<string> {
    try {
        return await invoke<string>('get_default_volume_id')
    } catch {
        // Fallback for non-macOS
        return DEFAULT_VOLUME_ID
    }
}

/**
 * Finds the actual volume (not a favorite) that contains a given path.
 * This is used to determine which volume to set as active when the user navigates to a "favorite folder".
 * @param path - Path to find the containing volume for
 * @returns The VolumeInfo for the containing volume, or null if not found
 */
export async function findContainingVolume(path: string): Promise<VolumeInfo | null> {
    try {
        return await invoke<VolumeInfo | null>('find_containing_volume', { path })
    } catch {
        // Command not available (non-macOS) - return null
        return null
    }
}

/** Space information for a volume. */
export interface VolumeSpaceInfo {
    totalBytes: number
    availableBytes: number
}

/**
 * Gets space information (total and available bytes) for a volume at the given path.
 * @param path - Any path on the volume to get space info for
 * @returns Space info or null if unavailable
 */
export async function getVolumeSpace(path: string): Promise<VolumeSpaceInfo | null> {
    try {
        return await invoke<VolumeSpaceInfo | null>('get_volume_space', { path })
    } catch {
        // Command not available (non-macOS) - return null
        return null
    }
}

// ============================================================================
// Permission checking (macOS only)
// ============================================================================

/**
 * Checks if the app has full disk access.
 * Only available on macOS.
 * @returns True if the app has FDA, false otherwise
 */
export async function checkFullDiskAccess(): Promise<boolean> {
    try {
        return await invoke<boolean>('check_full_disk_access')
    } catch {
        // Command not available (non-macOS) - assume we have access
        return true
    }
}

/**
 * Opens System Settings > Privacy & Security > Privacy.
 * Only available on macOS.
 */
export async function openPrivacySettings(): Promise<void> {
    try {
        await invoke('open_privacy_settings')
    } catch {
        // Command not available (non-macOS) - silently fail
    }
}

/** Opens System Settings > Appearance. Only available on macOS. */
export async function openAppearanceSettings(): Promise<void> {
    try {
        await invoke('open_appearance_settings')
    } catch {
        // Command not available (non-macOS) - silently fail
    }
}
