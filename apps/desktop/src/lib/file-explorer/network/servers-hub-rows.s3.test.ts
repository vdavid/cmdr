/**
 * An S3 account in the hub: the SMB host → shares shape, with buckets for shares.
 *
 * ❗ The account is never navigable itself (its places are), and every place row
 * acts through its OWN volume id: the account's id is the root place's id, which
 * names the account whether or not the root is saved, so acting on it would dial
 * or pin something the person never pointed at.
 */

import { describe, it, expect } from 'vitest'
import { buildHubRows, hubRowIcon, openMoveFor } from './servers-hub-rows'
import type { SavedPlace, SavedServer } from '$lib/tauri-commands'
import type { VolumeInfo } from '../types'

const ACCOUNT_ID = 's3-127-0-0-1-14480-akiaexample-0000'

function place(bucket: string | null, overrides: Partial<SavedPlace> = {}): SavedPlace {
  const volumeId = bucket === null ? ACCOUNT_ID : `s3-127-0-0-1-14480-akiaexample-${bucket}`
  return {
    volumeId,
    name: bucket ?? 'AKIAEXAMPLE@127.0.0.1',
    pinned: true,
    connected: false,
    appRoot: bucket === null ? 's3://AKIAEXAMPLE@127.0.0.1:14480/' : `s3://AKIAEXAMPLE@127.0.0.1:14480/${bucket}`,
    username: 'AKIAEXAMPLE',
    autoReconnect: true,
    ...overrides,
  }
}

function s3Account(places: SavedPlace[], overrides: Partial<SavedServer> = {}): SavedServer {
  return {
    id: ACCOUNT_ID,
    protocol: 's3',
    displayName: 'AKIAEXAMPLE@127.0.0.1',
    nameSource: 'fallback',
    address: 'http://127.0.0.1:14480',
    username: 'AKIAEXAMPLE',
    pinned: false,
    lastConnectedAt: '2026-10-01T10:00:00Z',
    autoReconnect: true,
    places,
    ...overrides,
  }
}

function s3Volume(id: string, connectionState: VolumeInfo['connectionState']): VolumeInfo {
  return { id, name: id, path: 's3://x', category: 'network', isEjectable: false, fsType: 's3', connectionState }
}

describe('an S3 account in the hub', () => {
  it('is one account row with a row per saved place right under it, in name order', () => {
    const rows = buildHubRows({
      saved: [s3Account([place('scans'), place('photos')])],
      hosts: [],
      volumes: [],
    })
    expect(rows.map((row) => [row.kind, row.name])).toEqual([
      ['server', 'AKIAEXAMPLE@127.0.0.1'],
      ['place', 'photos'],
      ['place', 'scans'],
    ])
    expect(rows[1].parentId).toBe(ACCOUNT_ID)
    expect(rows.every((row) => row.protocol === 's3')).toBe(true)
  })

  it('calls the account-root place "All buckets", since the account row above it already carries the name', () => {
    // The backend labels the root as its account (what the switcher and a pane show);
    // under the account's own row, that would read as the same name twice.
    const rows = buildHubRows({
      saved: [s3Account([place('photos'), place(null)], { displayName: 'Work', nameSource: 'user' })],
      hosts: [],
      volumes: [],
    })
    expect(rows.map((row) => [row.kind, row.name])).toEqual([
      ['server', 'Work'],
      ['place', 'All buckets'],
      ['place', 'photos'],
    ])
  })

  it('gives the account row no place of its own: no volume id and no pin', () => {
    // Its id is the ROOT place's id, saved or not, so a row that acted through it
    // would dial or pin the account root behind the person's back.
    const [account] = buildHubRows({ saved: [s3Account([place('photos')])], hosts: [], volumes: [] })
    expect(account.volumeId).toBeNull()
    expect(account.pinned).toBe(false)
  })

  it('lets each place act through its own volume id and pin', () => {
    const rows = buildHubRows({
      saved: [s3Account([place('photos', { pinned: false }), place(null)])],
      hosts: [],
      volumes: [],
    })
    const photos = rows.find((row) => row.name === 'photos')
    expect(photos?.volumeId).toBe('s3-127-0-0-1-14480-akiaexample-photos')
    expect(photos?.pinned).toBe(false)
    // The account root, when saved, is a place like any bucket.
    expect(rows.find((row) => row.kind === 'place' && row.volumeId === ACCOUNT_ID)).toBeDefined()
  })

  it('reads each place’s status off the volume list, and the account’s off its places', () => {
    const rows = buildHubRows({
      saved: [s3Account([place('photos'), place('scans')])],
      hosts: [],
      volumes: [
        s3Volume('s3-127-0-0-1-14480-akiaexample-photos', 'direct'),
        s3Volume('s3-127-0-0-1-14480-akiaexample-scans', 'needs_sign_in'),
      ],
    })
    expect(rows.find((row) => row.name === 'photos')?.status).toBe('connected')
    expect(rows.find((row) => row.name === 'scans')?.status).toBe('signed_out')
    // A live place makes the account live, the way a mounted share would.
    expect(rows[0].status).toBe('connected')
  })

  it('opens a place in the pane and says the account row is not a place', () => {
    const rows = buildHubRows({ saved: [s3Account([place('photos')])], hosts: [], volumes: [] })
    expect(openMoveFor(rows[1], rows, [])).toEqual({ kind: 'place', row: rows[1] })
    expect(openMoveFor(rows[0], rows, [])).toEqual({ kind: 'account', label: rows[0].name })
  })

  it('draws a bucket as a folder under a server', () => {
    const rows = buildHubRows({ saved: [s3Account([place('photos')])], hosts: [], volumes: [] })
    expect(hubRowIcon(rows[0])).toBe('server')
    expect(hubRowIcon(rows[1])).toBe('folder')
  })
})
