import { describe, it, expect, afterEach } from 'vitest'
import {
  focusPaneChrome,
  isPaneChromeFocused,
  isPaneChromeTarget,
  movePaneChromeFocus,
  paneChromeOwnsKey,
} from './pane-chrome'

/**
 * One pane's chrome the way the explorer renders it: the tab bar, the location bar, and the
 * list's sort header, each a `data-pane-chrome` group inside the pane's
 * `data-pane-chrome-scope` wrapper, with the list beside them.
 */
function mountPane(): { q: (selector: string) => HTMLElement; scope: HTMLElement } {
  const scope = document.createElement('div')
  scope.setAttribute('data-pane-chrome-scope', '')
  scope.innerHTML = `
    <div class="tab-bar" data-pane-chrome>
      <div role="tablist">
        <button role="tab" class="tab-1" aria-selected="false">One</button>
        <button role="tab" class="tab-2" aria-selected="true">Two</button>
      </div>
      <button class="new-tab">New tab</button>
    </div>
    <div class="file-pane">
      <div class="header" role="toolbar" data-pane-chrome>
        <button class="volume">Macintosh HD</button>
        <button class="detach" disabled>Eject</button>
        <span class="path">
          <button class="seg-users">Users</button>
          <span class="path-sep">/</span>
          <button class="seg-test_user_3">test_user_3</button>
          <span>projects</span>
        </span>
        <span class="repo-chip" tabindex="-1" role="img" aria-label="main, clean">main</span>
      </div>
      <div class="header-row" role="toolbar" data-pane-chrome>
        <button class="sort-name">Name</button>
        <button class="sort-size">Size</button>
      </div>
      <div class="list" role="listbox" tabindex="-1"></div>
    </div>`
  document.body.appendChild(scope)
  const q = (selector: string): HTMLElement => {
    const el = scope.querySelector<HTMLElement>(selector)
    if (!el) throw new Error(`no ${selector}`)
    return el
  }
  return { q, scope }
}

/** Presses `key` on the focused element through `movePaneChromeFocus`; true when it moved focus. */
function press(key: string, init: KeyboardEventInit = {}): boolean {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  Object.defineProperty(event, 'target', { value: document.activeElement })
  return movePaneChromeFocus(event)
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('isPaneChromeTarget / isPaneChromeFocused', () => {
  it('is true inside a chrome group and false on the list or outside the pane', () => {
    const { q } = mountPane()
    expect(isPaneChromeTarget(q('.seg-users'))).toBe(true)
    expect(isPaneChromeTarget(q('.tab-1'))).toBe(true)
    expect(isPaneChromeTarget(q('.sort-size'))).toBe(true)
    expect(isPaneChromeTarget(q('.list'))).toBe(false)
    expect(isPaneChromeTarget(document.body)).toBe(false)
    expect(isPaneChromeTarget(null)).toBe(false)
  })

  it('reads focus for the keyboard', () => {
    const { q } = mountPane()
    q('.volume').focus()
    expect(isPaneChromeFocused()).toBe(true)
    q('.list').focus()
    expect(isPaneChromeFocused()).toBe(false)
  })
})

describe('paneChromeOwnsKey', () => {
  it.each(['Enter', 'Space', '↑', '↓', '←', '→', 'Home', 'End'])('keeps %s for a focused chrome control', (combo) => {
    const { q } = mountPane()
    q('.seg-users').focus()
    expect(paneChromeOwnsKey(combo)).toBe(true)
  })

  it.each(['Tab', '⇧Tab', '⌘R', '⇧↓', 'F5', 'Escape'])('leaves %s to its command', (combo) => {
    const { q } = mountPane()
    q('.seg-users').focus()
    expect(paneChromeOwnsKey(combo)).toBe(false)
  })

  it('owns nothing while focus is on the list', () => {
    const { q } = mountPane()
    q('.list').focus()
    expect(paneChromeOwnsKey('Enter')).toBe(false)
  })
})

describe('focusPaneChrome', () => {
  it("lands on the group's first control", () => {
    const { q } = mountPane()
    expect(focusPaneChrome(q('.header'))).toBe(true)
    expect(document.activeElement).toBe(q('.volume'))
  })

  it('is false for a group with nothing to focus', () => {
    const empty = document.createElement('div')
    empty.setAttribute('data-pane-chrome', '')
    document.body.appendChild(empty)
    expect(focusPaneChrome(empty)).toBe(false)
  })
})

describe('movePaneChromeFocus', () => {
  it('walks the location bar with → and ←, skipping disabled controls and plain text', () => {
    const { q } = mountPane()
    q('.volume').focus()
    expect(press('ArrowRight')).toBe(true)
    expect(document.activeElement).toBe(q('.seg-users'))
    expect(press('ArrowRight')).toBe(true)
    expect(document.activeElement).toBe(q('.seg-test_user_3'))
    expect(press('ArrowRight')).toBe(true)
    expect(document.activeElement).toBe(q('.repo-chip'))
    expect(press('ArrowLeft')).toBe(true)
    expect(document.activeElement).toBe(q('.seg-test_user_3'))
  })

  it('stops at either end without wrapping', () => {
    const { q } = mountPane()
    q('.repo-chip').focus()
    expect(press('ArrowRight')).toBe(false)
    expect(document.activeElement).toBe(q('.repo-chip'))
    q('.volume').focus()
    expect(press('ArrowLeft')).toBe(false)
    expect(document.activeElement).toBe(q('.volume'))
  })

  it('jumps to the first and last control with Home and End', () => {
    const { q } = mountPane()
    q('.seg-users').focus()
    expect(press('End')).toBe(true)
    expect(document.activeElement).toBe(q('.repo-chip'))
    expect(press('Home')).toBe(true)
    expect(document.activeElement).toBe(q('.volume'))
  })

  it('hops between the tab bar, location bar, and sort header with ↑ and ↓', () => {
    const { q } = mountPane()
    q('.seg-users').focus()
    expect(press('ArrowDown')).toBe(true)
    expect(document.activeElement).toBe(q('.sort-name'))
    expect(press('ArrowDown')).toBe(false)
    expect(document.activeElement).toBe(q('.sort-name'))
    expect(press('ArrowUp')).toBe(true)
    // Back where it left the location bar.
    expect(document.activeElement).toBe(q('.seg-users'))
    expect(press('ArrowUp')).toBe(true)
    // A tab bar it hasn't visited yet opens on the selected tab.
    expect(document.activeElement).toBe(q('.tab-2'))
    expect(press('ArrowUp')).toBe(false)
  })

  it('never hops into the other pane', () => {
    const left = mountPane()
    const right = mountPane()
    left.q('.sort-size').focus()
    expect(press('ArrowDown')).toBe(false)
    right.q('.tab-1').focus()
    expect(press('ArrowUp')).toBe(false)
    expect(document.activeElement).toBe(right.q('.tab-1'))
  })

  it('leaves a modified arrow to its command', () => {
    const { q } = mountPane()
    q('.volume').focus()
    expect(press('ArrowRight', { metaKey: true })).toBe(false)
    expect(press('ArrowDown', { shiftKey: true })).toBe(false)
    expect(document.activeElement).toBe(q('.volume'))
  })

  it('does nothing for a key from outside pane chrome', () => {
    const { q } = mountPane()
    q('.list').focus()
    expect(press('ArrowRight')).toBe(false)
    expect(document.activeElement).toBe(q('.list'))
  })
})
