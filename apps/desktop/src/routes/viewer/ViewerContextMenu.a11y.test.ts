import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'

import ViewerContextMenu from './ViewerContextMenu.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ViewerContextMenu a11y', () => {
  it('default state (selection present) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerContextMenu, {
      target,
      props: {
        x: 50,
        y: 50,
        hasSelection: true,
        onCopy: () => {},
        onSelectAll: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('no-selection state (Copy disabled) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerContextMenu, {
      target,
      props: {
        x: 50,
        y: 50,
        hasSelection: false,
        onCopy: () => {},
        onSelectAll: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
