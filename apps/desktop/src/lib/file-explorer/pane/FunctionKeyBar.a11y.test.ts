/**
 * Tier 3 a11y tests for `FunctionKeyBar.svelte`.
 *
 * F1-F10 toolbar at the bottom of the pane. Tests cover the visible
 * and hidden states. Shift-held variant uses `<svelte:document>` so
 * we can't easily toggle it in jsdom — auditing the default variant
 * is sufficient for structural a11y.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('FunctionKeyBar a11y', () => {
  it('visible (default keys) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, {
      target,
      props: {
        visible: true,
        onRename: () => {},
        onView: () => {},
        onEdit: () => {},
        onCopy: () => {},
        onMove: () => {},
        onNewFile: () => {},
        onNewFolder: () => {},
        onDelete: () => {},
        onDeletePermanently: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('hidden (visible=false) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, {
      target,
      props: { visible: false },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
