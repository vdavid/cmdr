// S3 accounts: the account's secret, the unattended-reconnect query, and the
// list-price cost estimate a Copy, Move, or Delete dialog shows.
// Connecting, cancelling, disconnecting, pinning, and forgetting go through the
// protocol-agnostic `servers.ts`, which takes S3 places like any other.
//
// The model (an account is an endpoint plus an access key id; its places are
// buckets plus the account root) and every connect outcome:
// `crates/cmdr-s3/DETAILS.md`.

import { commands } from '$lib/ipc/bindings'
import type {
  ClashPlan,
  CostEstimate,
  CostEstimateRequest,
  KnownClash,
  S3ProviderChoice,
  S3UnattendedReconnect,
  SavedS3Place,
} from '$lib/ipc/bindings'
import { throwKeychainError } from '$lib/servers/keychain-failure'

export type {
  ClashPlan,
  CostEstimate,
  CostEstimateRequest,
  KnownClash,
  S3ProviderChoice,
  S3UnattendedReconnect,
  SavedS3Place,
}

/**
 * The saved S3 place a volume id names, or `null` when none does. A saved place
 * carries the provider the listing doesn't: what an edit form and a secret writer
 * need.
 *
 * ❗ By the `volumeId` the backend published, ❌ never a frontend twin of the hash
 * Rust mints from `(host, port, key id, bucket)`: an id spelled twice can drift.
 */
export async function knownS3PlaceOf(volumeId: string): Promise<SavedS3Place | null> {
  return (await commands.getKnownS3Places()).find((place) => place.volumeId === volumeId) ?? null
}

/**
 * Saves the secret access key for one ACCOUNT (a provider's endpoint plus an
 * access key id), so every bucket under that key connects silently.
 *
 * This call is the "remember the secret" switch: `hasServerSecret` reads it
 * back, `forgetServerSecret` turns it off (both in `servers.ts`), and there's no second flag that could
 * disagree with the store.
 *
 * Throws a `KeychainFailure` if the store refused, or if the provider's fields
 * make no endpoint.
 */
export async function saveS3Credentials(
  provider: S3ProviderChoice,
  accessKeyId: string,
  secret: string,
): Promise<void> {
  const res = await commands.saveS3Credentials(provider, accessKeyId, secret)
  if (res.status === 'error') throwKeychainError(res.error)
}

/**
 * Whether a registered S3 place can come back on its own as it stands, the
 * backend's own answer. `no_stored_secret` is the one to warn about: the switch
 * is on and nothing is stored. `null` when nothing S3 is registered under that id.
 * Ask when a banner renders rather than polling; it can reach the Keychain.
 */
export async function getS3UnattendedReconnect(volumeId: string): Promise<S3UnattendedReconnect | null> {
  return await commands.getS3UnattendedReconnect(volumeId)
}

/**
 * What a planned copy, move, or delete costs at each involved provider's list
 * prices, one entry per priced provider. Reads the dialog's settled scan preview
 * by `previewId`, never S3, so it's cheap to ask unconditionally once the scan
 * settles: it answers `[]` when neither end is a priced S3 place or the preview
 * isn't settled. Pricing model: `apps/desktop/src-tauri/src/s3_costs/DETAILS.md`.
 */
export async function estimateOperationCost(request: CostEstimateRequest): Promise<CostEstimate[]> {
  return await commands.estimateOperationCost(request)
}
