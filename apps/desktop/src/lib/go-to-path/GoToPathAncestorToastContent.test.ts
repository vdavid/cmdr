import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import GoToPathAncestorToastContent from './GoToPathAncestorToastContent.svelte'

function setup(props: { requested: string; landed: string; backShortcut: string }) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GoToPathAncestorToastContent, { target, props })
  return {
    target,
    cleanup: () => {
      target.remove()
    },
  }
}

describe('GoToPathAncestorToastContent', () => {
  it('renders the requested and landed paths', async () => {
    const { target, cleanup } = setup({ requested: '/tmp/nope/a.txt', landed: '/tmp', backShortcut: '⌘[' })
    await tick()
    const text = target.textContent
    expect(text).toContain('/tmp/nope/a.txt')
    expect(text).toContain('/tmp')
    cleanup()
  })

  it('renders the snapshotted back-shortcut in a kbd', async () => {
    const { target, cleanup } = setup({ requested: '/x/y', landed: '/', backShortcut: '⌘B' })
    await tick()
    const kbd = target.querySelector('kbd')
    expect(kbd?.textContent).toBe('⌘B')
    cleanup()
  })

  it('omits the hint line when no back-shortcut is given', async () => {
    const { target, cleanup } = setup({ requested: '/x/y', landed: '/', backShortcut: '' })
    await tick()
    expect(target.querySelector('kbd')).toBeNull()
    expect(target.querySelector('.hint')).toBeNull()
    cleanup()
  })
})
