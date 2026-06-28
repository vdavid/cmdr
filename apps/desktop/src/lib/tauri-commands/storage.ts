// Volume management, space, and permissions

import { type UnlistenFn } from '@tauri-apps/api/event'
import type { VolumeInfo } from '../file-explorer/types'
import type { TimedOut } from './ipc-types'
import { getAppLogger } from '$lib/logging/logger'
import {
  commands,
  events,
  type Location,
  type LowDiskSpacePayload,
  type ResolveLocationResult,
  type VolumeContextAction,
  type VolumesBusyChanged,
  type VolumeSpaceChanged,
  type VolumeUnmounted,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'
import { withTimeout } from '$lib/utils/timing'

export type { Location, ResolveLocationResult }

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
    // Command not available, fall back to listVolumes (shouldn't happen)
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
    // Command not available, return no volume, not timed out
    return { volume: null, timedOut: false }
  }
}

/** Frontend timeout for `resolveLocation`, the outer layer of the two-layer
 * timeout defense (the backend already caps the statfs at 2s). Slightly above
 * the backend cap so its honest `timedOut` flag wins when the filesystem hangs;
 * this only fires if the IPC channel itself stalls. */
const RESOLVE_LOCATION_TIMEOUT_MS = 3000

/**
 * Resolves a path into a `Location` (volume id + the path), the canonical
 * path→volume resolver for navigation edges. Wraps `resolve_location`, which
 * runs the full protocol dispatch (`mtp://` / `smb://` / local `statfs`), so it
 * resolves Cmdr's virtual paths too. `location: null` means no volume contains
 * the path; `timedOut: true` means the filesystem (or IPC) didn't respond.
 * @param path - The path to resolve
 * @returns The resolution result
 */
export async function resolveLocation(path: string): Promise<ResolveLocationResult> {
  try {
    return await withTimeout(commands.resolveLocation(path), RESOLVE_LOCATION_TIMEOUT_MS, {
      location: null,
      timedOut: true,
    })
  } catch {
    // Command not available (non-macOS/Linux): no volume, not timed out.
    return { location: null, timedOut: false }
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

/**
 * Ejects a mounted volume. Dispatches by kind: MTP devices close their USB
 * session, SMB shares run `diskutil unmount` (FSEvents handles the smb2
 * teardown), and physical or disk-image volumes run `diskutil eject` (powers
 * down USB devices, detaches DMGs).
 *
 * Resolves once the unmount or disconnect is initiated. The volume disappears
 * from the picker shortly after, via `volume-unmounted` or
 * `mtp-device-disconnected`. Throws an `IpcError`-shaped exception on failure
 * (e.g. "Resource busy" if Finder still has the volume open).
 */
export async function ejectVolume(volumeId: string): Promise<void> {
  const res = await commands.ejectVolume(volumeId)
  if (res.status === 'error') throwIpcError(res.error)
}

/**
 * Returns the IDs of volumes that currently have a copy / move / delete
 * operation reading from or writing to them. The volume picker bootstraps its
 * busy set from this, then keeps it live via the `volumes-busy-changed` event,
 * to disable Eject for a device while a transfer touches it.
 */
export async function getBusyVolumeIds(): Promise<string[]> {
  return commands.getBusyVolumeIds()
}

/** Volume-list-changed payload, with `data` exposed as the FE-wide `VolumeInfo` type. */
export interface VolumesChangedPayload {
  data: VolumeInfo[]
  timedOut: boolean
}

/**
 * Subscribes to backend-pushed volume-list updates. The wire payload is the typed
 * `tauri-specta` `VolumesChanged` event; `data` is re-typed to the FE-wide
 * `VolumeInfo` (same cast `listVolumes` applies, bridging the `LocationInfo`
 * null-vs-undefined optional shape). Call the returned `UnlistenFn` on destroy.
 */
export function onVolumesChanged(handler: (payload: VolumesChangedPayload) => void): Promise<UnlistenFn> {
  return events.volumesChanged.listen((event) => {
    handler(event.payload as VolumesChangedPayload)
  })
}

/**
 * Subscribes to per-volume unmount events. The handler receives the gone volume's
 * path so panes can redirect off it.
 * Call the returned `UnlistenFn` on component destroy to avoid leaks.
 */
export function onVolumeUnmounted(handler: (payload: VolumeUnmounted) => void): Promise<UnlistenFn> {
  return events.volumeUnmounted.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to busy-volume-set changes. The handler receives the sorted list of
 * volume IDs with an in-flight copy / move / delete operation.
 * Call the returned `UnlistenFn` on component destroy to avoid leaks.
 */
export function onVolumesBusyChanged(handler: (payload: VolumesBusyChanged) => void): Promise<UnlistenFn> {
  return events.volumesBusyChanged.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to volume-context-menu actions (currently just "eject") emitted by the
 * native breadcrumb context menu. The handler receives `{ action, volumeId, volumeName }`.
 * Returns an `UnlistenFn` — call it on component destroy to avoid leaks.
 */
export function onVolumeContextAction(handler: (payload: VolumeContextAction) => void): Promise<UnlistenFn> {
  return events.volumeContextAction.listen((event) => {
    handler(event.payload)
  })
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
 * Subscribes to live disk-space updates from the backend poller. The payload is
 * the typed `tauri-specta` event, so `volumeId` / `totalBytes` / `availableBytes`
 * are checked at compile time against the Rust `VolumeSpaceChanged` struct.
 * Call the returned `UnlistenFn` in `onDestroy` to avoid leaks.
 */
export async function onVolumeSpaceChanged(callback: (payload: VolumeSpaceChanged) => void): Promise<UnlistenFn> {
  return events.volumeSpaceChanged.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Subscribes to the backend low-disk-space warning. The payload is the typed
 * `tauri-specta` event, so `volumeId` / `freePercent` / `thresholdPercent` (and
 * the byte fields) are checked at compile time against the Rust `LowDiskSpacePayload`.
 * Call the returned `UnlistenFn` on component destroy to avoid leaks.
 */
export async function onLowDiskSpace(callback: (payload: LowDiskSpacePayload) => void): Promise<UnlistenFn> {
  return events.lowDiskSpace.listen((event) => {
    callback(event.payload)
  })
}

/**
 * Updates the disk space change threshold at runtime (in MB).
 */
export async function setDiskSpaceThreshold(mb: number): Promise<void> {
  await commands.setDiskSpaceThreshold(Math.round(mb))
}

/**
 * Updates the low-disk-space warning config at runtime. `enabled` registers or
 * removes the backend's permanent boot-volume watcher; `thresholdPercent` is
 * the free-space percent that trips the warning.
 */
export async function setLowDiskSpaceConfig(enabled: boolean, thresholdPercent: number): Promise<void> {
  await commands.setLowDiskSpaceConfig(enabled, Math.round(thresholdPercent))
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
 * Polls full disk access status without TCC-registration side effects.
 *
 * Unlike `checkFullDiskAccess`, this doesn't fire the `mmap` / `NSData` /
 * `read_dir` registration storm on a denial, so it's safe to call repeatedly.
 * The onboarding FDA step polls it every 500 ms to detect a same-session grant.
 * Use `checkFullDiskAccess` for the one-shot registration moments (it's the
 * call that gets Cmdr into the Full Disk Access list).
 * @returns True if the app has FDA, false otherwise
 */
export async function checkFullDiskAccessQuiet(): Promise<boolean> {
  try {
    return await commands.checkFullDiskAccessQuiet()
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
    // Command not available (non-macOS) or URL rejected. Silently fail.
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
