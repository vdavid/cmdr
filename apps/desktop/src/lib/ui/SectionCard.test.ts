import { describe, it, expect } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import SectionCard from './SectionCard.svelte'

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span data-testid="slot">${text}</span>`,
  }))
}

describe('SectionCard', () => {
  it('renders slot content inside the card', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { children: textSnippet('hello') },
    })
    await tick()
    const slot = target.querySelector('[data-testid="slot"]')
    expect(slot?.textContent).toBe('hello')
  })

  it('renders no label when label prop is omitted', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { children: textSnippet('body') },
    })
    await tick()
    expect(target.querySelector('h3')).toBeNull()
  })

  it('renders an h3 with the given label', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { label: 'Theme', children: textSnippet('body') },
    })
    await tick()
    const heading = target.querySelector('h3')
    expect(heading?.textContent).toBe('Theme')
  })

  it('sets the id on the outer section when provided', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { id: 'components-buttons', children: textSnippet('body') },
    })
    await tick()
    const section = target.querySelector('section')
    expect(section?.id).toBe('components-buttons')
  })

  it('emits data-gated="true" on the wrapper when gated', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { gated: true, children: textSnippet('body') },
    })
    await tick()
    const section = target.querySelector('section')
    expect(section?.getAttribute('data-gated')).toBe('true')
    // The dimming selector targets the inner `.section-card`, so the gated
    // attribute must sit on an element that still contains it.
    expect(section?.querySelector('.section-card')).not.toBeNull()
  })

  it('omits data-gated entirely when not gated', async () => {
    const target = document.createElement('div')
    mount(SectionCard, {
      target,
      props: { children: textSnippet('body') },
    })
    await tick()
    const section = target.querySelector('section')
    expect(section?.hasAttribute('data-gated')).toBe(false)
  })
})
