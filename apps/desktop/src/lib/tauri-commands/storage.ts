// Volume management, space, and permissions

import type { VolumeInfo } from '../file-explorer/types'
import type { TimedOut } from './ipc-types'
import { getAppLogger } from '$lib/logging/logger'
import { commands } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

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
    return (await commands.listVolumes()) as TimedOut<VolumeInfo[]>
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
    await commands.refreshVolumes()
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
    return await commands.getDefaultVolumeId()
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
    return (await commands.resolvePathVolume(path)) as PathVolumeResolution
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
    return await commands.getVolumeSpace(path)
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
  await commands.watchVolumeSpace(watcherId, volumeId, path)
}

/**
 * Stops live disk-space monitoring for this watcher. Other watchers on the
 * same volume are unaffected.
 */
export async function unwatchVolumeSpace(watcherId: string): Promise<void> {
  await commands.unwatchVolumeSpace(watcherId)
}

/**
 * Updates the disk space change threshold at runtime (in MB).
 */
export async function setDiskSpaceThreshold(mb: number): Promise<void> {
  await commands.setDiskSpaceThreshold(Math.round(mb))
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
    return await commands.checkFullDiskAccess()
  } catch {
    // Command not available (non-macOS) - assume we have access
    return true
  }
}

/**
 * Returns the current set of TCC-restricted paths (sorted, absolute). Used by
 * `$lib/stores/restricted-paths-store.svelte.ts` to hydrate the in-memory set
 * before the first `restricted-paths-changed` event arrives.
 */
export async function getRestrictedPaths(): Promise<string[]> {
  return commands.getRestrictedPaths()
}

/**
 * Returns the macOS major version (e.g. 14 for Sonoma). Returns 0 on non-macOS
 * platforms or if the command is unavailable.
 */
export async function getMacosMajorVersion(): Promise<number> {
  try {
    return await commands.getMacosMajorVersion()
  } catch {
    return 0
  }
}

/**
 * Opens the system privacy settings.
 * On macOS, opens System Settings > Privacy & Security. Not applicable on Linux.
 */
export async function openPrivacySettings(): Promise<void> {
  try {
    const res = await commands.openPrivacySettings()
    if (res.status === 'error') throwIpcError(res.error)
  } catch {
    // Command not available (non-macOS) - silently fail
  }
}

/**
 * Opens an `x-apple.systempreferences:` deep link via `open(1)` on macOS.
 *
 * Used by friendly-error markdown links (e.g. iCloud TCC hint) that point at
 * specific System Settings panes. We don't go through `openExternalUrl` because
 * the Tauri opener plugin's default URL allowlist (http/https/mailto/tel) rejects
 * the `x-apple.systempreferences:` scheme silently. The Rust-side command also
 * validates the scheme, so passing arbitrary URLs through here is safe.
 */
export async function openSystemSettingsUrl(url: string): Promise<void> {
  try {
    const res = await commands.openSystemSettingsUrl(url)
    if (res.status === 'error') throwIpcError(res.error)
  } catch {
    // Command not available (non-macOS) or URL rejected — silently fail
  }
}

/** Opens the system appearance settings. On macOS, opens System Settings > Appearance. On Linux, opens the DE-specific appearance settings. */
export async function openAppearanceSettings(): Promise<void> {
  try {
    const res = await commands.openAppearanceSettings()
    if (res.status === 'error') throwIpcError(res.error)
  } catch (error) {
    log.warn('Failed to open appearance settings: {error}', { error })
  }
}
