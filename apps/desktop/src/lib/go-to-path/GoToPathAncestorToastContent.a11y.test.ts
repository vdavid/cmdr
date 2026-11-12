import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'

describe('GoToPathAncestorToastContent a11y', () => {
  it('renders with no a11y violations (default binding)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GoToPathAncestorToastContent, {
      target,
      props: { requested: '/tmp/nope/a.txt', landed: '/tmp' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders with no a11y violations (another path)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(GoToPathAncestorToastContent, {
      target,
      props: { requested: '/x/y', landed: '/' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
