/**
 * Tier 3 a11y tests for `StaleDriveDialog.svelte`: the one-time "your drive went
 * stale" dialog must have no axe violations once open. The dialog renders nothing
 * until a first external Fresh→Stale event arrives, so the mocks below let the
 * test fire that event (mirroring `StaleDriveDialog.test.ts`).
 */
import { describe, it } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { IndexFreshnessChangedEvent } from '$lib/ipc/bindings'
import { vi } from 'vitest'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Capture the freshness-event callback the dialog registers so the test can fire it.
let freshnessCb: ((p: IndexFreshnessChangedEvent) => void) | undefined
vi.mock('$lib/tauri-commands/indexing', () => ({
  onIndexFreshnessChanged: (cb: (p: IndexFreshnessChangedEvent) => void) => {
    freshnessCb = cb
    return Promise.resolve(() => {})
  },
}))

vi.mock('$lib/settings', () => ({
  getSetting: () => true,
  setSetting: () => {},
}))

vi.mock('./drive-index-prefs', () => ({
  hasShownFirstStaleDialog: () => false,
  markFirstStaleDialogShown: () => {},
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'smb-backups', name: 'Backups', path: 'smb://x', category: 'network', isEjectable: false }],
}))

// ModalDialog notifies the backend on open/close; stub those IPC calls.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import StaleDriveDialog from './StaleDriveDialog.svelte'

describe('StaleDriveDialog a11y', () => {
  it('the open dialog has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(StaleDriveDialog, { target })
    flushSync()

    freshnessCb?.({ volumeId: 'smb-backups', freshness: 'stale' })
    await tick()
    flushSync()

    await expectNoA11yViolations(target)
  })
})
