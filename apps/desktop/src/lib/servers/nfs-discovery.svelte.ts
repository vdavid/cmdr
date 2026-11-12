/**
 * The two questions the NFS add form asks a server before any place exists: which
 * exports it lists, and who owns the files in the one picked (the uid and gid to
 * act as). `NfsPlaceFields.svelte` drives it as the person types.
 *
 * ❗ **Both race the typing**, so each answer is checked against the latest ask
 * and dropped when someone typed past it: a list for `nas` landing after the
 * person finished typing `testnas.local` would offer another server's exports.
 *
 * ❗ **Neither is a gate.** A refused or unreachable listing leaves the export
 * typeable (a typed export may still mount), and the connect reports its own
 * refusal. So a broken bridge reads as `unreachable` and a lost guess as none.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import type { GuessedNfsIdentity, NfsExportList } from '#lib/ipc/bindings.js'

const log = getAppLogger('servers')

/** What the export list says right now. `timed_out` and `unreachable` read the same: nothing answered. */
export type NfsExportListing =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'listed'; exports: string[] }
  | { kind: 'refused' }
  | { kind: 'no_nfs_v3' }
  | { kind: 'unreachable' }

/** The server an ask goes to: the address's host, and the NFS port it named (`null`: ask portmap). */
export interface NfsEndpoint {
  host: string
  port: number | null
}

/** The two backend asks, injected so tests can answer them out of order. */
export interface NfsDiscoveryDeps {
  listNfsExports: (endpoint: NfsEndpoint) => Promise<NfsExportList>
  guessNfsIdentity: (ask: NfsEndpoint & { exportPath: string }) => Promise<GuessedNfsIdentity>
}

/** One form's discovery state. */
export interface NfsDiscovery {
  readonly listing: NfsExportListing
  /** Asks the server for its exports; an answer for an older ask is dropped. */
  listFor: (endpoint: NfsEndpoint) => Promise<void>
  /** The guess for `exportPath`, or `null` when a newer ask superseded it or the bridge broke. */
  guessFor: (ask: NfsEndpoint & { exportPath: string }) => Promise<GuessedNfsIdentity | null>
  /** Forgets the list: the address stopped naming a server. */
  clear: () => void
}

export function createNfsDiscovery(deps: NfsDiscoveryDeps): NfsDiscovery {
  let listing = $state<NfsExportListing>({ kind: 'idle' })
  let listAsk = 0
  let guessAsk = 0

  async function listFor(endpoint: NfsEndpoint): Promise<void> {
    const ask = ++listAsk
    listing = { kind: 'loading' }
    let answer: NfsExportListing = { kind: 'unreachable' }
    try {
      answer = listingOf(await deps.listNfsExports(endpoint))
    } catch (e) {
      // The log names the host, ❌ never more: it's what the person typed.
      log.warn('Asking {host} for its NFS exports broke down: {error}', { host: endpoint.host, error: e })
    }
    if (ask === listAsk) listing = answer
  }

  async function guessFor(request: NfsEndpoint & { exportPath: string }): Promise<GuessedNfsIdentity | null> {
    const ask = ++guessAsk
    try {
      const guess = await deps.guessNfsIdentity(request)
      return ask === guessAsk ? guess : null
    } catch (e) {
      log.warn('Guessing who owns an NFS export on {host} broke down: {error}', { host: request.host, error: e })
      return null
    }
  }

  return {
    get listing() {
      return listing
    },
    listFor,
    guessFor,
    clear: () => {
      listAsk++
      listing = { kind: 'idle' }
    },
  }
}

function listingOf(answer: NfsExportList): NfsExportListing {
  switch (answer.outcome) {
    case 'listed':
      return { kind: 'listed', exports: answer.exports }
    case 'listing_refused':
      return { kind: 'refused' }
    case 'no_nfs_v3':
      return { kind: 'no_nfs_v3' }
    case 'timed_out':
    case 'unreachable':
      return { kind: 'unreachable' }
  }
}
