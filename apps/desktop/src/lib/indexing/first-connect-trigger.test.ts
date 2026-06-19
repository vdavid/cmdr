/**
 * Tests for the first-connect prompt gating (D6): the prompt shows only when
 * indexing is on, the per-drive prompt is on, the drive isn't silenced, isn't
 * `root`, isn't already indexed, and wasn't already prompted this session.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'

const addToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...a: unknown[]) => {
    addToast(...a)
  },
}))

const settings: Record<string, unknown> = {}
vi.mock('$lib/settings', () => ({ getSetting: (id: string) => settings[id] }))

let silenced: string[] = []
vi.mock('./drive-index-prefs', () => ({ isDriveSilenced: (id: string) => silenced.includes(id) }))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), debug: vi.fn(), info: vi.fn(), error: vi.fn() }),
}))

let statusByVolume: Record<string, VolumeIndexStatus> = {}
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getVolumeIndexStatusById: (volumeId: string) =>
      Promise.resolve({
        status: 'ok' as const,
        data: statusByVolume[volumeId] ?? {
          volumeId,
          enabled: false,
          freshness: null,
          scanCompletedAt: null,
          scanDurationMs: null,
        },
      }),
  },
}))

import { maybePromptFirstConnect } from './first-connect-trigger'

const actions = { onEnable: vi.fn(), onSilenceDrive: vi.fn(), onSilenceAll: vi.fn() }

beforeEach(() => {
  addToast.mockClear()
  silenced = []
  statusByVolume = {}
  settings['indexing.enabled'] = true
  settings['indexing.askForEachDrive'] = true
})

describe('maybePromptFirstConnect gating', () => {
  it('prompts a new external drive when all gates pass', async () => {
    await maybePromptFirstConnect('smb-a', 'Share A', actions)
    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('never prompts the local root volume', async () => {
    await maybePromptFirstConnect('root', 'Macintosh HD', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does not prompt when indexing is disabled', async () => {
    settings['indexing.enabled'] = false
    await maybePromptFirstConnect('smb-b', 'Share B', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does not prompt when "ask for each drive" is off', async () => {
    settings['indexing.askForEachDrive'] = false
    await maybePromptFirstConnect('smb-c', 'Share C', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does not prompt a silenced drive', async () => {
    silenced = ['smb-d']
    await maybePromptFirstConnect('smb-d', 'Share D', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does not prompt an already-indexed drive', async () => {
    statusByVolume['smb-e'] = {
      volumeId: 'smb-e',
      enabled: true,
      freshness: 'fresh',
      scanCompletedAt: null,
      scanDurationMs: null,
    }
    await maybePromptFirstConnect('smb-e', 'Share E', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('does not prompt the same drive twice in a session', async () => {
    await maybePromptFirstConnect('smb-f', 'Share F', actions)
    await maybePromptFirstConnect('smb-f', 'Share F', actions)
    expect(addToast).toHaveBeenCalledTimes(1)
  })
})
