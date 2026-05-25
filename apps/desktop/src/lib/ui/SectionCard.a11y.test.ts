import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import SectionCard from './SectionCard.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function snip(text: string) {
  return createRawSnippet(() => ({ render: () => `<span>${text}</span>` }))
}

describe('SectionCard a11y', () => {
  it('unlabelled card has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SectionCard, { target, props: { children: snip('Body') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('labelled card has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SectionCard, { target, props: { label: 'Theme', children: snip('Body') } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
