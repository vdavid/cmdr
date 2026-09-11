/**
 * The words for a typed `ShareListError`: what the network pane says when a
 * server's share list didn't load, and the tooltip on that server's row.
 *
 * Classification is the backend's (`crates/cmdr-smb/src/types.rs`, filled by the
 * smb2 client and its `smbutil` / `smbclient` fallbacks); the words are all here,
 * pulled from the `errors.shareList.*` catalog so every locale gets its own. Same
 * split as `mount-error-messages.ts`; `docs/guides/error-handling.md` is the map.
 *
 * ❌ The variant's `message` is never the message: the backend documents it as
 * diagnostic detail for the log, and it's English (often a tool's own stderr).
 * A `missing_dependency`'s install command renders separately, in the pane's copy
 * box.
 */
import type { ShareListError } from '$lib/ipc/bindings'
import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Fills `{token}` placeholders in ONE pass, so a server literally named `{server}` stays a name. */
function raw(key: MessageKey, params: Record<string, string> = {}): string {
  return getMessage(key).replace(/\{([a-zA-Z]+)\}/g, (token, name: string) => params[name] ?? token)
}

/**
 * One renderer per `ShareListError` variant. A record rather than a `switch`:
 * the mapped type demands every variant, so a backend that grows one stops the
 * frontend compiling until it has words.
 */
const SHARE_LIST_MESSAGE: { [K in ShareListError['type']]: (server: string) => string } = {
  host_unreachable: (server) => raw('errors.shareList.hostUnreachable', { server }),
  timeout: (server) => raw('errors.shareList.timeout', { server }),
  auth_required: (server) => raw('errors.shareList.authRequired', { server }),
  signing_required: (server) => raw('errors.shareList.signingRequired', { server }),
  auth_failed: (server) => raw('errors.shareList.authFailed', { server }),
  protocol_error: (server) => raw('errors.shareList.protocolError', { server }),
  resolution_failed: (server) => raw('errors.shareList.resolutionFailed', { server }),
  missing_dependency: () => raw('errors.shareList.missingDependency'),
}

/**
 * The sentence a share listing that didn't load says. `serverName` is what the
 * pane and the servers list call the host (like `Naspolya`).
 */
export function renderShareListError(error: ShareListError, serverName: string): string {
  return SHARE_LIST_MESSAGE[error.type](serverName)
}
