/**
 * Tests for the first-connect prompt gating (D6): the prompt shows only when
 * indexing is on, the per-drive prompt is on, the drive isn't silenced, isn't
 * `root`, isn't already indexed, and wasn't already prompted this session.
 * "Already indexed" means a scan built or is building it — ❌ never merely
 * "an instance is registered", which a search's walk is enough for.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { VolumeIndexStatus } from '$lib/ipc/bindings'

const addToast = vi.fn()
const dismissToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...a: unknown[]) => {
    addToast(...a)
  },
  dismissToast: (...a: unknown[]) => {
    dismissToast(...a)
  },
  // Every offer the mock was asked for counts as on screen.
  getToasts: () => addToast.mock.calls.map((call) => ({ id: (call[1] as { id?: string }).id })),
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
          failure: null,
          scanCompletedAt: null,
          scanDurationMs: null,
          coalescedSignalsSinceSweep: 0,
          unreadableLocations: 0,
          unreadableRetried: false,
          nextSweepDueAt: null,
          liveWatch: true,
        },
      }),
  },
}))

import { isReadyForFirstConnectPrompt, maybePromptFirstConnect, withdrawGonePrompts } from './first-connect-trigger'

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
      failure: null,
      scanCompletedAt: null,
      scanDurationMs: null,
      coalescedSignalsSinceSweep: 0,
      unreadableLocations: 0,
      unreadableRetried: false,
      nextSweepDueAt: null,
      liveWatch: true,
    }
    await maybePromptFirstConnect('smb-e', 'Share E', actions)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('still prompts a drive a SEARCH walked, which is registered but not indexed', async () => {
    // A search's walk stands a writer-only instance up on a drive nothing has
    // ever scanned, so `enabled` reads true there while `freshness` stays null.
    // Treating that as "already indexed" would retire the offer for the drive
    // that most needs it — the walk covered one folder, not the drive.
    statusByVolume['smb-walked'] = {
      volumeId: 'smb-walked',
      enabled: true,
      freshness: null,
      failure: null,
      scanCompletedAt: null,
      scanDurationMs: null,
      coalescedSignalsSinceSweep: 0,
      unreadableLocations: 0,
      unreadableRetried: false,
      nextSweepDueAt: null,
      liveWatch: true,
    }
    await maybePromptFirstConnect('smb-walked', 'Walked share', actions)
    expect(addToast).toHaveBeenCalledTimes(1)
  })

  it('does not prompt the same drive twice in a session', async () => {
    await maybePromptFirstConnect('smb-f', 'Share F', actions)
    await maybePromptFirstConnect('smb-f', 'Share F', actions)
    expect(addToast).toHaveBeenCalledTimes(1)
  })
})

/**
 * ❗ "Index private on localhost:11482?" popped up while the share was still
 * connecting, behind its sign-in sheet (QA round 3): the offer waits until the
 * drive is live and the pane has landed on it.
 */
describe('isReadyForFirstConnectPrompt', () => {
  it('waits for a network share to be live', () => {
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'saved' }, 'smb-p')).toBe(false)
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'needs_sign_in' }, 'smb-p')).toBe(false)
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'disconnected' }, 'smb-p')).toBe(false)
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'direct' }, 'smb-p')).toBe(true)
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'os_mount' }, 'smb-p')).toBe(true)
  })

  it('waits for the pane to land on the drive', () => {
    expect(isReadyForFirstConnectPrompt({ id: 'smb-p', connectionState: 'direct' }, 'root')).toBe(false)
    expect(isReadyForFirstConnectPrompt({ id: 'usb-1', connectionState: null }, 'root')).toBe(false)
    expect(isReadyForFirstConnectPrompt({ id: 'usb-1', connectionState: null }, 'usb-1')).toBe(true)
  })

  // ERR-JUCNB: the offer went up for an ADB phone still waiting for its "Allow USB
  // debugging?" tap, and accepting it could only be refused.
  it("waits for a phone's volume to be registered", () => {
    const phone = 'adb-lgh815-0123456789abcdef'
    expect(isReadyForFirstConnectPrompt({ id: phone, connectionState: null }, phone)).toBe(false)
  })
})

/**
 * ❗ The offer goes when its drive does: "Index private on localhost:11481?"
 * stayed on screen after the share was ejected (QA round 7).
 */
describe('withdrawGonePrompts', () => {
  it('withdraws the offer for a drive that left or stopped answering, and keeps the others', async () => {
    await maybePromptFirstConnect('smb-gone', 'Gone share', actions)
    await maybePromptFirstConnect('smb-here', 'Here share', actions)
    const offered = addToast.mock.calls.map((call) => (call[1] as { id?: string }).id)
    dismissToast.mockClear()

    withdrawGonePrompts([{ id: 'smb-here', connectionState: 'direct' }])

    expect(dismissToast).toHaveBeenCalledWith(offered[0])
    expect(dismissToast).not.toHaveBeenCalledWith(offered[1])
  })

  it('withdraws the offer for a share that is still listed but no longer live', async () => {
    await maybePromptFirstConnect('smb-saved', 'Saved share', actions)
    const offered = (addToast.mock.calls.at(-1)?.[1] as { id?: string }).id
    dismissToast.mockClear()
    withdrawGonePrompts([{ id: 'smb-saved', connectionState: 'saved' }])
    expect(dismissToast).toHaveBeenCalledWith(offered)
  })

  /**
   * ❗ Offered once per session, answered or not: a withdrawn offer doesn't come back on
   * the share's next connect. It reappeared on every reconnect of the same share (final
   * QA), which nags about a question the person already saw.
   */
  it('does not offer again when the drive reconnects after its offer was withdrawn', async () => {
    await maybePromptFirstConnect('smb-again', 'Again share', actions)
    withdrawGonePrompts([])
    addToast.mockClear()
    await maybePromptFirstConnect('smb-again', 'Again share', actions)
    expect(addToast).not.toHaveBeenCalled()
  })
})
