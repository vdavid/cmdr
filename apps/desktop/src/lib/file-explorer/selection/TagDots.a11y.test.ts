/**
 * Tier 3 a11y tests for `TagDots.svelte`.
 *
 * The cluster is decorative pixels (colored dots), so it must expose the tag
 * names through an accessible label / title; the individual dots stay
 * `aria-hidden`. Pure, no store/icon-cache stubs needed.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import type { TagRef } from '$lib/ipc/bindings'
import TagDots from './TagDots.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const tag = (name: string, color: number): TagRef => ({ name, color })

function render(tags: TagRef[] | undefined): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TagDots, { target, props: { tags } })
  return target
}

describe('TagDots a11y', () => {
  it('exposes the tag names as an accessible label on the cluster', async () => {
    const target = render([tag('Urgent', 6), tag('Review', 2)])
    await tick()
    const cluster = target.querySelector('[role="img"]')
    expect(cluster).not.toBeNull()
    expect(cluster?.getAttribute('aria-label')).toBe('Urgent, Review')
    expect(cluster?.getAttribute('title')).toBe('Urgent, Review')
    await expectNoA11yViolations(target)
  })

  it('includes colourless tag names in the label even with no dot', async () => {
    const target = render([tag('Filed', 0), tag('Hot', 6)])
    await tick()
    const cluster = target.querySelector('[role="img"]')
    expect(cluster?.getAttribute('aria-label')).toBe('Filed, Hot')
    await expectNoA11yViolations(target)
  })

  it('renders nothing when there are no colored tags', async () => {
    const target = render([tag('OnlyColourless', 0)])
    await tick()
    expect(target.querySelector('[role="img"]')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('overflow chip is hidden from assistive tech (label carries the names)', async () => {
    const target = render([tag('a', 1), tag('b', 2), tag('c', 3), tag('d', 4), tag('e', 5)])
    await tick()
    const chip = target.querySelector('.tag-chip')
    expect(chip?.getAttribute('aria-hidden')).toBe('true')
    expect(chip?.textContent).toBe('+3')
    await expectNoA11yViolations(target)
  })
})
