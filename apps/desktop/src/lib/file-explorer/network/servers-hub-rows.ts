/**
 * The hub's list: every server the user saved, plus every host mDNS found, as
 * one row each.
 *
 * Pure, so the merge and the ordering are testable without a component. The hub
 * component reads the three inputs (the saved list, the discovery store, the
 * volume list) and renders what comes back.
 *
 * ❗ **The merge is the part that goes quietly wrong.** A manually-typed SMB host
 * is BOTH a saved server and a discovered host (adding one injects it into the
 * discovery state), so concatenating the two sources shows a person's NAS twice.
 * The dedup matches on the id first (a manual host keeps its store id in the
 * discovery list) and then on the name or resolved hostname, because
 * `known_shares` files a host under the server name statfs reported while mDNS
 * files the same machine under its Bonjour name. It claims EVERY host that
 * matches, since one machine can be in the discovery list under both spellings.
 *
 * A server with MANY places (an SMB host's saved shares, `docs/specs/saved-smb-shares.md`,
 * and an S3 account's saved buckets) lists each place as a row of its own, right under
 * its server: the list says "user + server + share", which is what a person means to
 * save (cmdr-reports#7). The server row itself has no place to act on.
 */

import type { ServerProtocol } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'
import { parseServerPath } from '$lib/servers/server-path-utils'
import type { SavedPlace, SavedServer } from '$lib/tauri-commands'
import type { ConnectionState, NetworkHost, VolumeInfo } from '../types'
import { signedInAsOfMount, signedInAsUser, type SignedInAs } from './signed-in-as'

/** SMB's own port, which an address leaves unsaid. */
const SMB_PORT = 445

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
  /** Stable across rebuilds: the saved server's id, else the host's; `share:<volume id>` for a place row. */
  id: string
  /**
   * A SERVER (an account, or a host mDNS sees) or one of a many-place server's
   * saved PLACES (an SMB share, an S3 bucket or account root), right under it.
   */
  kind: 'server' | 'place'
  /** A place row's server row, `null` for a server. */
  parentId: string | null
  /**
   * The account the row is signed in as, `null` when nothing known says. A share: the live mount's while it's
   * connected, else the saved one the next connect uses. An SMB server: the SERVER-level account, the same one its
   * share list's header names (the account its listing signed in as, else the one it's set to be used with), ❌ never
   * a share's mount, which disagreed with the header. A one-place server: `null` (its name is `user@host` already).
   */
  account: SignedInAs | null
  /** A place row's place, `null` for a server row (a one-place server's is `saved.places[0]`). */
  place: SavedPlace | null
  /** What the Name column shows. */
  name: string
  /** Which protocol the row speaks, for the Type column. */
  protocol: ServerProtocol
  /** What the Address column shows: resolved where mDNS resolved it. */
  address: string
  status: HubRowStatus
  /** ISO 8601, or `null` when nothing ever recorded one. */
  lastConnectedAt: string | null
  /**
   * The place's volume id: a one-place server's, or a place row's.
   *
   * ❗ `null` for a many-place server row (an SMB host, an S3 account): its places
   * are rows of their own. Enter on an SMB host opens its places list instead.
   */
  volumeId: string | null
  /** Whether the place is pinned to the switcher. Always `false` for a many-place server. */
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
  /**
   * The account a host's share list last signed in as, by the host's id (`network-store.svelte.ts`'s
   * `getListedAccount`), or `undefined` when no listing said.
   */
  listedAs?: (hostId: string) => SignedInAs | undefined
}

/**
 * Rank groups, most urgent first: a live session, then one asking something of
 * the user, then the rest of what they saved, then what mDNS is seeing.
 */
const STATUS_RANK: Record<HubRowStatus, number> = {
  connected: 0,
  signed_out: 1,
  waiting_for_key: 1,
  saved: 2,
  found_nearby: 3,
}

/**
 * The hub's rows, merged and ordered.
 *
 * ❗ **Every row's `id` is unique, and that is this function's job, ❌ not its
 * caller's.** The hub keys its `{#each}` on it, and Svelte THROWS
 * (`each_key_duplicate`) on a repeat, so a duplicate is a CRASHED pane rather
 * than a row shown twice — and it takes the whole servers hub down with it. The
 * sources can genuinely repeat one: `listSavedServers()` unions three stores, and
 * a server recorded in two of them arrives twice. First writer wins, so the
 * order below still decides which row a person sees.
 */
export function buildHubRows(sources: HubRowSources): HubRow[] {
  const states = new Map(sources.volumes.map((volume) => [volume.id, volume.connectionState ?? null]))
  const mountAccounts = new Map(sources.volumes.map((volume) => [volume.id, volume.mountAccount ?? null]))
  const listed = (ids: (string | undefined)[]): SignedInAs | null => {
    for (const id of ids) {
      const answer = id === undefined ? undefined : sources.listedAs?.(id)
      if (answer) return answer
    }
    return null
  }
  const claimed = new Set<string>()
  const taken = new Set<string>()
  const rows: HubRow[] = []

  const add = (row: HubRow): void => {
    if (taken.has(row.id)) return
    taken.add(row.id)
    rows.push(row)
  }

  for (const server of sources.saved) {
    const hosts = matchingHosts(server, sources.hosts)
    for (const host of hosts) claimed.add(host.id)
    add(savedRow(server, primaryHost(hosts), states))
  }

  for (const host of sources.hosts) {
    if (claimed.has(host.id)) continue
    add({ ...nearbyRow(host), account: listed([host.id]) })
  }

  // Servers in rank order, each followed by its places in name order. ❗ Places
  // are placed AFTER the sort, so a place never drifts away from its server.
  const ordered: HubRow[] = []
  for (const row of rows.sort(compareRows)) {
    ordered.push(row)
    if (!row.saved || !hasManyPlaces(row.protocol)) continue
    // An S3 account's root ("All buckets") leads, then the places by name.
    const places = [...row.saved.places].sort(
      (a, b) =>
        Number(isS3AccountRoot(b, row.protocol)) - Number(isS3AccountRoot(a, row.protocol)) ||
        a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
    )
    const placeRows = places.map((place) => placeRow(row, place, states, mountAccounts.get(place.volumeId) ?? null))
    // An SMB server: the account its listing signed in as, else its saved one. An S3 account: its key.
    row.account =
      row.protocol === 'smb'
        ? (listed([row.host?.id, row.id]) ?? signedInAsUser(row.saved.username))
        : signedInAsUser(row.saved.username)
    for (const placeRowEntry of placeRows) {
      if (taken.has(placeRowEntry.id)) continue
      taken.add(placeRowEntry.id)
      ordered.push(placeRowEntry)
    }
  }
  return ordered
}

/**
 * Whether a protocol's server holds many places, each a row of its own under it:
 * an SMB host's shares, an S3 account's buckets. SFTP and WebDAV have exactly one.
 */
export function hasManyPlaces(protocol: ServerProtocol): boolean {
  return protocol === 'smb' || protocol === 's3'
}

/**
 * Whether `place` is an S3 account's ROOT (the place that lists every bucket), read
 * off its app root through the path grammar: the root's server path is empty.
 */
function isS3AccountRoot(place: SavedPlace, protocol: ServerProtocol): boolean {
  return protocol === 's3' && parseServerPath(place.appRoot)?.path === ''
}

/**
 * A saved place of a many-place server, as the row under its server.
 *
 * Its status is off the VOLUME LIST like every other place's. A share: connected
 * while its volume is mounted (through the kernel or directly), saved otherwise. An
 * S3 place reads like a one-place server (`placeStatus`), and names no account: the
 * account row right above it already says the key.
 *
 * ❗ While connected a share names the account the LIVE mount signed in as (`mountAccount`,
 * off the mount table; `GUEST` is nobody). The saved account is for the next connect:
 * an Add as otheruser over a mount signed in as testuser read "Connected … as otheruser".
 */
function placeRow(
  server: HubRow,
  place: SavedPlace,
  states: Map<string, ConnectionState | null>,
  mountAccount: string | null,
): HubRow {
  const state = states.get(place.volumeId) ?? null
  const isSmb = server.protocol === 'smb'
  const live = state === 'direct' || state === 'os_mount'
  const liveAccount = live && isSmb ? signedInAsOfMount(mountAccount) : null
  return {
    id: `share:${place.volumeId}`,
    kind: 'place',
    parentId: server.id,
    account: isSmb ? (liveAccount ?? signedInAsUser(place.username)) : null,
    place,
    // ❗ The backend labels an S3 root as its account (what the switcher and a pane
    // show), which right under the account's own row would read as one name twice.
    name: isS3AccountRoot(place, server.protocol) ? tString('servers.hub.s3AllBuckets') : place.name,
    protocol: server.protocol,
    address: server.address,
    status: isSmb ? (live ? 'connected' : 'saved') : placeStatus(state),
    lastConnectedAt: null,
    volumeId: place.volumeId,
    pinned: place.pinned,
    saved: server.saved,
    host: server.host,
  }
}

/**
 * Every host in the discovery list that IS this saved SMB server.
 *
 * ❗ Plural on purpose: ONE machine can be in that list twice, because adding a
 * host by hand injects a `manual` host beside the `discovered` one mDNS already
 * found. Claiming only the first would leave the other as a second row for the
 * same NAS.
 */
function matchingHosts(server: SavedServer, hosts: NetworkHost[]): NetworkHost[] {
  if (server.protocol !== 'smb') return []
  const address = server.address.toLowerCase()
  const name = server.displayName.toLowerCase()
  return hosts.filter(
    (host) =>
      host.id === server.id ||
      host.name.toLowerCase() === address ||
      host.name.toLowerCase() === name ||
      host.hostname?.toLowerCase() === address,
  )
}

/**
 * The ids of the discovery list's hosts that ARE one of the saved servers, by the
 * same match the hub's merge makes. Every other host is one Cmdr merely found.
 */
export function savedSmbHostIds(saved: SavedServer[], hosts: NetworkHost[]): Set<string> {
  return new Set(saved.flatMap((server) => matchingHosts(server, hosts)).map((host) => host.id))
}

/**
 * Whether the row is a host Cmdr only FOUND: no saved server, no saved share,
 * nothing the person added. The hub folds these into one group under the saved
 * servers (`servers-hub-items.ts`), and nothing lists their shares until the
 * person opens one (`network-store.svelte.ts`).
 */
export function isNearbyOnly(row: HubRow): boolean {
  return row.saved === null
}

/**
 * Which of them the row speaks for.
 *
 * A DISCOVERED host wins: its name is the Bonjour name a person recognizes,
 * where a manual host is named after the address they typed.
 */
function primaryHost(hosts: NetworkHost[]): NetworkHost | null {
  const discovered = hosts.find((host) => host.source === 'discovered')
  if (discovered) return discovered
  // ❗ Length-checked, ❌ not `[0] ?? null`: the index signature types the gap
  // away, so an empty list would hand back `undefined` wearing `NetworkHost`.
  return hosts.length > 0 ? hosts[0] : null
}

function savedRow(server: SavedServer, host: NetworkHost | null, states: Map<string, ConnectionState | null>): HubRow {
  // ❗ Length-checked, not `[0] ?? null`: an SMB server may carry no places,
  // and the index signature would otherwise type the gap away.
  const place: SavedPlace | null = server.places.length > 0 ? server.places[0] : null
  const many = hasManyPlaces(server.protocol)
  return {
    id: server.id,
    kind: 'server',
    parentId: null,
    account: null,
    place: null,
    name: displayName(server, host),
    protocol: server.protocol,
    address: hostAddress(host) ?? server.address,
    status: savedStatus(server, host, states),
    lastConnectedAt: server.lastConnectedAt,
    // A many-place server's places are rows of their own; the server row has none to act on.
    volumeId: many ? null : (place?.volumeId ?? null),
    pinned: many ? false : (place?.pinned ?? false),
    saved: server,
    host,
  }
}

/**
 * Which of the three names the Name column shows.
 *
 * ❗ A name a PERSON chose wins, then the Bonjour name mDNS found, then the
 * stand-in nobody chose. The top rank is a FACT the backend publishes
 * (`SavedServer.nameSource`), ❌ never a guess at the string's shape: an SMB
 * host's label is either the way `statfs` spells the server
 * (`smb-consumer-guest`, written to `known_shares` the first time the host is
 * opened) or the address typed into "Add server". Without the rank, the friendly
 * name a person recognizes (`SMB Test (Guest)`, `Naspolya`) would vanish from
 * the column the moment they used the host.
 */
function displayName(server: SavedServer, host: NetworkHost | null): string {
  if (server.nameSource === 'user') return server.displayName
  return host?.name ?? server.displayName
}

function nearbyRow(host: NetworkHost): HubRow {
  return {
    id: host.id,
    kind: 'server',
    parentId: null,
    account: null,
    place: null,
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
 * a mounted share is its own volume row, not this host. ❗ A DISCOVERED host,
 * ❌ never the host's own manual entry: Cmdr injects every typed-in host into the
 * discovery list at startup, reachable or not, so that entry says nothing about
 * the network. `primaryHost` puts a discovered one first when there is one.
 */
function savedStatus(
  server: SavedServer,
  host: NetworkHost | null,
  states: Map<string, ConnectionState | null>,
): HubRowStatus {
  if (server.protocol === 'smb') return host?.source === 'discovered' ? 'found_nearby' : 'saved'
  const placeStatuses = server.places.map((place) => placeStatus(states.get(place.volumeId) ?? null))
  // An S3 account reads as its most urgent place: live if one is, else asking if one asks.
  // A one-place server has exactly one.
  return placeStatuses.reduce<HubRowStatus>(
    (best, status) => (STATUS_RANK[status] < STATUS_RANK[best] ? status : best),
    'saved',
  )
}

/** How live one place is, off its volume's connection state. */
function placeStatus(state: ConnectionState | null): HubRowStatus {
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

/** Where Enter on a row leads. */
export type HubOpenMove =
  /** A host's share list. `label` is what the row calls it, for the list's words. */
  | { kind: 'host'; host: NetworkHost; label: string }
  /** The pane lands on the row's place: a one-place server's, or a place row's. */
  | { kind: 'place'; row: HubRow }
  /** An S3 account row, which is no place itself: the hub says to open one of the places under it. */
  | { kind: 'account'; label: string }
  /** A saved share no mount went through yet: its host's share list, mounting that share. */
  | { kind: 'share_via_host'; host: NetworkHost; share: string; label: string }

/**
 * What Enter does to `row`: the hub's ONE decision about where a row leads.
 *
 * ❗ A host opens its share list and ❌ never mounts a share on its own (the
 * person picked a HOST, cmdr-reports#7). A saved share with a place in the volume
 * list lands the pane on it the way an SFTP place does, and a place that isn't
 * mounted is mounted right there, in the pane. One nothing mounted yet (a share
 * Add named) goes through its host's share list: its first mount is what gives
 * it a place. `null` when a share has no host to go through, which shouldn't
 * happen and is logged by the caller.
 */
export function openMoveFor(row: HubRow, rows: HubRow[], volumes: VolumeInfo[]): HubOpenMove | null {
  // An S3 place is dialed in the pane like a one-place server; only a share goes through its host.
  if (row.kind === 'place' && row.protocol !== 'smb') return { kind: 'place', row }
  if (row.kind === 'place') {
    if (row.volumeId && volumes.some((volume) => volume.id === row.volumeId)) return { kind: 'place', row }
    const server = rows.find((candidate) => candidate.id === row.parentId)
    const host = row.host ?? (server ? savedHostFor(server) : null)
    return host && row.place
      ? { kind: 'share_via_host', host, share: row.place.name, label: server?.name ?? host.name }
      : null
  }
  if (row.protocol === 'smb') return { kind: 'host', host: row.host ?? savedHostFor(row), label: row.name }
  if (row.protocol === 's3') return { kind: 'account', label: row.name }
  return { kind: 'place', row }
}

/**
 * A saved SMB host mDNS isn't seeing right now, as a host the places list can
 * take. Its address is the only spelling anything has for it: `host`, or
 * `host:port` off 445 (the listing's `SavedServer.address`).
 */
export function savedHostFor(row: HubRow): NetworkHost {
  const withPort = /^(.+):(\d+)$/.exec(row.address)
  const [hostname, port] = withPort ? [withPort[1], Number(withPort[2])] : [row.address, SMB_PORT]
  return { id: row.id, name: row.name, hostname, port, source: 'manual' }
}

/** A share is a folder under its server; a server is a machine, or a service on one. */
export function hubRowIcon(row: HubRow): 'folder' | 'monitor' | 'server' {
  if (row.kind === 'place') return 'folder'
  return row.protocol === 'smb' ? 'monitor' : 'server'
}

/** `Last used`, as the seconds-based `DateLabel` takes it, or `null` for never. */
export function lastUsedSeconds(row: HubRow): number | null {
  if (!row.lastConnectedAt) return null
  const parsed = Date.parse(row.lastConnectedAt)
  return Number.isNaN(parsed) ? null : Math.floor(parsed / 1000)
}

/**
 * The most useful spelling of where a discovered host lives, with its port when
 * it isn't 445: `localhost` alone would name another server on the same machine.
 */
function hostAddress(host: NetworkHost | null): string | null {
  const address = host?.ipAddress ?? host?.hostname ?? null
  if (!host || address === null || host.port === SMB_PORT) return address
  return address.includes(':') ? `[${address}]:${String(host.port)}` : `${address}:${String(host.port)}`
}

/**
 * What the person saved first (live, then what's asking for them, then idle, then
 * the ones mDNS also sees), then the hosts Cmdr only found.
 *
 * ❗ The found-only hosts stay CONTIGUOUS at the end, whatever their recency or
 * name: the hub's group header sits in front of the first one.
 */
function compareRows(a: HubRow, b: HubRow): number {
  const byOwnership = Number(isNearbyOnly(a)) - Number(isNearbyOnly(b))
  if (byOwnership !== 0) return byOwnership
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
