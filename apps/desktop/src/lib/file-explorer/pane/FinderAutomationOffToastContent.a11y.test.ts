import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({ openAutomationSettings: vi.fn(() => Promise.resolve()) }))

import FinderAutomationOffToastContent from './FinderAutomationOffToastContent.svelte'

describe('FinderAutomationOffToastContent', () => {
  it('the Get info permission toast has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FinderAutomationOffToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
