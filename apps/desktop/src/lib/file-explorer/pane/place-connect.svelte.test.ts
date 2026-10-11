/**
 * The pane's half of opening a saved place: dial on landing, cancel while it
 * runs, reload once it is live, and say why when it isn't.
 *
 * ❗ The one-dial-per-landing rule is what these cells guard. The `$effect`
 * re-runs on every volume-list refresh, and a dial per refresh would be a dial
 * per second against a server the user opened once.
 *
 * Runes (`$effect.root` + `$state`), so the filename carries the `.svelte.`
 * infix.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { VolumeInfo } from '../types'

const { connectPlace, cancelPlaceConnect, resolveValidPath } = vi.hoisted(() => ({
  connectPlace: vi.fn(),
  cancelPlaceConnect: vi.fn().mockResolvedValue(undefined),
  resolveValidPath: vi.fn(),
}))

vi.mock('$lib/servers/connect-flow', () => ({ connectPlace, cancelPlaceConnect }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath }))
vi.mock('$lib/servers/connect-refusals', () => ({
  wordPaneRefusal: (kind: string, subject: { host: string; username: string; name: string }) =>
    `${kind} for ${subject.username} at ${subject.host}, named ${subject.name}`,
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createPlaceConnect, type PlaceConnect } from './place-connect.svelte'
import { notePlacesMoved } from './place-moves.svelte'

const savedPlace: VolumeInfo = {
  id: 'sftp-nas-local-22-ada',
  name: 'Naspolya',
  path: 'sftp://ada@nas.local:22/srv/data',
  category: 'network',
  fsType: 'sftp',
  isEjectable: false,
  connectionState: 'saved',
}

/**
 * A favorite (or a restored tab) enters a saved SERVER at a deep folder, and that
 * folder may be gone by the time the session is up: the pane lands on the deepest
 * one still there, ❌ never a listing error over the missing one.
 */
describe('createPlaceConnect on a server entered at a deep folder', () => {
  let dispose: (() => void) | undefined
  const deep = 'sftp://ada@nas.local:22/srv/data/photos/2026'

  function create(): ReturnType<typeof vi.fn> {
    const enter = vi.fn()
    dispose = $effect.root(() => {
      createPlaceConnect({
        getVolumeId: () => savedPlace.id,
        getCurrentVolumeInfo: () => savedPlace,
        getVolumePath: () => savedPlace.path,
        getCurrentPath: () => deep,
        getEnteredPath: () => deep,
        enter,
      })
    })
    flushSync()
    return enter
  }

  beforeEach(() => {
    vi.clearAllMocks()
    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedPlace.id })
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('keeps the folder when it is still there', async () => {
    resolveValidPath.mockImplementation((path: string) => Promise.resolve(path))
    const enter = create()
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalledWith({ volumeId: savedPlace.id, volumePath: savedPlace.path, targetPath: deep })
    })
  })

  it('lands on the nearest folder still there, asking the server itself', async () => {
    resolveValidPath.mockResolvedValue('sftp://ada@nas.local:22/srv/data/photos')
    const enter = create()
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalledWith({
        volumeId: savedPlace.id,
        volumePath: savedPlace.path,
        targetPath: 'sftp://ada@nas.local:22/srv/data/photos',
      })
    })
    expect(resolveValidPath).toHaveBeenCalledWith(
      deep,
      expect.objectContaining({ volumeRoot: savedPlace.path, volumeId: savedPlace.id }),
    )
  })
})

describe('createPlaceConnect', () => {
  let dispose: (() => void) | undefined
  /** The pane's live `VolumeInfo`, reactive so a reassignment re-runs the factory's effect. */
  let info = $state<VolumeInfo | null>(null)

  function create(): { sub: PlaceConnect; enter: ReturnType<typeof vi.fn> } {
    const enter = vi.fn()
    let sub!: PlaceConnect
    dispose = $effect.root(() => {
      sub = createPlaceConnect({
        getVolumeId: () => info?.id ?? 'root',
        getCurrentVolumeInfo: () => info,
        getVolumePath: () => savedPlace.path,
        getCurrentPath: () => savedPlace.path,
        getEnteredPath: () => savedPlace.path,
        enter,
      })
    })
    flushSync()
    return { sub, enter }
  }

  beforeEach(() => {
    vi.clearAllMocks()
    info = { ...savedPlace }
    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedPlace.id })
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('shows the connecting view the moment the pane lands on a saved place', () => {
    const { sub } = create()
    expect(sub.state?.kind).toBe('connecting')
    expect(connectPlace).toHaveBeenCalledWith(expect.objectContaining({ volumeId: savedPlace.id }))
  })

  it('enters the place again once it is live, and drops the view', async () => {
    const { sub, enter } = create()
    await vi.waitFor(() => {
      // A server place's root never moves on a connect, so it re-enters where it stands.
      expect(enter).toHaveBeenCalledWith({
        volumeId: savedPlace.id,
        volumePath: savedPlace.path,
        targetPath: savedPlace.path,
      })
    })
    expect(sub.state).toBeNull()
  })

  it('leaves a live or local volume alone', () => {
    info = { ...savedPlace, connectionState: 'direct' }
    const { sub } = create()
    expect(sub.state).toBeNull()
    expect(connectPlace).not.toHaveBeenCalled()

    info = { ...savedPlace, id: 'root', path: '/', category: 'main_volume', connectionState: undefined }
    dispose?.()
    const second = create()
    expect(second.sub.state).toBeNull()
    expect(connectPlace).not.toHaveBeenCalled()
  })

  it('dials ONCE per landing, however often the volume list refreshes', () => {
    const { sub } = create()
    // A refresh that changes nothing: a new object, same place, same state.
    info = { ...savedPlace }
    flushSync()
    info = { ...savedPlace }
    flushSync()
    expect(connectPlace).toHaveBeenCalledTimes(1)
    expect(sub.state?.kind).toBe('connecting')
  })

  it('arms Cancel with the attempt id the flow hands out', () => {
    connectPlace.mockImplementation((request: { onAttemptStarted?: (id: string) => void }) => {
      request.onAttemptStarted?.('server-connect-7')
      return new Promise(() => {
        // Never settles: the dial is still running while the user cancels.
      })
    })
    const { sub } = create()
    expect(sub.state?.kind).toBe('connecting')
    if (sub.state?.kind !== 'connecting') throw new Error('not connecting')
    sub.state.cancel()
    expect(cancelPlaceConnect).toHaveBeenCalledWith('server-connect-7')
  })

  it('words a refusal from the place’s own name, host, and account, and offers Try again', async () => {
    connectPlace.mockResolvedValue({ kind: 'refused', refusal: 'unreachable' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    if (sub.state?.kind !== 'refused') throw new Error('not refused')
    // ❗ The name the user gave it rides along, so "unreachable" can say "Naspolya" over "nas.local".
    expect(sub.state.refusal).toBe('unreachable for ada at nas.local, named Naspolya')
    // ❌ No Disconnect on a place with no session to drop.
    expect(sub.state.disconnect).toBeUndefined()

    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedPlace.id })
    // ❗ A server refusal always carries one: the state's `retry` is optional
    // only so a phone that left can render its sentence with no button at all.
    expect(sub.state.retry).toBeTypeOf('function')
    sub.state.retry?.()
    await vi.waitFor(() => {
      expect(sub.state).toBeNull()
    })
    expect(connectPlace).toHaveBeenCalledTimes(2)
  })

  it('words a refusal for an account that is an email address from its own host and account', async () => {
    // Email logins are common on WebDAV hosts. A path that didn't parse put the
    // place's display name in for both, on every refused dial.
    info = {
      ...savedPlace,
      id: 'webdav-cloud-443-ada',
      name: 'Cloud',
      fsType: 'webdav',
      path: 'webdav://ada@example.com@cloud.example.com:443',
    }
    connectPlace.mockResolvedValue({ kind: 'refused', refusal: 'unreachable' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('refused')
    })
    if (sub.state?.kind !== 'refused') throw new Error('not refused')
    expect(sub.state.refusal).toBe('unreachable for ada@example.com at cloud.example.com, named Cloud')
  })

  /**
   * ❗ A cancelled sign-in leaves the pane on the place, saying what it is and
   * offering to connect again. It fell through to the listing's generic "Cmdr
   * hasn't connected to the phone or server holding /Volumes/private-1" with only
   * Go back / Go to home (QA round 6).
   */
  it('names the place and offers to connect again when the user cancels', async () => {
    connectPlace.mockResolvedValueOnce({ kind: 'cancelled' })
    const { sub, enter } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('not_connected')
    })
    expect(enter).not.toHaveBeenCalled()

    connectPlace.mockResolvedValueOnce({ kind: 'connected', volumeId: savedPlace.id })
    const state = sub.state
    if (state?.kind !== 'not_connected') throw new Error('expected not_connected')
    state.connect()
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalled()
    })
    expect(connectPlace).toHaveBeenCalledTimes(2)
  })

  /**
   * ❗ Picking the place again in the switcher, while the pane stands on it not
   * connected, dials it like Try again. It was a no-op (the pane's own volume), so
   * the menu closed and nothing happened (QA round 7).
   */
  it('dials again when the place the pane stands on, not connected, is picked again', async () => {
    connectPlace.mockResolvedValueOnce({ kind: 'cancelled' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(sub.state?.kind).toBe('not_connected')
    })
    connectPlace.mockResolvedValueOnce({ kind: 'cancelled' })
    sub.picked(savedPlace.id)
    await vi.waitFor(() => {
      expect(connectPlace).toHaveBeenCalledTimes(2)
    })
    // Another volume's pick is not this place's business.
    sub.picked('root')
    await Promise.resolve()
    expect(connectPlace).toHaveBeenCalledTimes(2)
  })

  it('does not dial twice when the place is picked while it is still connecting', () => {
    connectPlace.mockReturnValueOnce(new Promise(() => {}))
    const { sub } = create()
    expect(sub.state?.kind).toBe('connecting')
    sub.picked(savedPlace.id)
    expect(connectPlace).toHaveBeenCalledTimes(1)
  })

  /**
   * ❗ A server move follows the pane to the new address while its dial to the old
   * one is still out. The backend calls that dial off, and its late answer is about
   * a place the pane left: it must neither replace the new dial's spinner with "Not
   * connected" nor, had it connected, send the pane back to the old address.
   */
  for (const late of [{ kind: 'cancelled' }, { kind: 'connected', volumeId: savedPlace.id }] as const) {
    it(`ignores the ${late.kind} answer of a dial to a place the pane already left`, async () => {
      let answerOldDial!: (result: typeof late) => void
      connectPlace.mockReturnValueOnce(
        new Promise((resolve) => {
          answerOldDial = resolve
        }),
      )
      connectPlace.mockReturnValueOnce(new Promise(() => {}))
      const { sub, enter } = create()
      info = { ...savedPlace, id: 'sftp-nas-moved-22-ada', path: 'sftp://ada@nas.moved:22/srv/data' }
      flushSync()
      expect(connectPlace).toHaveBeenCalledTimes(2)

      answerOldDial(late)
      await new Promise((resolve) => setTimeout(resolve, 0))
      flushSync()

      expect(sub.state?.kind).toBe('connecting')
      expect(enter).not.toHaveBeenCalled()
    })
  }

  /**
   * ❗ A WebDAV server whose base PATH moved keeps its id, so the pane never leaves
   * the place: its row was `saved` before and after. The move calls the dial to the
   * old URL off, and the pane dials again on its own, at the new one.
   */
  it('dials again when the place it is dialing moves and keeps its id, and ignores the old dial', async () => {
    let answerOldDial!: (result: { kind: 'cancelled' }) => void
    connectPlace.mockReturnValueOnce(
      new Promise((resolve) => {
        answerOldDial = resolve
      }),
    )
    connectPlace.mockReturnValueOnce(new Promise(() => {}))
    const { sub } = create()

    notePlacesMoved([savedPlace.id])
    flushSync()
    expect(connectPlace).toHaveBeenCalledTimes(2)

    answerOldDial({ kind: 'cancelled' })
    await new Promise((resolve) => setTimeout(resolve, 0))
    flushSync()
    expect(sub.state?.kind).toBe('connecting')
  })

  it('keeps the spinner while the reconnect manager owns the recovery', async () => {
    connectPlace.mockResolvedValue({ kind: 'reconnecting' })
    const { sub } = create()
    await vi.waitFor(() => {
      expect(connectPlace).toHaveBeenCalled()
    })
    expect(sub.state?.kind).toBe('connecting')
  })
})

/**
 * ❗ A saved SMB share comes back wherever its NEXT mount sat (`/Volumes/naspi-1`
 * when another server's `naspi` took the plain name), so the pane reloads there
 * rather than at the path the saved row remembered.
 */
describe('createPlaceConnect: a saved SMB share', () => {
  const savedShare: VolumeInfo = {
    id: 'smb-naspolya-445-naspi',
    name: 'naspi on Naspolya',
    path: '/Volumes/naspi',
    category: 'network',
    fsType: 'smbfs',
    isEjectable: false,
    connectionState: 'saved',
  }
  let dispose: (() => void) | undefined
  let shareInfo = $state<VolumeInfo>({ ...savedShare })
  let volumePath = $state('/Volumes/naspi')
  let currentPath = $state('/Volumes/naspi/docs')
  let enteredPath = $state('/Volumes/naspi/docs')

  let sub: PlaceConnect | undefined

  function create(landingOf = vi.fn(() => Promise.resolve<string | null>('/Volumes/naspi'))) {
    const enter = vi.fn()
    dispose = $effect.root(() => {
      sub = createPlaceConnect({
        getVolumeId: () => savedShare.id,
        getCurrentVolumeInfo: () => shareInfo,
        getVolumePath: () => volumePath,
        getCurrentPath: () => currentPath,
        getEnteredPath: () => enteredPath,
        enter,
        landingOf,
      })
    })
    flushSync()
    return enter
  }

  beforeEach(() => {
    vi.clearAllMocks()
    shareInfo = { ...savedShare }
    volumePath = '/Volumes/naspi'
    currentPath = '/Volumes/naspi/docs'
    enteredPath = '/Volumes/naspi/docs'
    connectPlace.mockResolvedValue({ kind: 'connected', volumeId: savedShare.id })
    // Every folder is there unless a cell says otherwise.
    resolveValidPath.mockImplementation((path: string) => Promise.resolve(path))
  })

  /**
   * ❗ A restored tab keeps the folder it was on inside an unmounted share, and that
   * folder may be gone by the time the mount lands: the pane enters the deepest one
   * that's still there, inside the share, ❌ never an error over a missing folder.
   */
  it('enters the nearest folder that still exists once the share is live', async () => {
    currentPath = '/Volumes/naspi/docs/2026'
    resolveValidPath.mockResolvedValue('/Volumes/naspi/docs')
    const enter = create()
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalledWith({
        volumeId: savedShare.id,
        volumePath: '/Volumes/naspi',
        targetPath: '/Volumes/naspi/docs',
      })
    })
    expect(resolveValidPath).toHaveBeenCalledWith(
      '/Volumes/naspi/docs/2026',
      expect.objectContaining({ volumeRoot: '/Volumes/naspi', volumeId: savedShare.id }),
    )
  })

  it('enters the share root when the walk finds nothing', async () => {
    currentPath = '/Volumes/naspi/docs/2026'
    resolveValidPath.mockResolvedValue(null)
    const enter = create()
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalledWith({
        volumeId: savedShare.id,
        volumePath: '/Volumes/naspi',
        targetPath: '/Volumes/naspi',
      })
    })
  })

  afterEach(() => {
    dispose?.()
  })

  /**
   * ❗ The pane ENTERS the volume at the path the mount got, the same route a
   * switcher pick takes: root, path, listing, and disk space all move together.
   * Reloading only the listing left the pane's root at the old path, and its
   * missing-folder poll walked it to Macintosh HD (QA round 4, R3-A case 1).
   */
  it('enters the share where the mount landed when that moved, keeping the folder inside it', async () => {
    const enter = create(vi.fn(() => Promise.resolve<string | null>('/Volumes/naspi-1')))
    await vi.waitFor(() => {
      expect(enter).toHaveBeenCalledWith({
        volumeId: savedShare.id,
        volumePath: '/Volumes/naspi-1',
        targetPath: '/Volumes/naspi-1/docs',
      })
    })
  })

  /**
   * ❗ A pane whose volume and path disagree could write to the wrong server:
   * 11480's share listed at `/Volumes/public`, which 11482's mount held (QA round
   * 4, R3-A case 2). Whatever put it there, the pane follows its LIVE row's path.
   */
  it('follows a live share whose mount path differs from where the pane stands', () => {
    shareInfo = { ...savedShare, path: '/Volumes/naspi-1', category: 'attached_volume', connectionState: 'direct' }
    const enter = create()
    expect(connectPlace, 'a live share is not dialed').not.toHaveBeenCalled()
    expect(enter).toHaveBeenCalledWith({
      volumeId: savedShare.id,
      volumePath: '/Volumes/naspi-1',
      targetPath: '/Volumes/naspi-1/docs',
    })
  })

  /**
   * ❗ The pane's root can match the live mount while the folder it stands in is the
   * stale saved one: after a Cancel whose kernel mount finished anyway, the pane sat
   * at `/Volumes/public-1` over a share now at `/Volumes/public`, listing "Not
   * connected yet" (final QA). Standing outside the live mount is followed too.
   */
  it('follows a live share when the pane stands outside its mount, even with the right root', () => {
    shareInfo = { ...savedShare, category: 'attached_volume', connectionState: 'direct' }
    currentPath = '/Volumes/naspi-1'
    enteredPath = '/Volumes/naspi-1'
    const enter = create()
    expect(enter).toHaveBeenCalledWith({
      volumeId: savedShare.id,
      volumePath: '/Volumes/naspi',
      targetPath: '/Volumes/naspi',
    })
  })

  /**
   * ❗ A dial that answers "already live" lands where the share IS live, ❌ never the
   * saved row's remembered path: a mount that finished after a Cancel never updated
   * that row, and Try again landed on it and read "Not connected yet" (final QA).
   */
  it('lands an already-live share at its live mount, not the stale saved landing', async () => {
    connectPlace.mockImplementationOnce(() => {
      shareInfo = { ...savedShare, path: '/Volumes/naspi-2', category: 'attached_volume', connectionState: 'direct' }
      return Promise.resolve({ kind: 'already_live' })
    })
    const landingOf = vi.fn(() => Promise.resolve<string | null>('/Volumes/naspi-1'))
    const enter = create(landingOf)
    await vi.waitFor(() => {
      expect(connectPlace).toHaveBeenCalledOnce()
      expect(enter).toHaveBeenCalled()
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
    const roots = enter.mock.calls.map(([change]) => (change as { volumePath: string }).volumePath)
    expect(roots).not.toContain('/Volumes/naspi-1')
    expect(roots.at(-1)).toBe('/Volumes/naspi-2')
  })

  /**
   * ❗ A Cancel that lands after the mount already went through leaves no
   * "isn't connected" view over a live share: the view stayed while the header's dot
   * was green, and Try again read "Not connected yet" (final QA).
   */
  it('drops the not-connected view when the cancelled mount finished anyway', async () => {
    connectPlace.mockImplementationOnce(() => {
      shareInfo = { ...savedShare, path: '/Volumes/naspi-2', category: 'attached_volume', connectionState: 'direct' }
      return Promise.resolve({ kind: 'cancelled' })
    })
    const enter = create()
    await vi.waitFor(() => {
      expect(connectPlace).toHaveBeenCalledOnce()
    })
    await new Promise((resolve) => setTimeout(resolve, 0))
    flushSync()
    expect(sub?.state).toBeNull()
    expect(enter).toHaveBeenLastCalledWith({
      volumeId: savedShare.id,
      volumePath: '/Volumes/naspi-2',
      targetPath: '/Volumes/naspi-2/docs',
    })
  })

  it('leaves a live share alone when the pane already stands on its mount path', () => {
    shareInfo = { ...savedShare, category: 'attached_volume', connectionState: 'direct' }
    const enter = create()
    expect(enter).not.toHaveBeenCalled()
  })

  /**
   * ❗ A switch onto a live share from another volume commits the share and its folder
   * together, but the pane's own folder catches up an effect later, so for one run it
   * still names the PREVIOUS volume's folder. Reading that as "outside the mount"
   * re-entered the share at its root, and the switch then landed on the remembered
   * folder: Go to path `/Volumes/public/docs` from `~/Downloads` came to rest at
   * `/Volumes/public` (verified in the dev app, 2026-10-10).
   */
  it('leaves a live share alone while the pane catches up with a switch onto it', () => {
    shareInfo = { ...savedShare, category: 'attached_volume', connectionState: 'direct' }
    currentPath = '/Users/ada/Downloads'
    const enter = create()
    expect(enter).not.toHaveBeenCalled()
  })
})
