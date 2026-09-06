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

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
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
  ipc = installIpcMock()
  ipc.mock('list_saved_servers', () => [SAVED_SERVER])
})
afterEach(() => {
  closeSignInSheet({ kind: 'cancelled' })
  clearIpcMocks()
  _setLocaleForTests(null)
})

describe('readSchemeInput: which scheme means what', () => {
  it('navigates to a saved server, by its own name', async () => {
    expect(await readSchemeInput(`${APP_ROOT}/photos`)).toEqual({
      kind: 'place',
      path: `${APP_ROOT}/photos`,
      label: 'Naspolya',
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

describe('previewSchemeInput: what the box says under it', () => {
  it('names what will open, or says a server gets added', () => {
    expect(previewSchemeInput({ kind: 'place', path: APP_ROOT, label: 'Naspolya' })).toBe('Opens Naspolya')
    expect(previewSchemeInput({ kind: 'add', address: 'smb://naspolya' })).toBe('Adds a server')
  })
})

describe('actOnSchemeInput: what the jump does', () => {
  it('reports a place as a directory, so the caller navigates the way it always has', async () => {
    expect(await actOnSchemeInput({ kind: 'place', path: APP_ROOT, label: 'Naspolya' })).toEqual({
      kind: 'directory',
      path: APP_ROOT,
    })
    expect(currentSignInRequest()).toBeNull()
  })

  it('opens the sheet on the address, and answers that it handed over', async () => {
    const acting = actOnSchemeInput({ kind: 'add', address: 'https://cloud.example.com' })
    for (let i = 0; i < 20 && !currentSignInRequest(); i++) {
      await new Promise((resolve) => setTimeout(resolve, 0))
    }
    const request = currentSignInRequest()
    expect(request).toMatchObject({ mode: 'add', prefill: 'https://cloud.example.com' })

    closeSignInSheet({ kind: 'cancelled' })
    expect(await acting).toEqual({ kind: 'handed_off' })
  })
})
