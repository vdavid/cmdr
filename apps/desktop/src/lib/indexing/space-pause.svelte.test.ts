import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { Freshness, IndexFreshnessChangedEvent } from '#lib/ipc/bindings.js'

const { statusMock, listeners, stops } = vi.hoisted(() => ({
  statusMock: vi.fn(),
  listeners: [] as ((payload: IndexFreshnessChangedEvent) => void)[],
  stops: [] as (() => void)[],
}))

vi.mock('#lib/tauri-commands/indexing.js', () => ({
  getVolumeIndexStatusById: statusMock,
  onIndexFreshnessChanged: (cb: (payload: IndexFreshnessChangedEvent) => void) => {
    listeners.push(cb)
    const stop = vi.fn()
    stops.push(stop)
    return Promise.resolve(stop)
  },
}))

import {
  destroySpacePause,
  getSpacePause,
  initSpacePause,
  pausedSizeTooltipKey,
  spacePauseFromStatus,
} from './space-pause.svelte'

function answer(freshness: Freshness | null, holdsIndexStorage: boolean) {
  statusMock.mockResolvedValue({ status: 'ok', data: { freshness, holdsIndexStorage } })
}

function emit(volumeId: string, freshness: Freshness) {
  for (const cb of listeners) cb({ volumeId, freshness })
}

async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('space pause', () => {
  beforeEach(async () => {
    listeners.length = 0
    statusMock.mockReset()
    answer(null, false)
    await initSpacePause()
  })

  afterEach(() => {
    destroySpacePause()
  })

  it('reads a paused status, and nothing else, as a pause', () => {
    expect(spacePauseFromStatus({ freshness: 'paused', holdsIndexStorage: true })).toEqual({ holdsIndexStorage: true })
    expect(spacePauseFromStatus({ freshness: 'stale', holdsIndexStorage: true })).toBeUndefined()
    expect(spacePauseFromStatus({ freshness: null, holdsIndexStorage: false })).toBeUndefined()
  })

  it('picks the copy by which drive is full', () => {
    expect(pausedSizeTooltipKey({ holdsIndexStorage: true })).toBe('fileExplorer.dirSize.pausedForSpaceThisDrive')
    expect(pausedSizeTooltipKey({ holdsIndexStorage: false })).toBe('fileExplorer.dirSize.pausedForSpaceOtherDrive')
  })

  it('follows the freshness events: paused on, anything else off', async () => {
    answer('paused', false)
    emit('smb-nas', 'paused')
    await settle()
    expect(getSpacePause('smb-nas')).toEqual({ holdsIndexStorage: false })

    emit('smb-nas', 'stale')
    expect(getSpacePause('smb-nas')).toBeUndefined()
  })

  it('asks the backend once for a volume it never heard about, so a reopened window catches up', async () => {
    answer('paused', true)
    expect(getSpacePause('root')).toBeUndefined()
    await settle()
    expect(getSpacePause('root')).toEqual({ holdsIndexStorage: true })
    getSpacePause('root')
    await settle()
    expect(statusMock).toHaveBeenCalledTimes(1)
  })

  it('stops a listener that finished registering after the window tore it down', async () => {
    destroySpacePause()
    stops.length = 0
    const starting = initSpacePause()
    destroySpacePause()
    await starting
    expect(stops).toHaveLength(1)
    expect(stops[0]).toHaveBeenCalledTimes(1)
  })
})
