/**
 * The add form's model, and the two translations around it: a pasted address in,
 * a `ServerTarget` out.
 *
 * Pure, so the sheet stays a renderer and the rules that decide what a typed
 * address MEANS are testable without mounting anything.
 */

import type { ServerProtocol, ServerTarget } from '$lib/ipc/bindings'
import type { SavedSftpServer, SavedWebdavServer } from '$lib/tauri-commands'
import { parseServerAddress, type ParsedAddress } from './address-parser'

/** Every field the add and edit forms hold, across all three protocols. */
export interface ServerForm {
  protocol: ServerProtocol
  /** What the user typed, kept verbatim so their own spelling survives an edit. */
  address: string
  username: string
  /** ❗ Lives here only while the sheet is open. Nothing persists it; the backend's store does. */
  secret: string
  remember: boolean
  displayName: string
  remoteRoot: string
  /** Where the place lands when opened, at or under `remoteRoot`. Empty is the root. */
  startFolder: string
  keyFile: string
  useAgent: boolean
  autoReconnect: boolean
}

/** A blank form, on add mode's defaults. */
export function emptyServerForm(): ServerForm {
  return {
    // Until an address says otherwise. SMB is the one a bare hostname means, and
    // a bare hostname is what someone types first.
    protocol: 'smb',
    address: '',
    username: '',
    secret: '',
    // ❗ Add mode ONLY. A person typing a password into a new server means to
    // come back to it; sign-in mode seeds this from what is already stored, so a
    // user who declined to remember stays declined.
    remember: true,
    displayName: '',
    remoteRoot: '',
    startFolder: '',
    keyFile: '',
    useAgent: true,
    autoReconnect: true,
  }
}

/**
 * Folds what an address turned out to say into the form: the protocol toggle,
 * the account, and the remote folder.
 *
 * ❗ An `unparsed` address changes nothing. The toggle stays where the user left
 * it, and what they typed stays in the field, because a half-typed address is
 * the normal state of a field someone is typing into.
 */
export function applyParsedAddress(form: ServerForm, parsed: ParsedAddress): ServerForm {
  if (parsed.kind === 'unparsed') return form
  return {
    ...form,
    protocol: parsed.protocol,
    // ❗ Only when the address carried one. Typing a host after a username would
    // otherwise wipe the username the same keystroke put there.
    username: parsed.username ?? form.username,
    remoteRoot: parsed.protocol === 'sftp' ? (parsed.path ?? form.remoteRoot) : form.remoteRoot,
  }
}

/**
 * The dial target this form names, or `null` when it names none.
 *
 * `null` covers both an address that doesn't parse and SMB, whose connect is a
 * share mount rather than a session and goes through `connectToServer` instead.
 */
export function serverTargetFrom(form: ServerForm): ServerTarget | null {
  const parsed = parseServerAddress(form.address)
  if (parsed.kind === 'unparsed') return null

  const username = form.username.trim()
  const displayName = form.displayName.trim() || form.address.trim()

  // ❗ The TOGGLE decides which target this is, not the address: it stays
  // editable exactly so someone can type a bare host and say "that one is SFTP".
  // The address only supplies the endpoint, and a port it named for a different
  // protocol is not this protocol's port.
  if (form.protocol === 'sftp') {
    return {
      protocol: 'sftp',
      displayName,
      host: parsed.host,
      port: parsed.protocol === 'sftp' ? parsed.port : 22,
      username,
      remoteRoot: normalizeRoot(form.remoteRoot),
      startFolder: startFolderOf(form),
      keyFile: form.keyFile.trim() === '' ? null : form.keyFile.trim(),
      useAgent: form.useAgent,
      autoReconnect: form.autoReconnect,
    }
  }

  if (form.protocol === 'webdav') {
    return {
      protocol: 'webdav',
      displayName,
      url: webdavBaseUrl(parsed),
      username,
      remoteRoot: normalizeRoot(form.remoteRoot),
      startFolder: startFolderOf(form),
      autoReconnect: form.autoReconnect,
    }
  }

  // SMB: its connect is a share mount rather than a session, so it goes through
  // `connectToServer` and has no target here.
  return null
}

/** The saved SFTP server, as the edit form holds it. */
export function formFromSftpServer(server: SavedSftpServer): ServerForm {
  return {
    ...emptyServerForm(),
    protocol: 'sftp',
    address: `${server.username}@${server.host}:${String(server.port)}`,
    username: server.username,
    displayName: server.displayName,
    remoteRoot: server.remoteRoot,
    startFolder: server.startFolder ?? '',
    keyFile: server.keyFile ?? '',
    useAgent: server.useAgent,
    autoReconnect: server.autoReconnect,
    // Seeded by the sheet from `hasServerSecret`, ❌ never defaulted on here: a
    // default-on box would offer to seed a secret the user already declined.
    remember: false,
  }
}

/** The saved WebDAV server, as the edit form holds it. */
export function formFromWebdavServer(server: SavedWebdavServer): ServerForm {
  return {
    ...emptyServerForm(),
    protocol: 'webdav',
    address: server.url,
    username: server.username,
    displayName: server.displayName,
    remoteRoot: server.remoteRoot,
    startFolder: server.startFolder ?? '',
    autoReconnect: server.autoReconnect,
    remember: false,
  }
}

/**
 * The Nextcloud collection path, appended to a bare origin.
 *
 * ❗ The one remedy worth a button: a person who pastes their Nextcloud's home
 * page gets "nothing here answers WebDAV", and the fix is a path nobody knows.
 * ownCloud shares it.
 */
export function nextcloudAddress(address: string, username: string): string {
  const account = username.trim()
  if (account === '') return address
  return `${address.replace(/\/+$/, '')}/remote.php/dav/files/${encodeURIComponent(account)}/`
}

/**
 * The base URL a WebDAV form dials, port included only when it isn't the
 * scheme's own.
 *
 * ❗ A port and a path the address named for ANOTHER protocol are dropped: `445`
 * off a bare hostname is SMB's default, not a WebDAV port, and carrying it over
 * would dial a port nothing listens for HTTP on.
 */
function webdavBaseUrl(parsed: Extract<ParsedAddress, { kind: 'parsed' }>): string {
  if (parsed.protocol !== 'webdav') return `https://${parsed.host}`
  const scheme = parsed.secure === false ? 'http' : 'https'
  const isDefaultPort = (scheme === 'https' && parsed.port === 443) || (scheme === 'http' && parsed.port === 80)
  const authority = isDefaultPort ? parsed.host : `${parsed.host}:${String(parsed.port)}`
  return `${scheme}://${authority}${parsed.path ?? ''}`
}

/**
 * The start folder a target carries: `null` for the root. The backend refuses one
 * outside the root and stores it normalized, so this only trims.
 */
function startFolderOf(form: ServerForm): string | null {
  const trimmed = form.startFolder.trim()
  return trimmed === '' ? null : trimmed
}

/** The three root spellings (`''`, `'.'`, `'/'`) all mean the volume root. */
function normalizeRoot(root: string): string {
  const trimmed = root.trim()
  return trimmed === '' || trimmed === '.' ? '/' : trimmed
}
