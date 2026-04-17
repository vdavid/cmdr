/**
 * Tier 3 a11y tests for `SettingPasswordInput.svelte`.
 *
 * Masked password input with a reveal button. Tests cover empty,
 * pre-filled (masked), and controlled-mode (external value + onchange)
 * variants.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SettingPasswordInput from './SettingPasswordInput.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let stored = ''
vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => stored),
  setSetting: vi.fn((_id: string, value: string) => {
    stored = value
  }),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

describe('SettingPasswordInput a11y', () => {
  it('empty (uncontrolled) has no a11y violations', async () => {
    stored = ''
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.openaiApiKey',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('pre-filled (masked) has no a11y violations', async () => {
    stored = 'sk-abcdef1234567890'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.openaiApiKey',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('controlled mode (external value + onchange) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.openaiApiKey',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
        value: 'sk-12345',
        onchange: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    stored = ''
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.openaiApiKey',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
        disabled: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
