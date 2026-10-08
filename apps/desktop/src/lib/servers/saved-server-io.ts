/**
 * The per-protocol reads and writes the add and edit sheets make against the saved
 * stores: which store row an edit opens on, where a typed secret is filed, and the
 * backend's "this can't reconnect on its own" answer.
 *
 * ❗ One switch per question, so a new protocol is one arm here rather than an arm
 * in the sheet and another in `open-sign-in.ts`'s "Add anyway" (the secret writer
 * used to live in both).
 */

import type { SavedServer, ServerTarget } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'
import {
  getKnownSftpServers,
  getKnownWebdavServers,
  getS3UnattendedReconnect,
  getSftpUnattendedReconnect,
  getWebdavUnattendedReconnect,
  knownS3PlaceOf,
  saveS3Credentials,
  saveSftpCredentials,
  saveWebdavCredentials,
} from '$lib/tauri-commands'
import { formFromS3Place, formFromSftpServer, formFromWebdavServer, type ServerForm } from './server-form'

/**
 * The edit form for a saved account's store row, or `null` when its store has none.
 *
 * ❗ SFTP and WebDAV are matched on the ADDRESS the listing published: the volume id
 * is minted in Rust and there is no frontend twin of that hash. An S3 place is looked
 * up by `placeId`, the volume id the backend publishes for each saved place, because
 * an account's buckets are each saved on their own. An SMB host has no store row here.
 */
export async function savedEditForm(server: SavedServer, placeId: string): Promise<ServerForm | null> {
  switch (server.protocol) {
    case 'sftp': {
      const saved = (await getKnownSftpServers()).find(
        (s) => `${s.host}:${String(s.port)}` === server.address && s.username === server.username,
      )
      return saved ? formFromSftpServer(saved) : null
    }
    case 'webdav': {
      const saved = (await getKnownWebdavServers()).find(
        (s) => s.url === server.address && s.username === server.username,
      )
      return saved ? formFromWebdavServer(saved) : null
    }
    case 's3': {
      const saved = await knownS3PlaceOf(placeId)
      return saved ? formFromS3Place(saved) : null
    }
    case 'smb':
      return null
  }
}

/**
 * Files `secret` under the key the target's volume id is minted from, so the entry
 * this writes is the one the next dial reads: SFTP's `(host, port, username)`,
 * WebDAV's base URL and account, and for S3 the ACCOUNT (provider plus key id),
 * which every bucket under the key shares. Throws a `KeychainFailure` on refusal.
 */
export async function saveTargetSecret(target: ServerTarget, secret: string): Promise<void> {
  switch (target.protocol) {
    case 'sftp':
      await saveSftpCredentials(target.host, target.port, target.username, secret)
      return
    case 'webdav':
      await saveWebdavCredentials(target.url, target.username, secret)
      return
    case 's3':
      await saveS3Credentials(target.provider, target.accessKeyId, secret)
      return
  }
}

/**
 * The backend's own answer to "auto-reconnect is on and nothing happens", worded,
 * or `null` when there is nothing to warn about.
 *
 * ❗ Asked when the sheet RENDERS, ❌ never derived from a rung plus a credential
 * check: the rung is decided per dial, and a derivation goes stale the moment one
 * lands elsewhere. The backends spell the same state differently.
 */
export async function unattendedReconnectWarning(
  id: string,
  protocol: SavedServer['protocol'],
): Promise<string | null> {
  const warn = tString('servers.sheet.needsStoredSecret')
  switch (protocol) {
    case 'sftp':
      return (await getSftpUnattendedReconnect(id)) === 'needs_stored_secret' ? warn : null
    case 'webdav':
      return (await getWebdavUnattendedReconnect(id)) === 'no_stored_secret' ? warn : null
    case 's3':
      return (await getS3UnattendedReconnect(id)) === 'no_stored_secret' ? warn : null
    case 'smb':
      return null
  }
}
