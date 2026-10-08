/**
 * What an agent reading `cmdr://state` sees while a pane is on the hub.
 *
 * The tokens are a WIRE: they are what `smb.spec.ts` polls on and what an agent
 * parses, so they stay locale-independent even though the column beside them is
 * translated.
 */

import { describe, it, expect } from 'vitest'
import {
  hubMcpEntries as entriesOf,
  hubPaneState,
  ADD_SERVER_MCP_NAME,
  ADD_SERVER_MCP_PATH,
  NEARBY_GROUP_MCP_NAME,
  NEARBY_GROUP_MCP_PATH,
  type HubMcpLookups,
} from './servers-hub-mcp'
import { hubItems } from './servers-hub-items'
import type { HubRow } from './servers-hub-rows'
import type { SavedServer } from '$lib/tauri-commands'

/** Saved rows only, so no group: what each entry says is these tests' subject. */
function hubMcpEntries(rows: HubRow[], lookups: HubMcpLookups) {
  return entriesOf(
    hubItems(
      rows.map((r) => ({ ...r, saved: {} as SavedServer })),
      false,
    ),
    lookups,
  )
}

function row(overrides: Partial<HubRow> = {}): HubRow {
  return {
    id: 'sftp-nas.local-22-ada',
    kind: 'server',
    parentId: null,
    account: null,
    place: null,
    name: 'Naspolya',
    protocol: 'sftp',
    address: 'nas.local:22',
    status: 'connected',
    lastConnectedAt: '2026-09-01T10:00:00Z',
    volumeId: 'sftp-nas.local-22-ada',
    pinned: true,
    saved: null,
    host: null,
    ...overrides,
  }
}

describe('hubMcpEntries', () => {
  it('encodes the protocol, the status, and the address into the name', () => {
    const [entry] = hubMcpEntries([row()], { appRootOf: () => 'sftp://ada@nas.local:22' })
    expect(entry.name).toBe('Naspolya  protocol=sftp  status="connected"  address=nas.local:22')
  })

  it('keeps the status token in English even though the column is translated', () => {
    const [entry] = hubMcpEntries([row({ status: 'waiting_for_key' })], { appRootOf: () => 'sftp://x' })
    expect(entry.name).toContain('status="waiting_for_key"')
  })

  it('points a one-place row at the app root, so an agent can navigate to it', () => {
    const [entry] = hubMcpEntries([row()], { appRootOf: () => 'sftp://ada@nas.local:22' })
    expect(entry.path).toBe('sftp://ada@nas.local:22')
  })

  it('points an SMB host at its `smb://` sentinel, the way the host list always has', () => {
    const [entry] = hubMcpEntries([row({ protocol: 'smb', volumeId: null, address: '10.0.0.4' })], {
      appRootOf: () => null,
    })
    expect(entry.path).toBe('smb://10.0.0.4')
  })

  it('carries the share count for an SMB host, which is what an agent polls on', () => {
    const [entry] = hubMcpEntries([row({ protocol: 'smb', volumeId: null, id: 'h1' })], {
      appRootOf: () => null,
      shareCountOf: () => 3,
    })
    expect(entry.name).toContain('shares=3')
  })

  it('leaves the share count off a row that has no share list to count', () => {
    const [entry] = hubMcpEntries([row()], { appRootOf: () => 'sftp://x' })
    expect(entry.name).not.toContain('shares=')
  })

  it('falls back to the row address when nothing knows the app root yet', () => {
    const [entry] = hubMcpEntries([row({ volumeId: null })], { appRootOf: () => null })
    expect(entry.path).toBe('sftp://nas.local:22')
  })

  it('ends with the add row, always, so an agent can find where adding lives', () => {
    const entries = hubMcpEntries([row()], { appRootOf: () => 'sftp://x' })
    expect(entries).toHaveLength(2)
    expect(entries[1].name).toBe(ADD_SERVER_MCP_NAME)
    expect(entries[1].path).toBe(ADD_SERVER_MCP_PATH)
    expect(entries[1].isDirectory).toBe(false)
  })

  it('lists an empty hub as the add row alone', () => {
    expect(hubMcpEntries([], { appRootOf: () => null })).toHaveLength(1)
  })
})

describe('the nearby group, as an agent sees it', () => {
  const saved = row({ id: 'nas', name: 'Naspolya', saved: {} as SavedServer })
  const nearby = (name: string) =>
    row({ id: name, name, protocol: 'smb', status: 'found_nearby', volumeId: null, address: `${name}.local` })
  const lookups = { appRootOf: () => null }

  it('lists the nearby servers while the group is collapsed, with the group saying so', () => {
    const entries = entriesOf(hubItems([saved, nearby('Printer'), nearby('TV')], false), lookups)
    expect(entries.map((entry) => entry.name.split('  ')[0])).toEqual([
      'Naspolya',
      NEARBY_GROUP_MCP_NAME,
      'Printer',
      'TV',
      ADD_SERVER_MCP_NAME,
    ])
    expect(entries[1].name).toBe(`${NEARBY_GROUP_MCP_NAME}  kind=group  state=collapsed  servers=2`)
    expect(entries[1].path).toBe(NEARBY_GROUP_MCP_PATH)
    expect(entries[1].isDirectory).toBe(false)
  })

  it('says when the group is open', () => {
    const entries = entriesOf(hubItems([nearby('Printer')], true), lookups)
    expect(entries[0].name).toBe(`${NEARBY_GROUP_MCP_NAME}  kind=group  state=expanded  servers=1`)
  })

  it('counts the cursor over the full list, which is the one an agent indexes into', () => {
    // On screen: Naspolya, the collapsed group, "Add server…". The cursor is on the add row.
    const items = hubItems([saved, nearby('Printer'), nearby('TV')], false)
    const state = hubPaneState(items, 2, 'Servers', lookups)
    expect(state.cursorIndex).toBe(4)
    expect(state.files[state.cursorIndex].name).toBe(ADD_SERVER_MCP_NAME)
    expect(state.totalFiles).toBe(4)
  })
})
