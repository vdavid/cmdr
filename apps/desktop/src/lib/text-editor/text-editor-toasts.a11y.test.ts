import { describe, it, vi } from 'vitest'
import { mount, tick, type Component } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('./text-editor-setting', () => ({
  openSettingsToTextEditor: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

import TextEditorToastContent from './TextEditorToastContent.svelte'

/** Mounts a toast body into a detached target and runs axe over it. */
async function expectClean<P extends Record<string, unknown>>(component: Component<P>, props: P): Promise<void> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(component, { target, props })
  await tick()
  await expectNoA11yViolations(target)
}

describe('the text editor toasts', () => {
  it('the missing-app toast (no Dismiss) has no a11y violations', async () => {
    await expectClean(TextEditorToastContent, {
      toastId: 'text-editor',
      message: 'Cmdr can’t find the editor you picked, so this file opened in TextEdit.',
    })
  })

  it('the body with a Dismiss button has no a11y violations', async () => {
    await expectClean(TextEditorToastContent, {
      toastId: 'text-editor',
      message: 'Opened in TextEdit.',
      showDismiss: true,
    })
  })
})
