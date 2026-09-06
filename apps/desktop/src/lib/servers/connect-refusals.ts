/**
 * The words for every reason a connect can stop.
 *
 * ❗ A `Record` over `ConnectRefusalKind`, so a new reason can't reach a person
 * without someone writing its sentence, and every key appears here as a literal
 * (which is what keeps `desktop-message-keys-unused` honest without a dynamic
 * prefix allowlist).
 *
 * Writing rules: `docs/guides/error-handling.md` § "Writing rules". Never the
 * words "error" or "failed", always what the person can do about it, and ❌ never
 * a backend diagnostic ("PROPFIND", "Digest", "rung", "transport").
 */

import { tString } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import type { ConnectRefusalKind } from './connect-flow'

const REFUSAL_KEYS: Record<ConnectRefusalKind, MessageKey> = {
  authentication_rejected: 'servers.refusal.authenticationRejected',
  needs_credentials: 'servers.refusal.needsCredentials',
  auth_method_unsupported: 'servers.refusal.authMethodUnsupported',
  certificate_untrusted: 'servers.refusal.certificateUntrusted',
  not_a_webdav_server: 'servers.refusal.notAWebdavServer',
  invalid_url: 'servers.refusal.invalidUrl',
  timed_out: 'servers.refusal.timedOut',
  unreachable: 'servers.refusal.unreachable',
  host_key_untrusted: 'servers.refusal.hostKeyUntrusted',
  host_key_revoked: 'servers.refusal.hostKeyRevoked',
}

/** What the place is called in a refusal: its host where there is one, else its name. */
export interface RefusalSubject {
  /** The server's own name, from the place's path. */
  host: string
  /** The account, for the sentence about a password that didn't work. */
  username: string
}

/** The one sentence a refusal says. */
export function wordConnectRefusal(kind: ConnectRefusalKind, subject: RefusalSubject): string {
  return tString(REFUSAL_KEYS[kind], { host: subject.host, username: subject.username })
}
