/**
 * Which server the palette's server commands act on.
 *
 * The hub IS a pane, so "the focused pane's volume" answers the synthetic hub row
 * rather than the server the cursor is on. The cursor row wins wherever there is
 * one; the pane's own volume is the fallback for a pane standing inside a server.
 */

import { describe, it, expect } from 'vitest'
import { serverCommandTarget } from './server-command-target'
import type { HubRow } from '$lib/file-explorer/network/servers-hub-rows'
import type { VolumeInfo } from '$lib/file-explorer/types'

function row(overrides: Partial<HubRow> = {}): HubRow {
  return {
    id: 'sftp-nas.local-22-ada',
    name: 'Naspolya',
    protocol: 'sftp',
    address: 'nas.local:22',
    status: 'saved',
    lastConnectedAt: null,
    volumeId: 'sftp-nas.local-22-ada',
    pinned: true,
    saved: null,
    host: null,
    ...overrides,
  }
}

function volume(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return {
    id: 'webdav-nas.local-5006-ada',
    name: 'Nextcloud',
    path: 'webdav://ada@nas.local:5006',
    category: 'network',
    isEjectable: false,
    fsType: 'webdav',
    connectionState: 'direct',
    ...overrides,
  }
}

describe('serverCommandTarget', () => {
  it('takes the hub row under the cursor when the focused pane is the hub', () => {
    const target = serverCommandTarget({ cursorRow: row(), paneVolume: null })
    expect(target).toEqual({ volumeId: 'sftp-nas.local-22-ada', name: 'Naspolya', pinned: true })
  })

  it('falls back to the focused pane’s own volume when it is a server place', () => {
    const target = serverCommandTarget({ cursorRow: null, paneVolume: volume() })
    expect(target?.volumeId).toBe('webdav-nas.local-5006-ada')
    expect(target?.name).toBe('Nextcloud')
  })

  it('prefers the cursor row over the pane volume, because the cursor is what the user is looking at', () => {
    const target = serverCommandTarget({ cursorRow: row(), paneVolume: volume() })
    expect(target?.volumeId).toBe('sftp-nas.local-22-ada')
  })

  it('answers nothing for an SMB host row: its places are mounted shares, not one place', () => {
    expect(serverCommandTarget({ cursorRow: row({ protocol: 'smb', volumeId: null }), paneVolume: null })).toBeNull()
  })

  it('answers nothing for the synthetic hub volume, which is not a server', () => {
    const hub = volume({ id: 'network', name: 'Servers', path: 'smb://', fsType: 'smbfs', connectionState: null })
    expect(serverCommandTarget({ cursorRow: null, paneVolume: hub })).toBeNull()
  })

  it('answers nothing for a mounted SMB share: its session is an OS mount the family doesn’t speak', () => {
    const share = volume({ id: 'smb-share-1', name: 'Public', path: '/Volumes/Public', fsType: 'smbfs' })
    expect(serverCommandTarget({ cursorRow: null, paneVolume: share })).toBeNull()
  })

  it('answers nothing for a local disk', () => {
    const local = volume({ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', fsType: 'apfs' })
    expect(serverCommandTarget({ cursorRow: null, paneVolume: local })).toBeNull()
  })

  it('reports a pane volume’s pin as unknown, since a volume row carries none', () => {
    expect(serverCommandTarget({ cursorRow: null, paneVolume: volume() })?.pinned).toBeNull()
  })
})
