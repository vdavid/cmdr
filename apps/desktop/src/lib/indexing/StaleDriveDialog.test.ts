/**
 * Tests for the one-time stale-drive dialog (D2): it fires on the first external
 * Fresh→Stale freshness event when `staleNotify` is on and the one-shot hasn't
 * fired, and the two buttons behave (Close dismisses; Never-show-again flips the
 * setting off). Local `root` and a `staleNotify: false` setting suppress it.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { IndexFreshnessChangedEvent } from '$lib/ipc/bindings'

// Capture the freshness-event callback the dialog registers so the test can fire it.
let freshnessCb: ((p: IndexFreshnessChangedEvent) => void) | undefined
vi.mock('$lib/tauri-commands/indexing', () => ({
  onIndexFreshnessChanged: (cb: (p: IndexFreshnessChangedEvent) => void) => {
    freshnessCb = cb
    return Promise.resolve(() => {})
  },
  // Only the phone has no live watch: the one fact the dialog reads from a status.
  getVolumeIndexStatusById: (volumeId: string) =>
    Promise.resolve({ status: 'ok', data: { volumeId, liveWatch: volumeId !== 'adb-pixel' } }),
}))

const settings: Record<string, unknown> = {}
const setSetting = vi.fn((id: string, value: unknown) => {
  settings[id] = value
})
vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => settings[id],
  setSetting: (id: string, value: unknown) => {
    setSetting(id, value)
  },
}))

let firstStaleShown = false
const markFirstStaleDialogShown = vi.fn(() => {
  firstStaleShown = true
})
vi.mock('./drive-index-prefs', () => ({
  hasShownFirstStaleDialog: () => firstStaleShown,
  markFirstStaleDialogShown: () => {
    markFirstStaleDialogShown()
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'smb-backups', name: 'Backups', path: 'smb://x', category: 'network', isEjectable: false },
    { id: 'adb-pixel', name: 'Pixel', path: 'adb://pixel', category: 'mobile_device', isEjectable: true },
  ],
}))

// ModalDialog notifies the backend on open/close; stub those IPC calls.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import StaleDriveDialog from './StaleDriveDialog.svelte'

async function fire(event: IndexFreshnessChangedEvent) {
  freshnessCb?.(event)
  // The dialog reads the volume's status before it opens: let that settle.
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
  flushSync()
}

function mountDialog() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(StaleDriveDialog, { target })
  flushSync()
  return target
}

beforeEach(() => {
  freshnessCb = undefined
  firstStaleShown = false
  settings['indexing.staleNotify'] = true
  setSetting.mockClear()
  markFirstStaleDialogShown.mockClear()
})

describe('StaleDriveDialog', () => {
  it('opens on the first external Fresh→Stale event and names the drive', async () => {
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'stale' })
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()
    expect(target.textContent).toContain('Backups')
    expect(target.textContent).toContain('was disconnected')
    expect(markFirstStaleDialogShown).toHaveBeenCalledTimes(1)
  })

  it("opens once for a phone, telling its owner that the phone's own changes show up after a rescan", async () => {
    const target = mountDialog()
    await fire({ volumeId: 'adb-pixel', freshness: 'stale' })
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()
    expect(target.textContent).toContain("This phone's index may be out of date")
    expect(target.textContent).toContain('Pixel')
    expect(target.textContent).toContain('show up in folder sizes and search after a rescan')
    expect(target.textContent).not.toContain('disconnected')
    expect(markFirstStaleDialogShown).toHaveBeenCalledTimes(1)
  })

  it('does not open for the local root volume', async () => {
    const target = mountDialog()
    await fire({ volumeId: 'root', freshness: 'stale' })
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('does not open for a non-stale transition', async () => {
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'fresh' })
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('does not open when staleNotify is off', async () => {
    settings['indexing.staleNotify'] = false
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'stale' })
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('does not open a second time (one-shot)', async () => {
    firstStaleShown = true
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'stale' })
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('"Never show again" turns the setting off and closes', async () => {
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'stale' })
    const btn = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Never show again')
    btn?.click()
    flushSync()
    expect(setSetting).toHaveBeenCalledWith('indexing.staleNotify', false)
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })

  it('"Close" dismisses without changing the setting', async () => {
    const target = mountDialog()
    await fire({ volumeId: 'smb-backups', freshness: 'stale' })
    const btn = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Close')
    btn?.click()
    flushSync()
    expect(setSetting).not.toHaveBeenCalled()
    expect(target.querySelector('[role="dialog"]')).toBeNull()
  })
})
