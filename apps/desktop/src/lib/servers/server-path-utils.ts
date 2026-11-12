/**
 * Reading and writing the paths a remote place addresses its files by, the way
 * `adb-path-utils.ts` does for a phone.
 *
 * Path format: `<protocol>://<user>@<host>:<port>/<server path>`
 * Examples:
 *   - `sftp://test_user_1@testnas.local:22` (the volume's root)
 *   - `sftp://test_user_1@testnas.local:22/srv/data/photos` (a folder on the server)
 *   - `webdav://test_user_1@testnas.local:5006/remote.php/dav/files/test_user_1`
 *   - `s3://AKIAEXAMPLE@s3.eu-west-1.amazonaws.com:443/photos/2026` (the account is
 *     the access key id; the first segment is the bucket)
 *   - `nfs://testnas.local:2049/volume1%2Fdata/photos` (no account: the first segment is
 *     the EXPORT, escaped into one segment; § "NFS" below)
 *
 * ❗ **The prefix is what makes a remote path self-describing.** Rust's mount
 * table answers the LOCAL root for any absolute path it doesn't recognize, on
 * both platforms, so a bare `/srv/data/photos` resolves to the boot disk at every
 * resolver site the app has. The scheme is the same convention `mtp://` and
 * `adb://` already carry.
 *
 * ❗ **The prefix is spelled exactly as Rust mints it**
 * (`cmdr_fs::volume::ids::sftp_app_root` / `webdav_app_root` / `s3_app_root` / `nfs_app_root`): the host folded to
 * lowercase, the account left alone, the port literal. A path that folds more or
 * less than that misses the volume its own id names. The translation between
 * this spelling and the server's own is `cmdr_fs::volume::remote_paths`.
 */

import { volumeScheme } from '#lib/volume-scheme.js'

/** The protocols that address a place by a scheme path. */
export type ServerPathProtocol = 'sftp' | 'webdav' | 's3' | 'nfs'

/** The account half of a server path: everything up to the port. */
export interface ServerAccount {
  protocol: Exclude<ServerPathProtocol, 'nfs'>
  /** The account on the server. ❗ Part of the identity, and NOT case-folded. */
  username: string
  /** The host, folded to lowercase the way Rust folds it. */
  host: string
  port: number
}

/**
 * An NFS export, the root of an NFS place. ❗ No account: the uid it acts as is a
 * setting, ❌ not identity, so it's nowhere in the path.
 */
export interface NfsExportRoot {
  protocol: 'nfs'
  /** The host, folded to lowercase the way Rust folds it. */
  host: string
  /** The canonical NFS port: the one the address named, else 2049 (`nfsAppRoot`). */
  port: number
  /** The export, canonical (`/volume1/data`, the root export `/`). */
  export: string
}

/** What every path on one place starts with: an account's prefix, or an NFS export's. */
export type ServerRoot = ServerAccount | NfsExportRoot

/** A server path, split into its root and the server-side path under it. */
export type ParsedServerPath = ServerRoot & {
  /** The path under the root (an NFS export's folder), no leading slash; empty string at the root. */
  path: string
}

const SERVER_SCHEMES: ServerPathProtocol[] = ['sftp', 'webdav', 's3', 'nfs']

/**
 * `<protocol>://<user>@<host>:<port>`, with the path left to the caller.
 *
 * ❗ It matches the PREFIX only, ending on a lookahead, so the remainder of the
 * string IS the server path and no optional capture group has to stand in for
 * "no path at all". The account and host are matched as "anything but the
 * delimiter" rather than a character class: a username can hold almost anything,
 * and a host can be an IPv4 literal or an mDNS name. The port is digits, so a
 * hostless spelling can't slip through as one.
 *
 * ❗ It splits the authority exactly where `cmdr_fs::volume::ids::server_of_path`
 * does: the account is everything before the LAST `@`, the port everything after
 * the LAST `:`. The account takes `@` (an email login) and backtracks to the last
 * one, since the host can't hold an `@`; the host takes `:`, so an IPv6 literal
 * (`sftp://test_user_1@::1:22`) still splits.
 */
const SERVER_PATH_RE = /^(sftp|webdav|s3):\/\/([^/]+)@([^@/]+):(\d{1,5})(?=\/|$)/

/**
 * `nfs://<host>:<port>/<export segment>`, with the folder left to the caller.
 *
 * ❗ `server_of_path`'s NFS half, split the same way: the port after the
 * authority's LAST `:` (so an IPv6 host still splits), then ONE segment that is
 * the export, required. The segment's escapes are checked by
 * `nfsExportOfSegment`, ❌ not here.
 */
const NFS_PATH_RE = /^nfs:\/\/([^/]+):(\d{1,5})\/([^/]+)(?=\/|$)/

/** The NFS port a place carries when its address named none, as `nfs_volume_id`'s caller passes it. */
export const NFS_DEFAULT_PORT = 2049

/** Whether a path is on one of the server schemes (`sftp://`, `webdav://`, `s3://`, or `nfs://`). */
export function isServerPath(path: string): boolean {
  return SERVER_SCHEMES.some((scheme) => path.startsWith(`${scheme}://`))
}

/**
 * Whether a volume id names a server place (`sftp-…` / `webdav-…` / `s3-…` / `nfs-…`, from
 * `cmdr_fs::volume::ids`).
 *
 * ❗ An id, not a path: the switcher holds rows whose id is all it has, and a
 * `saved` row's id is the same one the live volume gets.
 */
export function isServerVolumeId(volumeId: string): boolean {
  return serverProtocolOfVolumeId(volumeId) !== null
}

/**
 * Whether `path` is `root` or a folder inside it, by whole components: a share's
 * mount at `/Volumes/testshare` doesn't hold `/Volumes/testshare-1`.
 */
export function isAtOrUnder(path: string, root: string): boolean {
  return path === root || path.startsWith(root.endsWith('/') ? root : `${root}/`)
}

/**
 * Which protocol a server volume id names, or `null` when it names no server.
 *
 * ❗ The id's own prefix is the ONLY thing a caller with nothing but an id can
 * read a protocol off, and it is a real answer: `cmdr_fs::volume::ids` mints it.
 * ❌ Don't default to one when the id doesn't say — a WebDAV place described as
 * SFTP puts the wrong word in the sheet's header and in the refusal under it.
 */
export function serverProtocolOfVolumeId(volumeId: string): ServerPathProtocol | null {
  const scheme = volumeScheme(volumeId)
  switch (scheme) {
    case 'sftp':
    case 'webdav':
    case 's3':
    case 'nfs':
      return scheme
    // An SMB share isn't a server place: its session is a mount, which
    // `disconnectPlace` doesn't speak.
    case 'smb':
    case 'root':
    case 'local':
    case 'path':
    case 'mtp':
    case 'adb':
    case 'cloud':
    case 'favorite':
    case 'unknown':
      return null
    default: {
      const unhandled: never = scheme
      return unhandled
    }
  }
}

/**
 * Splits a server path into its parts, or `null` when it isn't one.
 *
 * ❌ Refuses a bare server-absolute path (`/srv/data`) rather than guessing an
 * account for it: guessing is how a request reaches the wrong server.
 */
export function parseServerPath(path: string): ParsedServerPath | null {
  if (path.startsWith('nfs://')) return parseNfsPath(path)
  const match = SERVER_PATH_RE.exec(path)
  if (!match) return null
  const [, protocol, username, host, rawPort] = match
  // The pattern matches the PREFIX only (a lookahead keeps the delimiter out of
  // it), so what is left is the server-side path, empty at the root.
  const rest = path.slice(match[0].length)
  const port = Number(rawPort)
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null
  return {
    // The pattern's alternation is these three, and never `nfs`.
    protocol: protocol as ServerAccount['protocol'],
    username,
    host: host.toLowerCase(),
    port,
    path: trimSlashes(rest),
  }
}

/**
 * `parseServerPath`'s NFS half. ❗ For an APP path only, the one `nfsAppRoot`
 * minted: a person's `nfs://nas/volume1/data/photos` is an ADDRESS (it has no
 * port, and its export isn't escaped), and reading it here would name export
 * `/volume1` and folder `data/photos`. Go to path and ⌘K hand one to the add
 * sheet, and the backend splits it by the server's own export list.
 */
function parseNfsPath(path: string): ParsedServerPath | null {
  const match = NFS_PATH_RE.exec(path)
  if (!match) return null
  const [, host, rawPort, segment] = match
  const port = Number(rawPort)
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null
  const exportPath = nfsExportOfSegment(segment)
  if (exportPath === null) return null
  return {
    protocol: 'nfs',
    host: host.toLowerCase(),
    port,
    export: exportPath,
    path: trimSlashes(path.slice(match[0].length)),
  }
}

/**
 * An NFS place's app root: `nfs://<host>:<port>/<export segment>`, exactly as
 * `cmdr_fs::volume::nfs_app_root` mints it. The host folded, the port canonical
 * (2049 for an address that named none, ❌ never the port portmap answered,
 * which can move), the export canonical and then escaped into ONE segment.
 */
export function nfsAppRoot(root: { host: string; port: number | null; export: string }): string {
  const port = root.port ?? NFS_DEFAULT_PORT
  return `nfs://${root.host.toLowerCase()}:${String(port)}/${nfsExportSegment(root.export)}`
}

/**
 * The one spelling of an export, `cmdr_fs::volume::canonical_nfs_export`'s:
 * absolute, no empty, `.`, or `..` segment, no trailing `/`, the root export `/`.
 * Otherwise the server's bytes: neither case nor Unicode normalization is folded.
 */
export function canonicalNfsExport(exportPath: string): string {
  return normalizeServerPath(exportPath)
}

/** The export as one segment: `%` → `%25`, `/` → `%2F`, the leading `/` dropped, the root export `%2F`. */
function nfsExportSegment(exportPath: string): string {
  const canonical = canonicalNfsExport(exportPath)
  if (canonical === '/') return '%2F'
  return canonical.slice(1).replaceAll('%', '%25').replaceAll('/', '%2F')
}

/**
 * The export a segment spells, or `null` for one no root mints: any escape but
 * `%25` and `%2F` (uppercase, as Rust matches them) refuses it.
 */
function nfsExportOfSegment(segment: string): string | null {
  if (segment === '%2F') return '/'
  let exportPath = '/'
  for (let at = 0; at < segment.length; at++) {
    if (segment[at] !== '%') {
      exportPath += segment[at]
      continue
    }
    const escape = segment.slice(at, at + 3)
    if (escape === '%25') exportPath += '%'
    else if (escape === '%2F') exportPath += '/'
    else return null
    at += 2
  }
  return canonicalNfsExport(exportPath)
}

/** A server-side path made absolute, with empty and `.` segments dropped and `..` resolved lexically. */
function normalizeServerPath(path: string): string {
  const parts: string[] = []
  for (const part of path.split('/')) {
    if (part === '' || part === '.') continue
    if (part === '..') parts.pop()
    else parts.push(part)
  }
  return `/${parts.join('/')}`
}

/**
 * The `host:port` a server path dials, for a person to read, or `null` off the
 * server schemes. An IPv6 literal is bracketed so its port reads as a port.
 *
 * What a pane waiting on a place shows under its name: a saved name says nothing
 * about whether it points at a LAN address or a VPN one (`ERR-VZ4CV`).
 */
export function serverAddressOf(path: string): string | null {
  const parsed = parseServerPath(path)
  if (!parsed) return null
  const host = parsed.host.includes(':') ? `[${parsed.host}]` : parsed.host
  return `${host}:${String(parsed.port)}`
}

/**
 * The root's app root, with no path: `<protocol>://<user>@<host>:<port>` for an
 * account, `nfs://<host>:<port>/<export segment>` for an NFS export.
 */
export function serverAppRoot(root: ServerRoot): string {
  if (root.protocol === 'nfs') return nfsAppRoot(root)
  return `${root.protocol}://${root.username}@${root.host.toLowerCase()}:${String(root.port)}`
}

/**
 * Builds a server path from a root and a path under it (absolute or relative).
 * The three root spellings (`''`, `'/'`, `'.'`) all answer the root.
 */
export function constructServerPath(account: ServerRoot, path = ''): string {
  const root = serverAppRoot(account)
  const trimmed = trimSlashes(path)
  return trimmed === '' || trimmed === '.' ? root : `${root}/${trimmed}`
}

/**
 * The parent of a server path, or `null` at the volume root and off-scheme.
 *
 * ❗ Never walks above the root: `sftp:/` is not a path, and a walk that produced
 * one would hand the local mount table a string it happily answers for.
 */
export function getServerParentPath(path: string): string | null {
  const parsed = parseServerPath(path)
  if (!parsed || parsed.path === '') return null
  const lastSlash = parsed.path.lastIndexOf('/')
  return constructServerPath(parsed, lastSlash > 0 ? parsed.path.slice(0, lastSlash) : '')
}

/** Joins a server path with a child name. Off-scheme paths pass through. */
export function joinServerPath(basePath: string, childName: string): string {
  const parsed = parseServerPath(basePath)
  if (!parsed) return basePath
  return constructServerPath(parsed, parsed.path ? `${parsed.path}/${childName}` : childName)
}

/**
 * The server-side spelling of a server path: what someone signed in to that
 * server would type. `/` at the root, and an NFS path with its export decoded in
 * front (`/volume1/data/photos`). Any other path passes through unchanged.
 */
export function getServerDisplayPath(path: string): string {
  const parsed = parseServerPath(path)
  if (!parsed) return path
  if (parsed.protocol === 'nfs') return canonicalNfsExport(`${parsed.export}/${parsed.path}`)
  return parsed.path ? `/${parsed.path}` : '/'
}

/**
 * Whether `path` is `root` or sits under it, by whole `/`-separated components.
 *
 * ❗ The twin of Rust's `server_volumes::path_is_under`, trailing-slash trim
 * included: an S3 account root's app root is `s3://<key>@<host>:<port>/`, and a
 * naive `${root}/` prefix would refuse every bucket under it. ❌ Never a raw
 * string prefix: `/srv/data-1` is a legal sibling of `/srv/data`.
 */
export function isUnderServerRoot(root: string, path: string): boolean {
  const trimmed = root.replace(/\/+$/, '')
  return path === trimmed || path.startsWith(`${trimmed}/`)
}

/** Strips leading and trailing slashes, so every caller holds one spelling. */
function trimSlashes(path: string): string {
  return path.replace(/^\/+/, '').replace(/\/+$/, '')
}
