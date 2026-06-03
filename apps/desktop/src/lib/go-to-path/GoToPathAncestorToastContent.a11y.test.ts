import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'

describe('GoToPathAncestorToastContent a11y', () => {
  it('renders with no a11y violations (with back-shortcut hint)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GoToPathAncestorToastContent, {
      target,
      props: { requested: '/tmp/nope/a.txt', landed: '/tmp', backShortcut: '⌘[' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders with no a11y violations (no back-shortcut hint)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GoToPathAncestorToastContent, {
      target,
      props: { requested: '/x/y', landed: '/', backShortcut: '' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
