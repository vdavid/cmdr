/**
 * Tests for Get info's "Cmdr can't control Finder" toast body: it says what to
 * switch, and its button opens the Automation pane of System Settings.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import { tString } from '$lib/intl/messages.svelte'

const { openAutomationSettings } = vi.hoisted(() => ({
  openAutomationSettings: vi.fn<() => Promise<void>>(() => Promise.resolve()),
}))
vi.mock('$lib/tauri-commands', () => ({ openAutomationSettings }))

import FinderAutomationOffToastContent from './FinderAutomationOffToastContent.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FinderAutomationOffToastContent, { target, props: {} })
  flushSync()
  const button = target.querySelector('button')
  if (!button) throw new Error('the toast has no button to press')
  return { target, button }
}

beforeEach(() => {
  openAutomationSettings.mockClear()
})

describe('FinderAutomationOffToastContent', () => {
  it('says what to switch back on', () => {
    const { target } = render()

    expect(target.textContent).toContain(tString('commands.handler.getInfo.automationOff'))
  })

  it('opens the Automation pane of System Settings', () => {
    const { button } = render()

    expect(button.textContent).toContain(tString('commands.handler.getInfo.openAutomationSettings'))
    button.click()
    expect(openAutomationSettings).toHaveBeenCalledOnce()
  })
})
