/**
 * The hub's list as the cursor walks it: the saved servers, then one header for
 * the servers that were only found nearby, then those servers while the group is
 * open.
 */

import { describe, it, expect } from 'vitest'
import {
  NEARBY_GROUP_ID,
  fullIndexOf,
  hubItems,
  isNearbyGroupExpanded,
  visibleHubItems,
  visibleIndexOf,
} from './servers-hub-items'
import type { HubRow } from './servers-hub-rows'
import type { SavedServer } from '$lib/tauri-commands'

function row(id: string, saved: boolean): HubRow {
  return {
    id,
    kind: 'server',
    parentId: null,
    account: null,
    place: null,
    name: id,
    protocol: 'smb',
    address: `${id}.local`,
    status: saved ? 'saved' : 'found_nearby',
    lastConnectedAt: null,
    volumeId: null,
    pinned: false,
    saved: saved ? ({ id } as SavedServer) : null,
    host: null,
  }
}

const savedRow = (id: string) => row(id, true)
const nearbyRow = (id: string) => row(id, false)
const ids = (items: { id: string }[]) => items.map((item) => item.id)

describe('isNearbyGroupExpanded', () => {
  it('starts collapsed for someone with a saved server: the nearby ones are noise to them', () => {
    expect(isNearbyGroupExpanded('auto', true)).toBe(false)
  })

  it('starts expanded for someone with none: the nearby list is the whole view', () => {
    expect(isNearbyGroupExpanded('auto', false)).toBe(true)
  })

  it('keeps what the person chose, whatever they have saved', () => {
    expect(isNearbyGroupExpanded('expanded', true)).toBe(true)
    expect(isNearbyGroupExpanded('collapsed', false)).toBe(false)
  })
})

describe('hubItems', () => {
  it('puts one header between the saved servers and the nearby ones', () => {
    const items = hubItems([savedRow('nas'), nearbyRow('printer'), nearbyRow('tv')], true)
    expect(ids(items)).toEqual(['nas', NEARBY_GROUP_ID, 'printer', 'tv'])
    expect(items[1]).toEqual({ kind: 'nearby_group', id: NEARBY_GROUP_ID, count: 2, expanded: true })
  })

  it('has no header when nothing was found nearby', () => {
    expect(ids(hubItems([savedRow('nas')], true))).toEqual(['nas'])
    expect(hubItems([], false)).toEqual([])
  })

  it('lists the nearby servers even while the group is collapsed, so an agent sees them', () => {
    const items = hubItems([savedRow('nas'), nearbyRow('printer')], false)
    expect(ids(items)).toEqual(['nas', NEARBY_GROUP_ID, 'printer'])
  })
})

describe('visibleHubItems', () => {
  it('hides the nearby servers of a collapsed group, and keeps its header', () => {
    const items = hubItems([savedRow('nas'), nearbyRow('printer'), nearbyRow('tv')], false)
    expect(ids(visibleHubItems(items))).toEqual(['nas', NEARBY_GROUP_ID])
  })

  it('shows everything while the group is expanded', () => {
    const items = hubItems([savedRow('nas'), nearbyRow('printer')], true)
    expect(visibleHubItems(items)).toEqual(items)
  })
})

describe('the two index spaces', () => {
  // Full:    0 nas, 1 header, 2 printer, 3 tv, 4 "Add server…"
  // Visible: 0 nas, 1 header, 2 "Add server…"
  const collapsed = hubItems([savedRow('nas'), nearbyRow('printer'), nearbyRow('tv')], false)
  const expanded = hubItems([savedRow('nas'), nearbyRow('printer'), nearbyRow('tv')], true)

  it('maps what is on screen to where an agent counts it', () => {
    expect([0, 1, 2].map((at) => fullIndexOf(collapsed, at))).toEqual([0, 1, 4])
    expect([0, 1, 2, 3, 4].map((at) => fullIndexOf(expanded, at))).toEqual([0, 1, 2, 3, 4])
  })

  it('maps an agent’s index back, and says when it names a hidden row', () => {
    expect([0, 1, 4].map((at) => visibleIndexOf(collapsed, at))).toEqual([0, 1, 2])
    expect(visibleIndexOf(collapsed, 2)).toBeNull()
    expect(visibleIndexOf(collapsed, 3)).toBeNull()
    expect([0, 1, 2, 3, 4].map((at) => visibleIndexOf(expanded, at))).toEqual([0, 1, 2, 3, 4])
  })
})
