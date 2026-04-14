// Volume management, space, and permissions

import { invoke } from '@tauri-apps/api/core'
import type { VolumeInfo } from '../file-explorer/types'
import type { TimedOut } from './ipc-types'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('storage')

/** Default volume ID for the root filesystem */
export const DEFAULT_VOLUME_ID = 'root'

/**
 * Lists all mounted volumes.
 * Available on macOS and Linux.
 * @returns Array of VolumeInfo objects, sorted with root first
 */
export async function listVolumes(): Promise<TimedOut<VolumeInfo[]>> {
  try {
    return await invoke<TimedOut<VolumeInfo[]>>('list_volumes')
  } catch {
    // Command not available (non-macOS) - return empty array
    return { data: [], timedOut: false }
  }
}

/**
 * Triggers a fresh `volumes-changed` broadcast from the backend.
 * The result arrives via the event, not as a return value.
 */
export async function refreshVolumes(): Promise<void> {
  try {
    await invoke('refresh_volumes')
  } catch {
    // Command not available — fall back to listVolumes (shouldn't happen)
    log.warn('refresh_volumes command not available')
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

/** Result of resolving a path to its containing volume via `statfs()`. */
export interface PathVolumeResolution {
  volume: VolumeInfo | null
  timedOut: boolean
}

/**
 * Resolves a path to its containing volume without enumerating all volumes.
 * Uses `statfs()` for local paths (<1ms), protocol dispatch for MTP/SMB.
 * @param path - The path to resolve
 * @returns The volume resolution result
 */
export async function resolvePathVolume(path: string): Promise<PathVolumeResolution> {
  try {
    return await invoke<PathVolumeResolution>('resolve_path_volume', { path })
  } catch {
    // Command not available — return no volume, not timed out
    return { volume: null, timedOut: false }
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
export async function getVolumeSpace(path: string): Promise<TimedOut<VolumeSpaceInfo | null>> {
  try {
    return await invoke<TimedOut<VolumeSpaceInfo | null>>('get_volume_space', { path })
  } catch {
    // Command not available (non-macOS) - return null
    return { data: null, timedOut: false }
  }
}

// ============================================================================
// Disk space polling
// ============================================================================

/**
 * Registers a watcher for live disk-space monitoring.
 * The backend will poll this volume and emit `volume-space-changed` events
 * when available space changes beyond the configured threshold.
 *
 * @param watcherId - Unique ID for this watcher (typically the pane ID).
 *   Multiple watchers can watch the same volume independently.
 */
export async function watchVolumeSpace(watcherId: string, volumeId: string, path: string): Promise<void> {
  await invoke('watch_volume_space', { watcherId, volumeId, path })
}

/**
 * Stops live disk-space monitoring for this watcher. Other watchers on the
 * same volume are unaffected.
 */
export async function unwatchVolumeSpace(watcherId: string): Promise<void> {
  await invoke('unwatch_volume_space', { watcherId })
}

/**
 * Updates the disk space change threshold at runtime (in MB).
 */
export async function setDiskSpaceThreshold(mb: number): Promise<void> {
  await invoke('set_disk_space_threshold', { mb: Math.round(mb) })
}

// ============================================================================
// Permission checking
// ============================================================================

/**
 * Checks if the app has full disk access.
 * On macOS, checks the actual FDA status. On Linux, always returns true (no sandboxing).
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
 * Opens the system privacy settings.
 * On macOS, opens System Settings > Privacy & Security. Not applicable on Linux.
 */
export async function openPrivacySettings(): Promise<void> {
  try {
    await invoke('open_privacy_settings')
  } catch {
    // Command not available (non-macOS) - silently fail
  }
}

/** Opens the system appearance settings. On macOS, opens System Settings > Appearance. On Linux, opens the DE-specific appearance settings. */
export async function openAppearanceSettings(): Promise<void> {
  try {
    await invoke('open_appearance_settings')
  } catch (error) {
    log.warn('Failed to open appearance settings: {error}', { error })
  }
}
