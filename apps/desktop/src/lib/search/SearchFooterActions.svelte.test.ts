import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchFooterActions from './SearchFooterActions.svelte'

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))

describe('SearchFooterActions', () => {
  it('renders nothing when there are zero results', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 0, disabled: false, onShowAllInMainWindow: () => {}, onGoToFile: () => {} },
    })
    await tick()
    expect(target.querySelector('.footer-actions')).toBeNull()
    target.remove()
  })

  it('renders both actions with the shortcut hints when there are results', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 7, disabled: false, onShowAllInMainWindow: () => {}, onGoToFile: () => {} },
    })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    expect(buttons).toHaveLength(2)
    expect(buttons.map((b) => b.getAttribute('aria-label'))).toEqual(['Go to file', 'Show all in main window'])
    // Discoverable shortcuts: each button surfaces its key hint inline.
    const labels = buttons.map((b) => b.textContent?.replace(/\s+/g, ' ').trim() ?? '')
    expect(labels[0]).toContain('Go to file')
    expect(labels[0]).toContain('⏎')
    expect(labels[1]).toContain('Show all in main window')
    expect(labels[1]).toContain('⌥A')
    target.remove()
  })

  it('fires the right handlers on click', async () => {
    const onShowAllInMainWindow = vi.fn()
    const onGoToFile = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: false, onShowAllInMainWindow, onGoToFile },
    })
    await tick()
    const [goBtn, mainBtn] = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    goBtn.click()
    mainBtn.click()
    expect(onGoToFile).toHaveBeenCalledTimes(1)
    expect(onShowAllInMainWindow).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('disables both buttons when the dialog is disabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: true, onShowAllInMainWindow: () => {}, onGoToFile: () => {} },
    })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    for (const b of buttons) {
      expect(b.disabled).toBe(true)
    }
    target.remove()
  })
})
