import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))
vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
  getSetting: vi.fn(() => '⌃⌥⌘J'),
}))
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    setGlobalGoToLatestShortcut: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
  },
}))

import GlobalShortcutWarnToastContent from './GlobalShortcutWarnToastContent.svelte'

describe('GlobalShortcutWarnToastContent a11y', () => {
  it('renders with no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GlobalShortcutWarnToastContent, {
      target,
      props: { toastId: 'shortcut-warn', binding: '⌃⌥⌘J' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
