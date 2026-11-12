/**
 * What the NFS form asks a server before any place exists: its exports, and who owns
 * the files in the one picked. The two questions race the person's typing, so an
 * answer for a host they've since typed past must never land.
 */

import { describe, expect, it, vi } from 'vitest'
import type { GuessedNfsIdentity, NfsExportList } from '#lib/ipc/bindings.js'
import { createNfsDiscovery } from './nfs-discovery.svelte'

/** A promise the test settles by hand, so two answers can arrive out of order. */
function deferred<T>() {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

const settle = () => new Promise((r) => setTimeout(r, 0))

const UNREACHABLE: NfsExportList = { outcome: 'unreachable' }

describe('createNfsDiscovery: the export list', () => {
  it('lists, folding a timeout into unreachable', async () => {
    const answers: NfsExportList[] = [
      { outcome: 'listed', exports: ['/Public', '/volume1/data'] },
      { outcome: 'timed_out' },
    ]
    const discovery = createNfsDiscovery({
      listNfsExports: vi.fn(() => Promise.resolve(answers.shift() ?? UNREACHABLE)),
      guessNfsIdentity: vi.fn(),
    })
    await discovery.listFor({ host: 'testnas.local', port: null })
    expect(discovery.listing).toEqual({ kind: 'listed', exports: ['/Public', '/volume1/data'] })
    await discovery.listFor({ host: 'testnas.local', port: null })
    expect(discovery.listing).toEqual({ kind: 'unreachable' })
  })

  it('reads a refused listing and a server with no v3 apart', async () => {
    const answers: NfsExportList[] = [{ outcome: 'listing_refused' }, { outcome: 'no_nfs_v3' }]
    const discovery = createNfsDiscovery({
      listNfsExports: vi.fn(() => Promise.resolve(answers.shift() ?? UNREACHABLE)),
      guessNfsIdentity: vi.fn(),
    })
    await discovery.listFor({ host: 'nas', port: 15481 })
    expect(discovery.listing).toEqual({ kind: 'refused' })
    await discovery.listFor({ host: 'nas', port: 15481 })
    expect(discovery.listing).toEqual({ kind: 'no_nfs_v3' })
  })

  it('shows loading while asking, and drops an answer for a host typed past', async () => {
    const first = deferred<NfsExportList>()
    const second = deferred<NfsExportList>()
    const queue = [first.promise, second.promise]
    const discovery = createNfsDiscovery({
      listNfsExports: vi.fn(() => queue.shift() ?? Promise.resolve(UNREACHABLE)),
      guessNfsIdentity: vi.fn(),
    })
    void discovery.listFor({ host: 'nas', port: null })
    expect(discovery.listing).toEqual({ kind: 'loading' })
    void discovery.listFor({ host: 'testnas.local', port: null })
    second.resolve({ outcome: 'listed', exports: ['/right'] })
    await settle()
    first.resolve({ outcome: 'listed', exports: ['/stale'] })
    await settle()
    expect(discovery.listing).toEqual({ kind: 'listed', exports: ['/right'] })
  })

  it('reads a broken bridge as unreachable, and forgets the list on clear', async () => {
    const discovery = createNfsDiscovery({
      listNfsExports: vi.fn(() => Promise.reject(new Error('bridge down'))),
      guessNfsIdentity: vi.fn(),
    })
    await discovery.listFor({ host: 'nas', port: null })
    expect(discovery.listing).toEqual({ kind: 'unreachable' })
    discovery.clear()
    expect(discovery.listing).toEqual({ kind: 'idle' })
  })
})

describe('createNfsDiscovery: the identity guess', () => {
  const guess: GuessedNfsIdentity = { uid: 1000, gid: 100, otherGids: [], source: 'most_common_owner' }

  it('answers the guess for the export asked about', async () => {
    const guessNfsIdentity = vi.fn(() => Promise.resolve(guess))
    const discovery = createNfsDiscovery({ listNfsExports: vi.fn(), guessNfsIdentity })
    expect(await discovery.guessFor({ host: 'nas', port: null, exportPath: '/Public' })).toEqual(guess)
    expect(guessNfsIdentity).toHaveBeenCalledWith({ host: 'nas', port: null, exportPath: '/Public' })
  })

  it('answers nothing for an export typed past, or when the bridge breaks', async () => {
    const first = deferred<GuessedNfsIdentity>()
    const queue = [first.promise, Promise.resolve(guess)]
    const discovery = createNfsDiscovery({
      listNfsExports: vi.fn(),
      guessNfsIdentity: vi.fn(() => queue.shift() ?? Promise.reject(new Error('bridge down'))),
    })
    const stale = discovery.guessFor({ host: 'nas', port: null, exportPath: '/Pub' })
    expect(await discovery.guessFor({ host: 'nas', port: null, exportPath: '/Public' })).toEqual(guess)
    first.resolve({ uid: 1, gid: 1, otherGids: [], source: 'root_owner' })
    expect(await stale).toBeNull()
    expect(await discovery.guessFor({ host: 'nas', port: null, exportPath: '/Public' })).toBeNull()
  })
})
