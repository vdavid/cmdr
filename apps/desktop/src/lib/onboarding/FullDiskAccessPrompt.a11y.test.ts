/**
 * Tier 3 a11y tests for `FullDiskAccessPrompt.svelte`.
 *
 * First-launch modal. Tests cover the first-ask and post-revoke copy.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FullDiskAccessPrompt from './FullDiskAccessPrompt.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings-store', () => ({
  saveSettings: vi.fn(() => Promise.resolve()),
}))

describe('FullDiskAccessPrompt a11y', () => {
  it('first-ask (wasRevoked=false) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FullDiskAccessPrompt, {
      target,
      props: { onComplete: () => {}, wasRevoked: false },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('revoked (wasRevoked=true) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FullDiskAccessPrompt, {
      target,
      props: { onComplete: () => {}, wasRevoked: true },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
