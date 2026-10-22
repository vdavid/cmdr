/**
 * Tier 3 a11y test for `DialogBoundary.svelte`. It adds no markup of its own (its
 * failed state renders nothing on purpose), so the one state worth checking is the
 * healthy one: its children render exactly as they would without it.
 */

import { describe, it } from 'vitest'
import { createRawSnippet, mount, tick } from 'svelte'
import DialogBoundary from './DialogBoundary.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

describe('DialogBoundary a11y', () => {
  it('renders its children with no violations of its own', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DialogBoundary, {
      target,
      props: {
        name: 'a11y',
        children: createRawSnippet(() => ({ render: () => '<p>A dialog would be here</p>' })),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
