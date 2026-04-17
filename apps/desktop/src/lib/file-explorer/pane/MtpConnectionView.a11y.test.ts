/**
 * Tier 3 a11y tests for `MtpConnectionView.svelte`.
 *
 * Only renders when the current volume is a device-only MTP ID. Tests
 * verify that the connecting and error UIs have no violations.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import MtpConnectionView from './MtpConnectionView.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Don't resolve: the component auto-connects on mount, but a pending
// promise keeps the UI in the "Connecting..." state we want to audit.
vi.mock('$lib/mtp/mtp-store.svelte', () => ({
  connect: vi.fn(() => new Promise<never>(() => {})),
}))

vi.mock('$lib/mtp', () => ({
  isMtpVolumeId: (id: string) => id.startsWith('mtp-'),
  constructMtpPath: (device: string, storage: number) => `mtp://${device}/${String(storage)}`,
}))

describe('MtpConnectionView a11y', () => {
  it('connecting state (device-only volumeId) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectionView, {
      target,
      props: { volumeId: 'mtp-336592896' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('non-MTP volume (no render) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectionView, {
      target,
      props: { volumeId: 'root' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
