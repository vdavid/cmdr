import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./terminal-app-setting', () => ({
  openSettingsToTerminalApp: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import OpenTerminalHintToastContent from './OpenTerminalHintToastContent.svelte'
import TerminalAppMissingToastContent from './TerminalAppMissingToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the "Open terminal here" toasts', () => {
  it('the one-time hint has no a11y violations', async () => {
    await expectClean(OpenTerminalHintToastContent, { toastId: 'open-terminal-hint' })
  })

  it('the uninstalled-app toast has no a11y violations, named', async () => {
    await expectClean(TerminalAppMissingToastContent, { toastId: 'terminal-app-missing', appName: 'Warp' })
  })

  it('the uninstalled-app toast has no a11y violations when Cmdr has no name for the app', async () => {
    await expectClean(TerminalAppMissingToastContent, { toastId: 'terminal-app-missing', appName: null })
  })
})
