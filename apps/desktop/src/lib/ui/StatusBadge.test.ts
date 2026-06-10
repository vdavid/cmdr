import { describe, expect, it } from 'vitest'
import { mount, unmount } from 'svelte'
import StatusBadge from './StatusBadge.svelte'

function mountBadge(status: 'alpha' | 'beta') {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(StatusBadge, { target, props: { status } })
  return { target, component }
}

describe('StatusBadge', () => {
  it('renders the alpha status text (uppercased via CSS)', async () => {
    const { target, component } = mountBadge('alpha')
    const badge = target.querySelector('.feature-status-badge')
    expect(badge?.textContent).toBe('alpha')
    await unmount(component)
  })

  it('renders the beta status text', async () => {
    const { target, component } = mountBadge('beta')
    const badge = target.querySelector('.feature-status-badge')
    expect(badge?.textContent).toBe('beta')
    await unmount(component)
  })
})
