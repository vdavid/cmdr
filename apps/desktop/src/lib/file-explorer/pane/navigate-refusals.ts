/**
 * The synchronous refusals `navigate()` answers before it commits anything: the
 * refusal shape, the fixed refusal strings, and the MTP, ADB, and server capability
 * checks of the in-place arm. Split out of `navigate.ts` for length; the strings are
 * contract (L12), byte-pinned by `navigate.refusals.test.ts`.
 */
import { isAdbVolumeId } from '$lib/adb/adb-path-utils'
import { isServerPath, isServerVolumeId } from '$lib/servers/server-path-utils'

/** Why a synchronous navigation refused. `message` is the exact current string — contract (L12). */
export interface NavigateRefusal {
  kind:
    | 'on-network-volume'
    | 'smb-path-unsupported'
    | 'mtp-unconnected'
    | 'adb-unconnected'
    | 'server-unconnected'
    | 'pane-unavailable'
    | 'no-volume-resolved'
  /** EXACT current refusal string, forwarded verbatim as the `mcp-response` error. Pinned byte-for-byte. */
  message: string
}

/** The one `NavigateDeps` read the ADB and server checks need, taken as a parameter so this module imports nothing back. */
interface VolumePathLookup {
  getVolumePathById: (volumeId: string) => string | undefined
}

/** Exact refusal strings — contract (L12). Pinned byte-for-byte by the navigate suites. */
export function onNetworkRefusal(volumeLabel: string): NavigateRefusal {
  return {
    kind: 'on-network-volume',
    message: `Pane is on the ${volumeLabel} volume. Use select_volume to switch to a local volume first.`,
  }
}

export const PANE_UNAVAILABLE_REFUSAL: NavigateRefusal = { kind: 'pane-unavailable', message: 'Pane not available' }

/** The one navigable path on the virtual `network` volume: its host list. */
export const NETWORK_VOLUME_PATH = 'smb://'

/**
 * `resolve_location` maps EVERY `smb://` path to the virtual `network` volume, whose
 * state is a host and a share list rather than a path, so only the `smb://` sentinel
 * above is navigable. Anything longer used to commit the switch and report success
 * while the pane sat on the host list, which is a worse answer than saying so.
 */
export const SMB_PATH_REFUSAL: NavigateRefusal = {
  kind: 'smb-path-unsupported',
  message:
    "nav_to_path doesn't take smb:// paths. A mounted share is its own volume, so use select_volume with the name from cmdr://state volumes; for a share that isn't mounted, use select_volume Network and open the host.",
}

/**
 * MTP capability check. Returns a refusal or `null`. Note the em dash in the
 * first string — it's contract (L12), byte-pinned by `navigate.refusals.test.ts`.
 */
export function validateMtpNavigation(
  path: string,
  volumeId: string,
  volumeName: string | undefined,
): NavigateRefusal | null {
  if (path.startsWith('mtp://')) {
    const mtpMatch = path.match(/^mtp:\/\/([^/]+)\/(\d+)/)
    const pathDeviceId = mtpMatch?.[1]
    const pathStorageId = mtpMatch?.[2]
    if (!pathDeviceId || !pathStorageId || volumeId !== `${pathDeviceId}:${pathStorageId}`) {
      return { kind: 'mtp-unconnected', message: `Pane is not on this MTP volume — call select_volume first.` }
    }
  } else if (volumeId.includes(':') && volumeId.startsWith('mtp-')) {
    return {
      kind: 'mtp-unconnected',
      message: `Pane is on the ${volumeName ?? volumeId} MTP volume. Use select_volume to switch to a local volume first.`,
    }
  }
  return null
}

/**
 * ADB capability check, the MTP twin: an `adb://<serial>/…` path is navigable only
 * while the pane sits on that device's volume. Returns a refusal or `null`.
 */
export function validateAdbNavigation(
  deps: VolumePathLookup,
  path: string,
  volumeId: string,
  volumeName: string | undefined,
): NavigateRefusal | null {
  if (path.startsWith('adb://')) {
    // The volume id (`adb-<slug>-<digest>`) isn't derivable from the serial; the
    // volume's registered root (`adb://<serial>`) is the link.
    const serial = /^adb:\/\/([^/]+)/.exec(path)?.[1]
    if (!serial || !isAdbVolumeId(volumeId) || deps.getVolumePathById(volumeId) !== `adb://${serial}`) {
      return { kind: 'adb-unconnected', message: 'Pane is not on this ADB volume. Call select_volume first.' }
    }
  } else if (isAdbVolumeId(volumeId)) {
    return {
      kind: 'adb-unconnected',
      message: `Pane is on the ${volumeName ?? volumeId} ADB volume. Use select_volume to switch to a local volume first.`,
    }
  }
  return null
}

/**
 * Server capability check, the ADB twin for an SFTP or WebDAV place: a scheme
 * path is navigable only while the pane sits on the volume rooted at or above
 * it. Returns a refusal or `null`.
 *
 * ❗ The test is "is the target under the pane volume's OWN root", by whole path
 * components, ❌ never a string prefix: two servers can both hold `/srv/data`,
 * and `/srv/data-1` is a legal sibling of `/srv/data` that a string compare
 * would accept and then ask the wrong server for. The Rust twin is
 * `cmdr_fs::volume::remote_paths::RemoteRoot::to_remote_path`, which refuses the
 * same three shapes for the same reason.
 */
export function validateServerNavigation(
  deps: VolumePathLookup,
  path: string,
  volumeId: string,
  volumeName: string | undefined,
): NavigateRefusal | null {
  if (isServerPath(path)) {
    const volumeRoot = isServerVolumeId(volumeId) ? deps.getVolumePathById(volumeId) : undefined
    if (!volumeRoot || !isUnderServerRoot(volumeRoot, path)) {
      return { kind: 'server-unconnected', message: 'Pane is not on this server volume. Call select_volume first.' }
    }
  } else if (isServerVolumeId(volumeId)) {
    return {
      kind: 'server-unconnected',
      message: `Pane is on the ${volumeName ?? volumeId} server volume. Use select_volume to switch to a local volume first.`,
    }
  }
  return null
}

/** Whether `path` is the volume root or sits under it, matched by whole components. */
function isUnderServerRoot(volumeRoot: string, path: string): boolean {
  return path === volumeRoot || path.startsWith(`${volumeRoot}/`)
}
