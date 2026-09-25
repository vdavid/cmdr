/**
 * What Go to path does with a pasted address, and the one thing it must never
 * do with one: hand it to the local resolver.
 *
 * ❗ `resolve_go_to_path` walks `std::fs::metadata` over a path joined onto the
 * pane's directory. Given `sftp://ada@nas.local:22/srv` it answers `invalid`,
 * and the dialog would tell someone their own NAS address doesn't exist. The
 * last cell in each block is what catches a scheme leaking through.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { clearIpcMocks, installIpcMock, type IpcRecorder } from '$lib/ipc/test-helpers'

const { warn } = vi.hoisted(() => ({ warn: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { _setLocaleForTests } from '$lib/intl/locale'
import { actOnSchemeInput, previewSchemeInput, readSchemeInput } from './scheme-intercept'
import { closeSignInSheet, currentSignInRequest } from '$lib/servers/sign-in-sheet-state.svelte'

const APP_ROOT = 'sftp://ada@nas.local:22/srv/data'

const SAVED_SERVER = {
  id: 'sftp-nas',
  protocol: 'sftp',
  displayName: 'Naspolya',
  address: 'nas.local:22',
  username: 'ada',
  pinned: true,
  lastConnectedAt: null,
  places: [{ volumeId: 'sftp-nas', name: 'Naspolya', pinned: true, connected: false, appRoot: APP_ROOT }],
}

let ipc: IpcRecorder

beforeEach(() => {
  _setLocaleForTests('en-US')
  warn.mockClear()
  ipc = installIpcMock()
  ipc.mock('list_saved_servers', () => [SAVED_SERVER])
})
afterEach(() => {
  closeSignInSheet({ kind: 'cancelled' })
  clearIpcMocks()
  _setLocaleForTests(null)
})

describe('readSchemeInput: a saved-servers store that does not answer', () => {
  it('keeps a password typed into a server path out of the log', async () => {
    // Warn lines reach the log file and every error-report bundle, and a server
    // path's account accepts `user:password` as readily as `user`.
    ipc.mock('list_saved_servers', () => {
      throw new Error('store unavailable')
    })

    await readSchemeInput('sftp://ada:hunter2@nas.local:22/srv')

    expect(warn).toHaveBeenCalledOnce()
    expect(JSON.stringify(warn.mock.calls)).not.toContain('hunter2')
    // Still names the machine, so the line stays useful at triage.
    expect(warn.mock.calls[0][1]).toMatchObject({ host: 'nas.local' })
  })

  it('keeps the password out of the log for an account that is an email address, too', async () => {
    // ❗ The account reads up to the LAST at sign, so a password typed after an
    // email login lands inside the account and never in the host the line names.
    ipc.mock('list_saved_servers', () => {
      throw new Error('store unavailable')
    })

    await readSchemeInput('webdav://ada@example.com:hunter2@cloud.example.com:443/remote.php')

    expect(warn).toHaveBeenCalledOnce()
    expect(JSON.stringify(warn.mock.calls)).not.toContain('hunter2')
    expect(warn.mock.calls[0][1]).toMatchObject({ host: 'cloud.example.com' })
  })
})

describe('readSchemeInput: which scheme means what', () => {
  it('navigates to a saved server, by its own name', async () => {
    expect(await readSchemeInput(`${APP_ROOT}/photos`)).toEqual({
      kind: 'place',
      path: `${APP_ROOT}/photos`,
      label: 'Naspolya',
    })
  })

  it('navigates to a saved place whose account is an email address', async () => {
    // Email logins are common on WebDAV hosts. A path to such a place that didn't
    // parse opened the add sheet over a server that is already saved.
    const root = 'webdav://ada@example.com@cloud.example.com:443'
    ipc.mock('list_saved_servers', () => [
      {
        ...SAVED_SERVER,
        id: 'webdav-cloud',
        protocol: 'webdav',
        places: [{ ...SAVED_SERVER.places[0], volumeId: 'webdav-cloud', name: 'Cloud', appRoot: root }],
      },
    ])
    expect(await readSchemeInput(`${root}/remote.php`)).toEqual({
      kind: 'place',
      path: `${root}/remote.php`,
      label: 'Cloud',
    })
  })

  it('adds a server path nothing saved, rather than calling it a dead end', async () => {
    // Someone pasted a link to a server they haven't added yet, which is an
    // ADDRESS. The sheet is what adds it.
    expect(await readSchemeInput('sftp://ada@other.local:22/srv')).toEqual({
      kind: 'add',
      address: 'sftp://ada@other.local:22/srv',
    })
  })

  it('navigates straight to a device', async () => {
    expect(await readSchemeInput('adb://ABC123/sdcard')).toEqual({
      kind: 'place',
      path: 'adb://ABC123/sdcard',
      label: 'ABC123',
    })
    expect(await readSchemeInput('mtp://PIXEL7')).toMatchObject({ kind: 'place', label: 'PIXEL7' })
  })

  it.each(['smb://naspolya', 'ssh://ada@nas.local', 'davs://cloud.example.com/dav', 'https://cloud.example.com'])(
    'reads %j as an address to add',
    async (input) => {
      expect(await readSchemeInput(input)).toEqual({ kind: 'add', address: input })
    },
  )

  it.each(['~/Documents', '/srv/data', 'Downloads', '', '   ', 'ftp://nas.local'])(
    'leaves %j to the local resolver',
    async (input) => {
      // ❗ A bare hostname is a legal RELATIVE path, and Go to path has always
      // resolved those. Only a scheme is intercepted.
      expect(await readSchemeInput(input)).toBeNull()
    },
  )

  it('❌ never asks the local resolver about a scheme', async () => {
    for (const input of [`${APP_ROOT}/photos`, 'smb://naspolya', 'adb://ABC123']) {
      await readSchemeInput(input)
    }
    expect(ipc.callCount('resolve_go_to_path')).toBe(0)
  })

  it('opens the sheet rather than navigating when the saved list is unreadable', async () => {
    // One extra step for the user, ❌ never a wrong destination.
    ipc.mock('list_saved_servers', () => {
      throw new Error('the store is busy')
    })
    expect(await readSchemeInput(APP_ROOT)).toEqual({ kind: 'add', address: APP_ROOT })
  })
})

describe('readSchemeInput: a search-results path', () => {
  it('reads a snapshot URL as its own thing, never as a server address', async () => {
    // Left to the local resolver it joins onto the pane's folder, misses, and
    // walks the pane to the nearest existing ancestor — a jump nobody asked for,
    // reported as a success.
    expect(await readSchemeInput('search-results://sr-9')).toEqual({ kind: 'snapshot' })
  })

  it('reads the bare scheme the same way', async () => {
    expect(await readSchemeInput('search-results://')).toEqual({ kind: 'snapshot' })
  })
})

describe('previewSchemeInput: what the box says under it', () => {
  it('names what will open, or says a server gets added', () => {
    expect(previewSchemeInput({ kind: 'place', path: APP_ROOT, label: 'Naspolya' })).toBe('Opens Naspolya')
    expect(previewSchemeInput({ kind: 'add', address: 'smb://naspolya' })).toBe('Adds a server')
  })

  it('says a search-results path is not somewhere to go', () => {
    expect(previewSchemeInput({ kind: 'snapshot' })).toBe('Search results aren’t a path you can go to.')
  })
})

describe('actOnSchemeInput: what the jump does', () => {
  it('reports a place as a directory, so the caller navigates the way it always has', async () => {
    expect(
      await actOnSchemeInput(
        { kind: 'place', path: APP_ROOT, label: 'Naspolya' },
        { onSmbHandOff: () => {}, onConnected: () => {} },
      ),
    ).toEqual({
      kind: 'directory',
      path: APP_ROOT,
    })
    expect(currentSignInRequest()).toBeNull()
  })

  it('opens the sheet on the address, and answers that it handed over', async () => {
    const acting = actOnSchemeInput(
      { kind: 'add', address: 'https://cloud.example.com' },
      { onSmbHandOff: () => {}, onConnected: () => {} },
    )
    for (let i = 0; i < 20 && !currentSignInRequest(); i++) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    const request = currentSignInRequest()
    expect(request).toMatchObject({ mode: 'add', prefill: 'https://cloud.example.com' })

    closeSignInSheet({ kind: 'cancelled' })
    expect(await acting).toEqual({ kind: 'handed_off' })
  })

  /**
   * ❗ Go to path is a NAVIGATION command, so an SMB address has to land the
   * person somewhere. Its connect is a share MOUNT rather than a session, so
   * there is no volume to navigate to and the host's places list is the
   * destination — which is exactly where ⌘K's own hand-off goes. Two entry points
   * into one sheet ending differently for one input is the bug.
   */
  it('sends an SMB address on to the hub, the same place ⌘K does', async () => {
    ipc.mock('connect_to_server', () => ({ host: { id: 'h1', name: 'naspolya' }, sharePath: null }))
    let handedOver = 0
    const acting = actOnSchemeInput(
      { kind: 'add', address: 'smb://naspolya' },
      {
        onSmbHandOff: () => {
          handedOver++
        },
        onConnected: () => {},
      },
    )
    for (let i = 0; i < 20 && !currentSignInRequest(); i++) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    const request = currentSignInRequest()
    if (request?.mode !== 'add') throw new Error('expected the add sheet')

    expect(await request.attempt({ mode: 'add_smb', address: 'naspolya' })).toEqual({ kind: 'handed_off' })
    expect(handedOver).toBe(1)

    closeSignInSheet({ kind: 'handed_off' })
    expect(await acting).toEqual({ kind: 'handed_off' })
  })

  it('lands on the place an SFTP or WebDAV address connected, the same place ⌘K does', async () => {
    const landed: unknown[] = []
    const acting = actOnSchemeInput(
      { kind: 'add', address: 'sftp://ada@nas.local:22/srv/data' },
      { onSmbHandOff: () => {}, onConnected: (place) => landed.push(place) },
    )
    for (let i = 0; i < 20 && !currentSignInRequest(); i++) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }

    closeSignInSheet({ kind: 'connected', volumeId: SAVED_SERVER.places[0].volumeId })
    expect(await acting).toEqual({ kind: 'handed_off' })
    expect(landed).toEqual([{ volumeId: SAVED_SERVER.places[0].volumeId, root: APP_ROOT }])
  })
})
