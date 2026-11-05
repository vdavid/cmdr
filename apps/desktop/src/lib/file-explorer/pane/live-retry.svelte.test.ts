/**
 * A pane whose listing failed while its volume wasn't live lists it the moment the
 * volume is, by whatever route it got there, with no timing assumptions.
 *
 * ❗ The race it closes (final QA, 4/4): a Cancel within ~20 ms of picking a saved
 * share left the pane on "Not connected yet /Volumes/public" while the kernel mount
 * finished and the header dot turned green; only re-picking recovered it.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { VolumeInfo } from '../types'
import { createLiveRetry } from './live-retry.svelte'

const share = (connectionState: VolumeInfo['connectionState']): VolumeInfo => ({
  id: 'smb-localhost-11482-public',
  name: 'public on localhost:11482',
  path: '/Volumes/public',
  category: 'attached_volume',
  fsType: 'smbfs',
  isEjectable: true,
  connectionState,
})

describe('createLiveRetry', () => {
  let dispose: (() => void) | undefined
  let info = $state<VolumeInfo | null>(null)
  let failed = $state(false)

  function create() {
    const retry = vi.fn()
    dispose = $effect.root(() => {
      createLiveRetry({ getVolumeInfo: () => info, hasListingError: () => failed, retry })
    })
    flushSync()
    return retry
  }

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('lists the volume once it goes live after the listing failed', () => {
    info = share('saved')
    failed = true
    const retry = create()
    expect(retry).not.toHaveBeenCalled()

    info = share('direct')
    flushSync()
    expect(retry).toHaveBeenCalledOnce()
  })

  it('lists it when the failure arrives after the volume is already live', () => {
    info = share('direct')
    failed = false
    const retry = create()
    expect(retry).not.toHaveBeenCalled()

    failed = true
    flushSync()
    expect(retry).toHaveBeenCalledOnce()
  })

  it('retries once per live spell, so a listing that fails again doesn’t loop', () => {
    info = share('direct')
    failed = true
    const retry = create()
    expect(retry).toHaveBeenCalledOnce()

    failed = false
    flushSync()
    failed = true
    flushSync()
    expect(retry, 'the same live spell').toHaveBeenCalledOnce()

    info = share('disconnected')
    flushSync()
    info = share('direct')
    flushSync()
    expect(retry, 'a new live spell tries again').toHaveBeenCalledTimes(2)
  })

  it('tries again when a share on the macOS mount gets its direct session (#966)', () => {
    // Under "Always require SMB encryption" the unencrypted mount is refused, and the direct,
    // encrypted session landing is what can serve the folder.
    info = share('os_mount')
    failed = true
    const retry = create()
    expect(retry, 'the os_mount spell tries once').toHaveBeenCalledOnce()

    info = share('direct')
    flushSync()
    expect(retry, 'the direct spell is a new one').toHaveBeenCalledTimes(2)
  })

  it('leaves a local volume and a pane with no error alone', () => {
    info = { ...share(null), connectionState: null }
    failed = true
    const retry = create()
    info = share('direct')
    failed = false
    flushSync()
    expect(retry).not.toHaveBeenCalled()
  })
})
