/**
 * The hub's list: every server the user saved, plus every host mDNS found, as
 * one row each.
 *
 * Pure, so the merge and the ordering are testable without a component. The hub
 * component reads the three inputs (the saved list, the discovery store, the
 * volume list) and renders what comes back.
 *
 * ❗ **The merge is the part that goes quietly wrong.** A manually-typed SMB host
 * is BOTH a saved server and a discovered host — adding one injects it into the
 * discovery state — so concatenating the two sources shows a person's NAS twice.
 * The dedup matches on the id first (a manual host keeps its store id in the
 * discovery list) and then on the name or resolved hostname, because
 * `known_shares` files a host under the server name statfs reported while mDNS
 * files the same machine under its Bonjour name.
 */

import type { SavedPlace, SavedServer } from '$lib/tauri-commands'
import type { ConnectionState, NetworkHost, VolumeInfo } from '../types'

/** What the Status column says about a row. */
export type HubRowStatus =
  /** A live session Cmdr owns, or an SMB share mounted through the OS. */
  | 'connected'
  /** Saved and idle: nothing in flight, and nothing is wrong. */
  | 'saved'
  /** mDNS is seeing it right now. */
  | 'found_nearby'
  /** The session dropped because a credential is what's missing. */
  | 'signed_out'
  /** An SFTP host key is waiting for the user to look at it. */
  | 'waiting_for_key'

/** One line in the hub's table. */
export interface HubRow {
  /** Stable across rebuilds: the saved server's id, else the host's. */
  id: string
  /** What the Name column shows. */
  name: string
  /** Which protocol the row speaks, for the Type column. */
  protocol: 'smb' | 'sftp' | 'webdav'
  /** What the Address column shows: resolved where mDNS resolved it. */
  address: string
  status: HubRowStatus
  /** ISO 8601, or `null` when nothing ever recorded one. */
  lastConnectedAt: string | null
  /**
   * The place's volume id, for the protocols that have exactly one place.
   *
   * ❗ `null` for an SMB host: its places are mounted shares with ids `statfs`
   * mints, so a host row has nothing a place command could act on. Enter on one
   * opens its places list instead.
   */
  volumeId: string | null
  /** Whether the place is pinned to the switcher. Always `false` for SMB. */
  pinned: boolean
  /** The saved entry behind the row, when the user saved one. */
  saved: SavedServer | null
  /** The discovered host behind the row, when mDNS is seeing one. */
  host: NetworkHost | null
}

/** What the hub reads to build its list. */
export interface HubRowSources {
  /** `listSavedServers()`, the union of the three stores. */
  saved: SavedServer[]
  /** The discovery store's hosts, manual entries included. */
  hosts: NetworkHost[]
  /** The current volume list, which is where a place's standing lives. */
  volumes: VolumeInfo[]
}

/**
 * Rank groups, most urgent first: a live session, then one asking something of
 * the user, then the rest of what they saved, then what is merely nearby.
 */
const STATUS_RANK: Record<HubRowStatus, number> = {
  connected: 0,
  signed_out: 1,
  waiting_for_key: 1,
  saved: 2,
  found_nearby: 3,
}

/** The hub's rows, merged and ordered. */
export function buildHubRows(sources: HubRowSources): HubRow[] {
  const states = new Map(sources.volumes.map((volume) => [volume.id, volume.connectionState ?? null]))
  const claimed = new Set<string>()
  const rows: HubRow[] = []

  for (const server of sources.saved) {
    const host = server.protocol === 'smb' ? matchHost(server, sources.hosts) : null
    if (host) claimed.add(host.id)
    rows.push(savedRow(server, host, states))
  }

  for (const host of sources.hosts) {
    if (claimed.has(host.id)) continue
    rows.push(nearbyRow(host))
  }

  return rows.sort(compareRows)
}

/** The discovered host that IS this saved SMB server, if mDNS is seeing it. */
function matchHost(server: SavedServer, hosts: NetworkHost[]): NetworkHost | null {
  const address = server.address.toLowerCase()
  const name = server.displayName.toLowerCase()
  return (
    hosts.find(
      (host) =>
        host.id === server.id ||
        host.name.toLowerCase() === address ||
        host.name.toLowerCase() === name ||
        host.hostname?.toLowerCase() === address,
    ) ?? null
  )
}

function savedRow(server: SavedServer, host: NetworkHost | null, states: Map<string, ConnectionState | null>): HubRow {
  // ❗ Length-checked, not `[0] ?? null`: an SMB server carries no places at
  // all, and the index signature would otherwise type the gap away.
  const place: SavedPlace | null = server.places.length > 0 ? server.places[0] : null
  return {
    id: server.id,
    name: server.displayName,
    protocol: server.protocol,
    address: hostAddress(host) ?? server.address,
    status: savedStatus(server, host, place ? (states.get(place.volumeId) ?? null) : null),
    lastConnectedAt: server.lastConnectedAt,
    volumeId: place?.volumeId ?? null,
    pinned: place?.pinned ?? false,
    saved: server,
    host,
  }
}

function nearbyRow(host: NetworkHost): HubRow {
  return {
    id: host.id,
    name: host.name,
    protocol: 'smb',
    address: hostAddress(host) ?? host.name,
    status: 'found_nearby',
    lastConnectedAt: null,
    volumeId: null,
    pinned: false,
    saved: null,
    host,
  }
}

/**
 * How live a saved row is.
 *
 * ❗ Off the VOLUME LIST, ❌ never off `SavedPlace.connected`: that flag is a
 * snapshot from the moment the listing was built, while the volume list is what
 * the switcher's dot and the pane both read. Two surfaces disagreeing about
 * whether a server is up is worse than either being briefly stale.
 *
 * An SMB host has no place to ask about, so mDNS seeing it is the whole answer:
 * a mounted share is its own volume row, not this host.
 */
function savedStatus(server: SavedServer, host: NetworkHost | null, state: ConnectionState | null): HubRowStatus {
  if (server.protocol === 'smb') return host ? 'found_nearby' : 'saved'
  switch (state) {
    case 'direct':
    case 'os_mount':
      return 'connected'
    case 'needs_sign_in':
      return 'signed_out'
    case 'needs_host_key_approval':
      return 'waiting_for_key'
    // `disconnected` says a backoff loop is running, which is a detail of HOW the
    // row gets back: to a person it is still one of their saved servers, and the
    // switcher's dot is where liveness is spelled out.
    case 'disconnected':
    case 'saved':
    case null:
      return 'saved'
  }
}

/** The most useful spelling of where a discovered host lives. */
function hostAddress(host: NetworkHost | null): string | null {
  return host?.ipAddress ?? host?.hostname ?? null
}

/** Live first, then what's asking for you, then saved, then nearby. */
function compareRows(a: HubRow, b: HubRow): number {
  const byStatus = STATUS_RANK[a.status] - STATUS_RANK[b.status]
  if (byStatus !== 0) return byStatus
  const byRecency = recency(b) - recency(a)
  if (byRecency !== 0) return byRecency
  return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
}

/** When the row was last used, as a sortable number. Never used sorts last. */
function recency(row: HubRow): number {
  if (!row.lastConnectedAt) return 0
  const parsed = Date.parse(row.lastConnectedAt)
  return Number.isNaN(parsed) ? 0 : parsed
}
