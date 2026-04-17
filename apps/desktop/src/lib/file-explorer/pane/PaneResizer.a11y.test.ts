/**
 * Tier 3 a11y tests for `PaneResizer.svelte`.
 *
 * Thin drag handle between panes with `role="separator"` and
 * `aria-orientation="vertical"`.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import PaneResizer from './PaneResizer.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('PaneResizer a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PaneResizer, {
      target,
      props: {
        onResize: () => {},
        onResizeEnd: () => {},
        onReset: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
