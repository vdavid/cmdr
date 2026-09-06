/**
 * What an agent reading `cmdr://state` sees while a pane is on the hub.
 *
 * The tokens are a WIRE: they are what `smb.spec.ts` polls on and what an agent
 * parses, so they stay locale-independent even though the column beside them is
 * translated.
 */

import { describe, it, expect } from 'vitest'
import { hubMcpEntries, ADD_SERVER_MCP_NAME, ADD_SERVER_MCP_PATH } from './servers-hub-mcp'
import type { HubRow } from './servers-hub-rows'

function row(overrides: Partial<HubRow> = {}): HubRow {
  return {
    id: 'sftp-nas.local-22-ada',
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
