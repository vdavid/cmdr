import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import SearchFooterActions from './SearchFooterActions.svelte'

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))

const isMacOSMock = vi.fn(() => true)
vi.mock('$lib/shortcuts/key-capture', () => ({
  isMacOS: () => isMacOSMock(),
}))

describe('SearchFooterActions', () => {
  beforeEach(() => {
    isMacOSMock.mockReturnValue(true)
  })

  it('renders nothing when there are zero results', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 0, disabled: false, onOpenInPane: () => {}, onOpenInFileManager: () => {} },
    })
    await tick()
    expect(target.querySelector('.footer-actions')).toBeNull()
    target.remove()
  })

  it('renders both actions when there are results', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 7, disabled: false, onOpenInPane: () => {}, onOpenInFileManager: () => {} },
    })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    expect(buttons).toHaveLength(2)
    expect(buttons.map((b) => b.getAttribute('aria-label'))).toEqual(['Open in Finder', 'Open in pane'])
    target.remove()
  })

  it('uses the macOS label "Open in Finder" when isMacOS() is true', async () => {
    isMacOSMock.mockReturnValue(true)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: false, onOpenInPane: () => {}, onOpenInFileManager: () => {} },
    })
    await tick()
    const fmBtn = target.querySelectorAll('button')[0]
    expect(fmBtn.getAttribute('aria-label')).toBe('Open in Finder')
    expect(fmBtn.textContent?.trim()).toBe('Open in Finder')
    target.remove()
  })

  it('uses "Open in file manager" on non-macOS platforms', async () => {
    isMacOSMock.mockReturnValue(false)
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: false, onOpenInPane: () => {}, onOpenInFileManager: () => {} },
    })
    await tick()
    const fmBtn = target.querySelectorAll('button')[0]
    expect(fmBtn.getAttribute('aria-label')).toBe('Open in file manager')
    expect(fmBtn.textContent?.trim()).toBe('Open in file manager')
    target.remove()
  })

  it('fires the right handlers on click', async () => {
    const onOpenInPane = vi.fn()
    const onOpenInFileManager = vi.fn()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: false, onOpenInPane, onOpenInFileManager },
    })
    await tick()
    const [fmBtn, paneBtn] = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    fmBtn.click()
    paneBtn.click()
    expect(onOpenInFileManager).toHaveBeenCalledTimes(1)
    expect(onOpenInPane).toHaveBeenCalledTimes(1)
    target.remove()
  })

  it('disables both buttons when the dialog is disabled', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: { resultCount: 1, disabled: true, onOpenInPane: () => {}, onOpenInFileManager: () => {} },
    })
    await tick()
    const buttons = Array.from(target.querySelectorAll('button')) as HTMLButtonElement[]
    for (const b of buttons) {
      expect(b.disabled).toBe(true)
    }
    target.remove()
  })
})
