/**
 * The words for a typed `MountError`: what the network pane says when a share
 * someone opened didn't mount.
 *
 * Classification is the backend's (`src-tauri/src/network/mount.rs`, its Linux
 * twin, and `share_access.rs`'s second opinion on a "not found"); the words are
 * all here, pulled from the `errors.mount.*` catalog so every locale gets its
 * own. Same split as the eject path (`../navigation/eject-error-messages.ts`);
 * `docs/guides/error-handling.md` is the map.
 *
 * Each message is ONE plain-text sentence or two, rendered under the pane's
 * "Couldn't mount share" title and mirrored into `cmdr://state`. Copy is pulled
 * via `getMessage()` (a RAW catalog lookup, never ICU `t()`), like every
 * `errors.*` family, so its `{token}`s are filled here.
 *
 * ❌ `Unexpected.detail` is never the message: it's diagnostic text for the log.
 */
import type { MountError } from '$lib/ipc/bindings'
import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * Fills `{token}` placeholders in ONE pass, so a share literally named
 * `{server}` stays a name instead of becoming a second substitution target.
 */
function raw(key: MessageKey, params: Record<string, string> = {}): string {
  return getMessage(key).replace(/\{([a-zA-Z]+)\}/g, (token, name: string) => params[name] ?? token)
}

/**
 * One renderer per `MountError` variant. `server` is already resolved: the
 * name the pane shows, else the address the mount used.
 *
 * A record rather than a `switch`: the mapped type demands every variant, so a
 * backend that grows one stops the frontend compiling until it has words.
 */
const MOUNT_MESSAGE: {
  [K in MountError['type']]: (error: Extract<MountError, { type: K }>, server: string) => string
} = {
  host_unreachable: (_e, server) => raw('errors.mount.hostUnreachable', { server }),
  timeout: (_e, server) => raw('errors.mount.timeout', { server }),
  share_not_found: (e, server) => raw('errors.mount.shareNotFound', { server, share: e.share }),
  auth_required: (e, server) => raw('errors.mount.authRequired', { server, share: e.share }),
  auth_failed: (_e, server) => raw('errors.mount.authFailed', { server }),
  permission_denied: (e, server) =>
    raw('errors.mount.permissionDenied', { server, share: e.share, username: e.username }),
  cancelled: (e) => raw('errors.mount.cancelled', { share: e.share }),
  unsupported_protocol: (_e, server) => raw('errors.mount.unsupportedProtocol', { server }),
  mount_refused: (e, server) => raw('errors.mount.mountRefused', { server, share: e.share }),
  mount_missing: (e, server) => raw('errors.mount.mountMissing', { server, share: e.share }),
  gvfs_missing: () => raw('errors.mount.gvfsMissing'),
  unexpected: (e, server) => raw('errors.mount.unexpected', { server, share: e.share }),
}

/**
 * The sentence a mount refusal says.
 *
 * `serverName` is what the pane calls the server (the host's own name, like
 * `Naspolya`). The backend only knows the address it mounted by, often an IP,
 * so that's the fallback.
 */
export function renderMountError(error: MountError, serverName?: string): string {
  const server = serverName ?? ('server' in error ? error.server : '')
  const render = MOUNT_MESSAGE[error.type] as (error: MountError, server: string) => string
  return render(error, server)
}
