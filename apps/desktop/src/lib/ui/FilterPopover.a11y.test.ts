/**
 * Tier-3 a11y tests for `FilterPopover.svelte`.
 *
 * `FilterPopover` composes `Popover` (positioning, focus trap, Esc-scoped close) with a
 * section header above the filter controls. Two header shapes need covering: a plain `<span>`
 * heading above a radio grid (the default), and a real `<label for=…>` association when the
 * header labels a single control (`labelFor`). Closed state renders nothing.
 *
 * The anchor is a real button in the test DOM so the popover has something to position against.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import FilterPopover from './FilterPopover.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeAnchor(target: HTMLElement): HTMLButtonElement {
  const anchor = document.createElement('button')
  anchor.textContent = 'Size'
  target.appendChild(anchor)
  return anchor
}

describe('FilterPopover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: false,
        onClose: () => {},
        label: 'Size',
        ariaLabel: 'Size filter',
        children: createRawSnippet(() => ({
          render: () => '<input type="radio" name="size-op" aria-label="Any size" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open with a span heading above a radio grid has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        label: 'Modified',
        ariaLabel: 'Modified filter',
        sectionClass: 'size-grid-section',
        children: createRawSnippet(() => ({
          render: () =>
            '<label><input type="radio" name="mod-op" aria-label="Any time" />Any</label>' +
            '<label><input type="radio" name="mod-op" aria-label="After" />After</label>',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })

  it('open with a labelFor association on a single control has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        label: 'Search in',
        ariaLabel: 'Search in filter',
        labelFor: 'scope-textarea',
        sectionClass: 'scope-popover',
        children: createRawSnippet(() => ({
          render: () => '<textarea id="scope-textarea"></textarea>',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})
