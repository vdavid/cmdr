/**
 * Tier 3 a11y for the MTP pane's USB-debugging line.
 *
 * Two things are worth auditing here rather than assuming: the × has no visible
 * text, so its accessible name comes entirely from `aria-label`, and the "How"
 * link sits inline in a sentence where a bare underline would be the only cue.
 */
import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import AdbHint from './AdbHint.svelte'

const { stubs } = vi.hoisted(() => ({
  stubs: {
    volumes: [] as unknown[],
    settings: new Map<string, boolean>(),
  },
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => stubs.volumes }))
vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => stubs.settings.get(id),
  setSetting: vi.fn(),
}))
vi.mock('$lib/tauri-commands', () => ({ openExternalUrl: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

const MTP_VOLUME_ID = 'mtp-pixel-7:65537'

describe('AdbHint a11y', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    stubs.volumes = [{ id: MTP_VOLUME_ID, name: 'Pixel 7', category: 'mobile_device', fsType: 'mtp' }]
    stubs.settings = new Map([
      ['fileOperations.adbEnabled', true],
      ['behavior.adbHintDismissed', false],
    ])
  })

  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AdbHint, { target, props: { volumeId: MTP_VOLUME_ID } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
