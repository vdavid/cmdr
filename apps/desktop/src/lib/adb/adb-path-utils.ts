/**
 * Utilities for parsing and constructing ADB paths, plus the device-scheme
 * predicates that treat MTP and ADB alike.
 *
 * ADB path format: adb://{serial}/{device path}
 * Examples:
 *   - adb://R58M12345 (the device's `/`)
 *   - adb://R58M12345/sdcard/DCIM (a folder on the device)
 *
 * Volume id format: `adb-{slug}-{digest}`. No colon, so `isMtpVolumeId` stays
 * false for it and the two families never shadow each other.
 *
 * Contract: `docs/specs/android-adb-backend.md` § "Volume contract".
 */

import { getMtpDisplayPath, isMtpVolumeId } from '$lib/mtp/mtp-path-utils'

const ADB_SCHEME = 'adb://'
const MTP_SCHEME = 'mtp://'

/** Parsed ADB path components. */
export interface ParsedAdbPath {
  serial: string
  /** Path within the device (no leading slash; empty string for the device root). */
  path: string
}

/**
 * Parses an ADB path into its components.
 * @returns Parsed path, or null if not a valid ADB path.
 */
export function parseAdbPath(path: string): ParsedAdbPath | null {
  if (!path.startsWith(ADB_SCHEME)) return null
  const parts = path.slice(ADB_SCHEME.length).split('/')
  const serial = parts[0]
  if (!serial) return null
  return { serial, path: parts.slice(1).join('/') }
}

/** Constructs an ADB path from a serial and a device path (absolute or relative). */
export function constructAdbPath(serial: string, path: string = ''): string {
  const base = `${ADB_SCHEME}${serial}`
  if (!path || path === '/') return base
  const normalizedPath = path.startsWith('/') ? path.slice(1) : path
  return `${base}/${normalizedPath}`
}

/** Whether a volume id names an ADB volume (`adb-…`). */
export function isAdbVolumeId(volumeId: string): boolean {
  return volumeId.startsWith('adb-')
}

/** Whether a path is on the `adb://` scheme. */
export function isAdbPath(path: string): boolean {
  return path.startsWith(ADB_SCHEME)
}

/**
 * Gets the parent path for an ADB path.
 * Returns null when not an ADB path or already at the device root.
 */
export function getAdbParentPath(path: string): string | null {
  const parsed = parseAdbPath(path)
  if (!parsed || !parsed.path) return null
  const lastSlash = parsed.path.lastIndexOf('/')
  return constructAdbPath(parsed.serial, lastSlash > 0 ? parsed.path.slice(0, lastSlash) : '')
}

/** Joins an ADB path with a child folder name. */
export function joinAdbPath(basePath: string, childName: string): string {
  const parsed = parseAdbPath(basePath)
  if (!parsed) return basePath
  return constructAdbPath(parsed.serial, parsed.path ? `${parsed.path}/${childName}` : childName)
}

/**
 * Gets the display path for an ADB path (the absolute path on the device).
 * Returns "/" for the device root.
 */
export function getAdbDisplayPath(path: string): string {
  const parsed = parseAdbPath(path)
  if (!parsed) return path
  return parsed.path ? `/${parsed.path}` : '/'
}

/**
 * Whether a volume id names a device-anchored volume (MTP or ADB): not local,
 * no trash, no Finder reveal, no OS-visible path, no git over the transport.
 */
export function isDeviceVolumeId(volumeId: string): boolean {
  return isMtpVolumeId(volumeId) || isAdbVolumeId(volumeId)
}

/** Whether a path is on one of the device schemes (`mtp://` or `adb://`). */
export function isDeviceScheme(path: string): boolean {
  return path.startsWith(MTP_SCHEME) || path.startsWith(ADB_SCHEME)
}

/**
 * The display form of a device-scheme path: the path within the MTP storage or
 * on the ADB device, "/" at the root. Any other path passes through unchanged.
 */
export function getDeviceDisplayPath(path: string): string {
  if (path.startsWith(MTP_SCHEME)) return getMtpDisplayPath(path)
  if (path.startsWith(ADB_SCHEME)) return getAdbDisplayPath(path)
  return path
}
