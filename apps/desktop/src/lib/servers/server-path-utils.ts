/**
 * Reading and writing the paths a remote place addresses its files by, the way
 * `adb-path-utils.ts` does for a phone.
 *
 * Path format: `<protocol>://<user>@<host>:<port>/<server path>`
 * Examples:
 *   - `sftp://ada@nas.local:22` (the volume's root)
 *   - `sftp://ada@nas.local:22/srv/data/photos` (a folder on the server)
 *   - `webdav://ada@nas.local:5006/remote.php/dav/files/ada`
 *
 * ❗ **The prefix is what makes a remote path self-describing.** Rust's mount
 * table answers the LOCAL root for any absolute path it doesn't recognize, on
 * both platforms, so a bare `/srv/data/photos` resolves to the boot disk at every
 * resolver site the app has. The scheme is the same convention `mtp://` and
 * `adb://` already carry.
 *
 * ❗ **The prefix is spelled exactly as Rust mints it**
 * (`cmdr_fs::volume::ids::sftp_app_root` / `webdav_app_root`): the host folded to
 * lowercase, the account left alone, the port literal. A path that folds more or
 * less than that misses the volume its own id names. The translation between
 * this spelling and the server's own is `cmdr_fs::volume::remote_paths`.
 */

/** The two protocols that address a place by a scheme path. */
export type ServerPathProtocol = 'sftp' | 'webdav'

/** The account half of a server path: everything up to the port. */
export interface ServerAccount {
  protocol: ServerPathProtocol
  /** The account on the server. ❗ Part of the identity, and NOT case-folded. */
  username: string
  /** The host, folded to lowercase the way Rust folds it. */
  host: string
  port: number
}

/** A server path, split into the account and the server-side path under it. */
export interface ParsedServerPath extends ServerAccount {
  /** The path within the server, no leading slash; empty string at the root. */
  path: string
}

const SERVER_SCHEMES: ServerPathProtocol[] = ['sftp', 'webdav']

/**
 * `<protocol>://<user>@<host>:<port>`, with the path left to the caller.
 *
 * ❗ It matches the PREFIX only, ending on a lookahead, so the remainder of the
 * string IS the server path and no optional capture group has to stand in for
 * "no path at all". The account and host are matched as "anything but the
 * delimiter" rather than a character class: a username can hold almost anything,
 * and a host can be an IPv4 literal or an mDNS name. The port is digits, so a
 * hostless spelling can't slip through as one.
 */
const SERVER_PATH_RE = /^(sftp|webdav):\/\/([^@/]+)@([^@/:]+):(\d{1,5})(?=\/|$)/

/** Whether a path is on one of the server schemes (`sftp://` or `webdav://`). */
export function isServerPath(path: string): boolean {
  return SERVER_SCHEMES.some((scheme) => path.startsWith(`${scheme}://`))
}

/**
 * Whether a volume id names a server place (`sftp-…` / `webdav-…`, from
 * `cmdr_fs::volume::ids`).
 *
 * ❗ An id, not a path: the switcher holds rows whose id is all it has, and a
 * `saved` row's id is the same one the live volume gets.
 */
export function isServerVolumeId(volumeId: string): boolean {
  return SERVER_SCHEMES.some((scheme) => volumeId.startsWith(`${scheme}-`))
}

/**
 * Splits a server path into its parts, or `null` when it isn't one.
 *
 * ❌ Refuses a bare server-absolute path (`/srv/data`) rather than guessing an
 * account for it: guessing is how a request reaches the wrong server.
 */
export function parseServerPath(path: string): ParsedServerPath | null {
  const match = SERVER_PATH_RE.exec(path)
  if (!match) return null
  const [, protocol, username, host, rawPort] = match
  // The pattern matches the PREFIX only (a lookahead keeps the delimiter out of
  // it), so what is left is the server-side path, empty at the root.
  const rest = path.slice(match[0].length)
  const port = Number(rawPort)
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null
  return {
    protocol: protocol as ServerPathProtocol,
    username,
    host: host.toLowerCase(),
    port,
    path: trimSlashes(rest),
  }
}

/** The account's app root: `<protocol>://<user>@<host>:<port>`, with no path. */
export function serverAppRoot(account: ServerAccount): string {
  return `${account.protocol}://${account.username}@${account.host.toLowerCase()}:${String(account.port)}`
}

/**
 * Builds a server path from an account and a path on the server (absolute or
 * relative). The three root spellings (`''`, `'/'`, `'.'`) all answer the root.
 */
export function constructServerPath(account: ServerAccount, path = ''): string {
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
 * server would type. `/` at the root. Any other path passes through unchanged.
 */
export function getServerDisplayPath(path: string): string {
  const parsed = parseServerPath(path)
  if (!parsed) return path
  return parsed.path ? `/${parsed.path}` : '/'
}

/** Strips leading and trailing slashes, so every caller holds one spelling. */
function trimSlashes(path: string): string {
  return path.replace(/^\/+/, '').replace(/\/+$/, '')
}
