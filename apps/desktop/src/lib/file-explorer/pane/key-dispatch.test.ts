import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { FilePaneAPI } from './types'
import { createKeyDispatch, isTypingInInput, type KeyDispatchDeps } from './key-dispatch'
import { focusActivePane } from './active-pane-focus'

function makePaneRef(overrides: Record<string, unknown> = {}) {
  return {
    isLoading: vi.fn(() => false),
    handleCancelLoading: vi.fn(),
    isHeaderMenuOpen: vi.fn(() => false),
    isRenaming: vi.fn(() => false),
    isJumpActive: vi.fn(() => false),
    handleJumpKeystroke: vi.fn(),
    clearJumpState: vi.fn(),
    isQuickFilterMode: vi.fn(() => false),
    isQuickFilterActive: vi.fn(() => false),
    appendQuickFilter: vi.fn(),
    backspaceQuickFilter: vi.fn(),
    clearQuickFilter: vi.fn(),
    isFilterTypingLive: vi.fn(() => false),
    endFilterTyping: vi.fn(),
    noteFilterKeyRanCommand: vi.fn(),
    handleKeyDown: vi.fn(),
    handleKeyUp: vi.fn(),
    focusList: vi.fn(() => false),
    ...overrides,
  }
}

function setup(refs: Partial<Record<'left' | 'right', ReturnType<typeof makePaneRef>>>, container?: HTMLElement) {
  const deps: KeyDispatchDeps = {
    getPaneRef: (p) => refs[p] as unknown as FilePaneAPI | undefined,
    getFocusedPane: () => 'left',
    getContainerElement: () => container,
  }
  return createKeyDispatch(deps)
}

/**
 * A KeyboardEvent with spied preventDefault/stopPropagation. The spies are also
 * returned as plain `preventDefaultSpy`/`stopPropagationSpy` properties so
 * assertions read a function-valued spy (not the class method) and don't trip
 * `@typescript-eslint/unbound-method`.
 */
function keyEvent(key: string) {
  const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
  const preventDefaultSpy = vi.spyOn(e, 'preventDefault')
  const stopPropagationSpy = vi.spyOn(e, 'stopPropagation')
  return Object.assign(e, { preventDefaultSpy, stopPropagationSpy })
}

describe('createKeyDispatch', () => {
  beforeEach(() => vi.clearAllMocks())

  it('Escape while loading cancels the load and swallows the key (no forward)', () => {
    const left = makePaneRef({ isLoading: vi.fn(() => true) })
    const kd = setup({ left })
    const e = keyEvent('Escape')

    kd.handleKeyDown(e)

    expect(left.handleCancelLoading).toHaveBeenCalled()
    expect(e.preventDefaultSpy).toHaveBeenCalled()
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  // The switcher's own keys never reach here (its `Menu` catches them on a document capture
  // listener). What this covers is everything the menu deliberately let through — the inline
  // favorite-rename input's keystrokes — which must still not move the pane cursor behind it.
  it('an open volume chooser swallows the key from the pane behind (panes stay inert)', () => {
    const left = makePaneRef({ isHeaderMenuOpen: vi.fn(() => true) })
    const kd = setup({ left })

    kd.handleKeyDown(keyEvent('ArrowDown'))

    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('swallows for a chooser open on the OTHER pane too (⌥F2 opens one over an unfocused pane)', () => {
    const left = makePaneRef()
    const right = makePaneRef({ isHeaderMenuOpen: vi.fn(() => true) })
    const kd = setup({ left, right })

    kd.handleKeyDown(keyEvent('ArrowDown'))

    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('a printable letter is captured into the type-to-jump buffer and not forwarded', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('a')

    kd.handleKeyDown(e)

    expect(left.handleJumpKeystroke).toHaveBeenCalledWith('a')
    expect(e.preventDefaultSpy).toHaveBeenCalled()
    expect(e.stopPropagationSpy).toHaveBeenCalled()
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('once a jump is active, a printable non-letter (like "-") extends the buffer', () => {
    const left = makePaneRef({ isJumpActive: vi.fn(() => true) })
    const kd = setup({ left })

    kd.handleKeyDown(keyEvent('-'))

    expect(left.handleJumpKeystroke).toHaveBeenCalledWith('-')
    expect(left.handleKeyDown).not.toHaveBeenCalled()
  })

  it('a reset key clears the jump buffer and still forwards to the pane', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('ArrowDown')

    kd.handleKeyDown(e)

    expect(left.clearJumpState).toHaveBeenCalled()
    expect(left.handleKeyDown).toHaveBeenCalledWith(e)
  })

  it('does not intercept jump keys while renaming (forwards instead)', () => {
    const left = makePaneRef({ isRenaming: vi.fn(() => true) })
    const kd = setup({ left })
    const e = keyEvent('a')

    kd.handleKeyDown(e)

    expect(left.handleJumpKeystroke).not.toHaveBeenCalled()
    expect(left.handleKeyDown).toHaveBeenCalledWith(e)
  })

  it('handleKeyUp forwards to the focused pane', () => {
    const left = makePaneRef()
    const kd = setup({ left })
    const e = keyEvent('Shift')

    kd.handleKeyUp(e)

    expect(left.handleKeyUp).toHaveBeenCalledWith(e)
  })

  describe('handleFocusGuard', () => {
    it('refocuses the container when focus escapes to a non-exempt element', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const button = document.createElement('button')

      kd.handleFocusGuard({ target: button } as unknown as FocusEvent)

      expect(focusSpy).toHaveBeenCalled()
    })

    it('leaves focus alone on input elements', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const input = document.createElement('input')

      kd.handleFocusGuard({ target: input } as unknown as FocusEvent)

      expect(focusSpy).not.toHaveBeenCalled()
    })

    it('leaves focus alone inside dialog content (the trapFocus ping-pong guard)', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const kd = setup({}, container)
      const dialog = document.createElement('div')
      dialog.setAttribute('role', 'dialog')
      const inner = document.createElement('button')
      dialog.appendChild(inner)

      kd.handleFocusGuard({ target: inner } as unknown as FocusEvent)

      expect(focusSpy).not.toHaveBeenCalled()
    })

    it('leaves focus on the container itself when the focused pane shows no list', () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const left = makePaneRef()
      const kd = setup({ left }, container)

      kd.handleFocusGuard({ target: container } as unknown as FocusEvent)

      expect(left.focusList).toHaveBeenCalled()
      expect(focusSpy).not.toHaveBeenCalled()
    })

    // Every container focuser (E2E helpers, a selector somewhere) converges on the list.
    it("moves focus that lands on the container to the focused pane's list", () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const left = makePaneRef({ focusList: vi.fn(() => true) })
      const right = makePaneRef({ focusList: vi.fn(() => true) })
      const kd = setup({ left, right }, container)

      kd.handleFocusGuard({ target: container } as unknown as FocusEvent)

      expect(left.focusList).toHaveBeenCalledTimes(1)
      expect(right.focusList).not.toHaveBeenCalled()
      expect(focusSpy).not.toHaveBeenCalled()
    })

    it("bounces focus that escapes to a button onto the focused pane's list, not the container", () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const left = makePaneRef({ focusList: vi.fn(() => true) })
      const kd = setup({ left }, container)

      kd.handleFocusGuard({ target: document.createElement('button') } as unknown as FocusEvent)

      expect(left.focusList).toHaveBeenCalledTimes(1)
      expect(focusSpy).not.toHaveBeenCalled()
    })
  })

  describe('focusActivePane', () => {
    it("focuses the focused pane's list, and the container when it shows none", () => {
      const container = document.createElement('div')
      const focusSpy = vi.spyOn(container, 'focus')
      const left = makePaneRef({ focusList: vi.fn(() => true) })
      const deps = {
        getPaneRef: (p: 'left' | 'right') => ({ left })[p as 'left'] as unknown as FilePaneAPI,
        getFocusedPane: () => 'left' as const,
        getContainerElement: () => container,
      }

      focusActivePane(deps)
      expect(left.focusList).toHaveBeenCalledTimes(1)
      expect(focusSpy).not.toHaveBeenCalled()

      left.focusList.mockReturnValue(false)
      focusActivePane(deps)
      expect(focusSpy).toHaveBeenCalledTimes(1)
    })
  })

  /**
   * Focus and key routing with real focus moves and real dispatches against a DOM shaped
   * like the explorer's. Started as characterization pins for the screen-reader focus work
   * (#437); the pane chrome cases changed on purpose when chrome became reachable from the
   * keyboard: the guard leaves focus on a chrome control, and keys from one stay there.
   */
  describe('focus and key routing in an explorer-shaped DOM', () => {
    /**
     * The container with two panes, each with chrome (a tab, a path segment, a sort
     * header) and a listbox of two rows, wired the way `DualPaneExplorer` binds them.
     */
    function mountExplorer(focusedPane: () => 'left' | 'right' = () => 'left') {
      const container = document.createElement('div')
      container.className = 'dual-pane-explorer'
      container.tabIndex = 0
      container.innerHTML = ['left', 'right']
        .map(
          (side) => `
          <div class="file-pane" data-side="${side}" data-pane-chrome-scope>
            <div data-pane-chrome><div role="tablist"><button class="tab" role="tab" aria-selected="true">Tab</button></div></div>
            <div class="header" role="toolbar" data-pane-chrome>
              <button type="button" class="volume-name">Macintosh HD</button>
              <button type="button" class="path-segment">Users</button>
            </div>
            <div class="header-row" data-pane-chrome><button type="button" class="sortable-header">Name</button></div>
            <div class="brief-list" role="listbox" tabindex="-1" aria-label="File list">
              <div class="file-entry" role="option" aria-selected="true">a.txt</div>
              <div class="file-entry" role="option" aria-selected="false">b.txt</div>
            </div>
          </div>`,
        )
        .join('')
      document.body.appendChild(container)
      const left = makePaneRef()
      const right = makePaneRef()
      const kd = createKeyDispatch({
        getPaneRef: (p) => ({ left, right })[p] as unknown as FilePaneAPI,
        getFocusedPane: focusedPane,
        getContainerElement: () => container,
      })
      container.addEventListener('focusin', kd.handleFocusGuard)
      container.addEventListener('keydown', kd.handleKeyDown)
      const q = (side: 'left' | 'right', selector: string): HTMLElement => {
        const el = container.querySelector<HTMLElement>(`[data-side="${side}"] ${selector}`)
        if (!el) throw new Error(`no ${selector} in the ${side} pane`)
        return el
      }
      return { container, left, right, q }
    }

    afterEach(() => {
      document.body.innerHTML = ''
    })

    // So "keyboard focus follows the VoiceOver cursor" can rest on a control, and the
    // "Move focus to the pane controls" command can put it there.
    it.each([
      ['a path segment button', '.path-segment'],
      ['a pane tab', '[role="tab"]'],
      ['a sort header button', '.sortable-header'],
    ])('leaves focus that lands on %s there', (_name, selector) => {
      const { left, q } = mountExplorer()
      const control = q('left', selector)
      control.focus()
      expect(document.activeElement).toBe(control)
      expect(left.focusList).not.toHaveBeenCalled()
    })

    it.each([
      ['the container', null],
      ["the focused pane's listbox", '[role="listbox"]'],
      ["the focused pane's cursor row", '.file-entry'],
    ])('forwards ↓ dispatched on %s to the focused pane only', (_name, selector) => {
      const { container, left, right, q } = mountExplorer()
      const target = selector === null ? container : q('left', selector)
      target.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
      expect(left.handleKeyDown).toHaveBeenCalledTimes(1)
      expect(right.handleKeyDown).not.toHaveBeenCalled()
    })

    it("routes by the store's focused pane, not by which pane the event came from", () => {
      const { left, right, q } = mountExplorer(() => 'right')
      q('left', '[role="listbox"]').dispatchEvent(
        new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }),
      )
      expect(right.handleKeyDown).toHaveBeenCalledTimes(1)
      expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    /** Dispatches a keydown on `target` the way a real keypress lands; returns the event. */
    function press(target: HTMLElement, key: string): KeyboardEvent {
      const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
      target.dispatchEvent(event)
      return event
    }

    it.each([
      ['↓', 'ArrowDown'],
      ['Enter', 'Enter'],
      ['Space', ' '],
      ['a letter', 'b'],
    ])('keeps %s pressed on a chrome control away from both panes', (_name, key) => {
      const { left, right, q } = mountExplorer()
      const segment = q('left', '.path-segment')
      segment.focus()
      press(segment, key)
      expect(left.handleKeyDown).not.toHaveBeenCalled()
      expect(left.handleJumpKeystroke).not.toHaveBeenCalled()
      expect(right.handleKeyDown).not.toHaveBeenCalled()
      expect(right.handleJumpKeystroke).not.toHaveBeenCalled()
    })

    // Enter and Space activate the focused button through the browser's default action.
    it.each([
      ['Enter', 'Enter'],
      ['Space', ' '],
    ])("leaves %s's default action to the chrome control", (_name, key) => {
      const { q } = mountExplorer()
      const segment = q('left', '.path-segment')
      segment.focus()
      expect(press(segment, key).defaultPrevented).toBe(false)
    })

    it('walks the location bar with ← and →, and hops to the sort header with ↓, claiming each key', () => {
      const { q } = mountExplorer()
      q('left', '.volume-name').focus()
      expect(press(q('left', '.volume-name'), 'ArrowRight').defaultPrevented).toBe(true)
      expect(document.activeElement).toBe(q('left', '.path-segment'))
      press(q('left', '.path-segment'), 'ArrowLeft')
      expect(document.activeElement).toBe(q('left', '.volume-name'))
      press(q('left', '.volume-name'), 'ArrowDown')
      expect(document.activeElement).toBe(q('left', '.sortable-header'))
    })

    it("sends focus back to the focused pane's list on Escape", () => {
      const { left, q } = mountExplorer()
      left.focusList.mockImplementation(() => {
        q('left', '[role="listbox"]').focus()
        return true
      })
      const segment = q('left', '.path-segment')
      segment.focus()
      const event = press(segment, 'Escape')
      expect(document.activeElement).toBe(q('left', '[role="listbox"]'))
      expect(event.defaultPrevented).toBe(true)
      expect(left.handleCancelLoading).not.toHaveBeenCalled()
    })

    it('still sends a letter typed on a plain button outside chrome to type-to-jump', () => {
      const { container, left } = mountExplorer()
      const stray = document.createElement('button')
      container.appendChild(stray)
      press(stray, 'b')
      expect(left.handleJumpKeystroke).toHaveBeenCalledWith('b')
    })
  })

  it('isTypingInInput detects text-entry controls', () => {
    const input = document.createElement('input')
    expect(isTypingInInput({ target: input } as unknown as KeyboardEvent)).toBe(true)
    const div = document.createElement('div')
    expect(isTypingInInput({ target: div } as unknown as KeyboardEvent)).toBe(false)
  })
})
