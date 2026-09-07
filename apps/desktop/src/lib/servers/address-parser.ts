/**
 * One pasted string into a protocol and an endpoint, for add mode's address
 * field.
 *
 * ❗ **Address first, protocol second.** People have an address, not a protocol:
 * an `ssh` line off a wiki, a `user@host` from a colleague, a Nextcloud URL out
 * of a browser bar, an `smb://` off a Finder dialog. The sheet flips its
 * protocol `ToggleGroup` to whatever this answers and leaves it editable, so a
 * wrong read costs one click and a demand that the user classify their own NAS
 * costs the whole feature.
 *
 * ❗ **Pure, and the example table in `address-parser.test.ts` IS the contract**:
 * there is no property-testing library on the frontend, so a shape that reaches
 * the field and isn't in that table is a shape nobody decided.
 *
 * ❌ **Never guesses past what the string says.** An unrecognized shape answers
 * `unparsed` and the sheet leaves the toggle where it is; inventing a protocol
 * for `not a server!!` is how a password reaches the wrong port.
 */

import type { ServerProtocol } from '$lib/ipc/bindings'

/** What one address string turned out to name. Tagged, so `unparsed` is a case rather than a `null`. */
export type ParsedAddress =
  | {
      kind: 'parsed'
      protocol: ServerProtocol
      /** Folded to lowercase, the way `cmdr_fs::volume::ids` folds it. */
      host: string
      /** What the address named, or the protocol's default. */
      port: number
      /** ❗ NOT case-folded: a POSIX account may be case-sensitive. */
      username?: string
      /** The path the address named, leading slash kept, trailing slashes dropped. Absent at the root. */
      path?: string
      /** WebDAV only: whether the base URL is TLS. Absent for the other two. */
      secure?: boolean
    }
  | { kind: 'unparsed' }

/** The default port per endpoint kind, used whenever the address names none. */
const DEFAULT_PORTS = { sftp: 22, smb: 445, webdavSecure: 443, webdavPlain: 80 }

/**
 * Every scheme the field accepts, and what it means.
 *
 * `secure` is WebDAV's alone: `davs` and `dav` exist precisely to say which,
 * and a bare `webdav://` is read as TLS because defaulting the other way would
 * send a password in the clear on a typo.
 */
const SCHEMES: Partial<Record<string, { protocol: ServerProtocol; secure?: boolean }>> = {
  sftp: { protocol: 'sftp' },
  ssh: { protocol: 'sftp' },
  smb: { protocol: 'smb' },
  cifs: { protocol: 'smb' },
  webdav: { protocol: 'webdav', secure: true },
  davs: { protocol: 'webdav', secure: true },
  dav: { protocol: 'webdav', secure: false },
  https: { protocol: 'webdav', secure: true },
  http: { protocol: 'webdav', secure: false },
}

/**
 * A host label: letters, digits, dots, and dashes, starting and ending on an
 * alphanumeric. Covers a DNS name, an mDNS `.local`, and an IPv4 literal.
 *
 * ❗ An IPv6 literal is deliberately outside it. A remote path spells its
 * account as `<user>@<host>:<port>` on both sides of the wire
 * (`server-path-utils.ts` and `cmdr_fs::volume::remote_paths`), and neither can
 * carry the brackets, so accepting one here would mint a volume whose own paths
 * don't parse. Refusing says so at the field instead.
 */
const HOST_RE = /^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/

const UNPARSED: ParsedAddress = { kind: 'unparsed' }

/** Splits one pasted address into a protocol and an endpoint, or answers `unparsed`. */
export function parseServerAddress(input: string): ParsedAddress {
  const trimmed = input.trim()
  if (trimmed === '') return UNPARSED

  const sshLine = /^ssh\s+(.+)$/i.exec(trimmed)
  if (sshLine) return parseSshCommandLine(sshLine[1])

  const scheme = /^([A-Za-z][A-Za-z0-9+.-]*):\/\/(.*)$/.exec(trimmed)
  if (scheme) {
    const known = SCHEMES[scheme[1].toLowerCase()]
    // ❌ An unknown scheme is never demoted to "a host called ftp": `ftp://nas`
    // names a protocol Cmdr doesn't speak, and saying so beats dialing SMB.
    if (!known) return UNPARSED
    return readEndpoint(scheme[2], known.protocol, known.secure)
  }

  // No scheme. `user@host` names an ACCOUNT, and an account is what SFTP has;
  // a bare host names a machine, and SMB is the one protocol that browses one
  // with no account at all, so a wrong guess there asks the user for nothing.
  return readEndpoint(trimmed, trimmed.includes('@') ? 'sftp' : 'smb', undefined)
}

/**
 * A pasted `ssh` invocation: its `-p` flag, and its `[user@]host` target.
 *
 * The target is re-read through the `ssh://` scheme rather than as a bare
 * string, so `ssh nas.local` lands on SFTP instead of falling through to the
 * bare-host reading. Flags other than `-p` are dropped: `-i`, `-J`, and friends
 * describe a transport this app builds for itself.
 */
function parseSshCommandLine(rest: string): ParsedAddress {
  const tokens = rest.split(/\s+/).filter((token) => token !== '')
  let port: number | undefined
  let target: string | undefined

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i]
    if (token === '-p') {
      port = readPort(tokens[++i] ?? '')
      if (port === undefined) return UNPARSED
    } else if (token.startsWith('-p')) {
      port = readPort(token.slice(2))
      if (port === undefined) return UNPARSED
    } else if (!token.startsWith('-')) {
      target = token
    }
  }

  if (target === undefined) return UNPARSED
  const parsed = parseServerAddress(target.includes('://') ? target : `ssh://${target}`)
  if (parsed.kind === 'unparsed' || port === undefined) return parsed
  return { ...parsed, port }
}

/**
 * Splits an authority into the account and what's left, or `null` when the
 * userinfo names nobody.
 *
 * ❗ **A URL's userinfo may be `user:password`, and only the ACCOUNT survives.**
 * What this parse returns feeds the visible Username field, the volume id Rust
 * mints from `(host, port, username)`, and the Keychain scope keyed on it; a
 * password belongs in none of the three. The person retypes it into the password
 * field, the one place that treats it as a secret. Userinfo that is a password
 * with no account in front of it is refused rather than guessed at.
 */
function readAccount(authority: string): { username?: string; hostPort: string } | null {
  const at = authority.lastIndexOf('@')
  const hostPort = authority.slice(at + 1)
  if (at === -1) return { hostPort }
  const username = authority.slice(0, at).split(':', 1)[0]
  return username === '' ? null : { username, hostPort }
}

/**
 * `[user@]host[:port][/path]`, once the protocol is known.
 *
 * The path is split off at the FIRST `/`, so everything before it is the
 * authority however many slashes follow. `host:` with nothing after it is scp's
 * spelling of "the path starts here" and carries no port.
 */
function readEndpoint(rest: string, protocol: ServerProtocol, secure: boolean | undefined): ParsedAddress {
  const slash = rest.indexOf('/')
  const authority = slash === -1 ? rest : rest.slice(0, slash)
  const rawPath = slash === -1 ? '' : rest.slice(slash)

  const account = readAccount(authority)
  if (!account) return UNPARSED
  const { username, hostPort } = account

  const colon = hostPort.lastIndexOf(':')
  const host = colon === -1 ? hostPort : hostPort.slice(0, colon)
  const rawPort = colon === -1 ? '' : hostPort.slice(colon + 1)
  if (!HOST_RE.test(host)) return UNPARSED

  let port: number | undefined
  if (rawPort !== '') {
    port = readPort(rawPort)
    // Not a port and not scp's empty one: whatever it is, this string doesn't
    // name an endpoint anyone can dial.
    if (port === undefined) return UNPARSED
  }

  const path = normalizePath(rawPath)
  return {
    kind: 'parsed',
    protocol,
    host: host.toLowerCase(),
    port: port ?? defaultPort(protocol, secure),
    ...(username === undefined ? {} : { username }),
    ...(path === undefined ? {} : { path }),
    ...(secure === undefined ? {} : { secure }),
  }
}

/** A port number, or `undefined` when the text isn't one in range. */
function readPort(text: string): number | undefined {
  if (!/^\d{1,5}$/.test(text)) return undefined
  const port = Number(text)
  return port >= 1 && port <= 65535 ? port : undefined
}

/** The protocol's own default, and WebDAV's two. */
function defaultPort(protocol: ServerProtocol, secure: boolean | undefined): number {
  if (protocol === 'sftp') return DEFAULT_PORTS.sftp
  if (protocol === 'smb') return DEFAULT_PORTS.smb
  return secure === false ? DEFAULT_PORTS.webdavPlain : DEFAULT_PORTS.webdavSecure
}

/**
 * The path an address carried, or `undefined` at the root.
 *
 * ❗ For WebDAV this stays whole, path and all. Nobody can tell where a
 * Nextcloud base URL ends and a collection begins, and the backend resolves the
 * remote root relative to the base anyway, so the sheet puts all of it in the
 * address and starts the remote folder at the root.
 */
function normalizePath(rawPath: string): string | undefined {
  const trimmed = rawPath.replace(/\/+$/, '')
  return trimmed === '' ? undefined : trimmed
}
