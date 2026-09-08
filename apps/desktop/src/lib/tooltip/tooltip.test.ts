import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { tooltip } from './tooltip'

describe('tooltip', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    vi.useFakeTimers()
    // The action's module state is app-wide and survives between tests, so clear any hover
    // suppression a previous test's keypress left behind (a real pointer move does the same).
    document.dispatchEvent(new MouseEvent('mousemove'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  function makeTrigger(): HTMLElement {
    const el = document.createElement('div')
    el.textContent = 'row'
    document.body.appendChild(el)
    // happy-dom does no layout, so feed a real rect for the positioning math.
    vi.spyOn(el, 'getBoundingClientRect').mockReturnValue({
      left: 100,
      top: 100,
      right: 150,
      bottom: 120,
      width: 50,
      height: 20,
      x: 100,
      y: 100,
      toJSON: () => ({}),
    })
    return el
  }

  /** A caller-owned hidden host holding the live rich content the `contentEl` param adopts. */
  function makeContentHost(text: string): { host: HTMLElement; content: HTMLElement } {
    const host = document.createElement('div')
    const content = document.createElement('div')
    content.textContent = text
    host.appendChild(content)
    document.body.appendChild(host)
    return { host, content }
  }

  // Regression: a virtual-scroll row recycled while hovered used to leak its 400ms show-timer (removing
  // a node fires no `mouseleave`), which then fired against the detached node — `getBoundingClientRect`
  // returns all-zero for a detached element, so the tooltip landed in the top-left corner of the window.
  it('does not show when the trigger is destroyed during the show delay', () => {
    const el = makeTrigger()
    const action = tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))

    // Svelte tearing down a recycled row: it removes the node and calls the action's destroy().
    el.remove()
    action.destroy?.()

    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })

  // Defense in depth: even if the timer fires against a detached node (no destroy ran — e.g. the trigger
  // was removed as part of a larger subtree), the tooltip must never become visible in the corner.
  it('does not show when the timer fires against a detached node', () => {
    const el = makeTrigger()
    tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))

    el.remove()

    vi.advanceTimersByTime(500)

    expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
  })

  // Sanity: a still-connected trigger shows and positions the tooltip normally (not dumped at the corner).
  it('shows and positions the tooltip for a connected trigger', () => {
    const el = makeTrigger()
    tooltip(el, 'Folder info')
    el.dispatchEvent(new MouseEvent('mouseenter'))
    vi.advanceTimersByTime(500)

    const tip = document.querySelector('.cmdr-tooltip')
    expect(tip).not.toBeNull()
    expect(tip?.classList.contains('visible')).toBe(true)
    expect(tip?.textContent).toBe('Folder info')
    expect((tip as HTMLElement).style.top).not.toBe('0px')
  })

  // Typing or moving the pane cursor must clear the tooltip the way OS-native ones do: a tooltip
  // hovering over the file list obstructs the very rows you're arrowing through.
  describe('hides on keypress', () => {
    it('hides a visible tooltip when a key is pressed', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)
      expect(document.querySelector('.cmdr-tooltip.visible')).not.toBeNull()

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }))

      expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
    })

    it('cancels a pending show timer, so a keypress during the delay shows nothing', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(100)

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }))
      vi.advanceTimersByTime(500)

      expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
    })

    // A bare modifier isn't "typing": holding ⌥ to read the favorites tooltip's own "⌥↑ / ⌥↓ to
    // reorder" hint must not wipe that hint off the screen. Matches macOS.
    it('keeps the tooltip up for a bare modifier keydown', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      for (const key of ['Shift', 'Alt', 'Control', 'Meta']) {
        document.dispatchEvent(new KeyboardEvent('keydown', { key }))
        expect(document.querySelector('.cmdr-tooltip.visible')).not.toBeNull()
      }
    })

    // The pane case that plain hiding misses: arrowing past the last visible row scrolls the list, so
    // a DIFFERENT row slides under the motionless pointer and fires `mouseenter`. Without suppression
    // the tooltip pops back 400ms later, over a file the user never pointed at.
    it('stays hidden when a row slides under the motionless pointer', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }))

      // The scrolled-in row's own trigger enters under the stationary mouse.
      const scrolledInRow = makeTrigger()
      tooltip(scrolledInRow, 'Another folder')
      scrolledInRow.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      expect(document.querySelector('.cmdr-tooltip.visible')).toBeNull()
    })

    it('shows again on hover once the pointer actually moves', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }))

      document.dispatchEvent(new MouseEvent('mousemove'))
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      expect(document.querySelector('.cmdr-tooltip.visible')).not.toBeNull()
    })

    // Suppression covers the HOVER path only. Tab fires keydown before the `focus` it causes, so
    // gating focus too would silently kill keyboard-focus tooltips.
    it('still shows on keyboard focus right after the keypress', () => {
      const el = makeTrigger()
      tooltip(el, 'Folder info')

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab' }))
      el.dispatchEvent(new FocusEvent('focus'))
      vi.advanceTimersByTime(500)

      expect(document.querySelector('.cmdr-tooltip.visible')).not.toBeNull()
    })
  })

  describe('contentEl (rich live content)', () => {
    it('adopts the host element into the tooltip on show', () => {
      const el = makeTrigger()
      const { content } = makeContentHost('Scanning...')
      tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip).not.toBeNull()
      expect(content.parentElement).toBe(tip)
      expect(tip?.textContent).toBe('Scanning...')
    })

    it('returns the element to its host on hide', () => {
      const el = makeTrigger()
      const { host, content } = makeContentHost('Scanning...')
      tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      el.dispatchEvent(new MouseEvent('mouseleave'))

      expect(content.parentElement).toBe(host)
    })

    it('returns the element to its host on destroy', () => {
      const el = makeTrigger()
      const { host, content } = makeContentHost('Scanning...')
      const action = tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      action.destroy?.()

      expect(content.parentElement).toBe(host)
    })

    it('reflects live mutations of the adopted element without a re-show', () => {
      const el = makeTrigger()
      const { content } = makeContentHost('Scanning... 0 entries')
      tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      // The caller (Svelte) mutates the live element in place while the tooltip is shown.
      content.textContent = 'Scanning... 42,000 entries'

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip?.textContent).toBe('Scanning... 42,000 entries')
      expect(content.parentElement).toBe(tip)
    })

    // The singleton-steal case: the tooltip element is shared app-wide. When trigger B's plain tooltip
    // shows while trigger A's rich content is adopted, A's element must go back to A's host undamaged,
    // not get orphaned by B's `textContent` write.
    it('returns A’s content to its host when B’s tooltip steals the shared element', () => {
      const triggerA = makeTrigger()
      const { host: hostA, content: contentA } = makeContentHost('A rich content')
      tooltip(triggerA, { contentEl: contentA })
      triggerA.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)
      expect(contentA.parentElement).toBe(document.querySelector('.cmdr-tooltip'))

      // A hides, B shows plain text into the same shared tooltip element.
      triggerA.dispatchEvent(new MouseEvent('mouseleave'))
      const triggerB = makeTrigger()
      tooltip(triggerB, 'B plain text')
      triggerB.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip?.textContent).toBe('B plain text')
      // A's content is back in A's host, intact (not detached, not damaged).
      expect(contentA.parentElement).toBe(hostA)
      expect(contentA.textContent).toBe('A rich content')
    })

    it('swaps content back to the host when update() changes the param while visible', () => {
      const el = makeTrigger()
      const { host, content } = makeContentHost('Scanning...')
      const action = tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)
      expect(content.parentElement).toBe(document.querySelector('.cmdr-tooltip'))

      // The caller swaps to a plain-text param while the tooltip is still shown.
      action.update?.('Now plain')

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip?.textContent).toBe('Now plain')
      expect(content.parentElement).toBe(host)
    })

    it('detaches the element when its host unmounted mid-show', () => {
      const el = makeTrigger()
      const { host, content } = makeContentHost('Scanning...')
      tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      // The owning component unmounts while the tooltip is showing (host leaves the DOM).
      host.remove()
      el.dispatchEvent(new MouseEvent('mouseleave'))

      // Nothing to return to, so the element is simply detached, never left dangling in the tooltip.
      expect(content.parentElement).toBeNull()
      expect(document.querySelector('.cmdr-tooltip')?.contains(content)).toBe(false)
    })

    // Regression: the documented pattern wraps the live content in a `<div hidden>` host and passes the
    // INNER content element as `contentEl`. Passing the hidden host itself would render an empty tooltip,
    // because an adopted element carries its own `hidden` attribute into the tooltip. This pins the
    // host-wraps-content shape: the adopted child is visible inside the tooltip.
    it('renders adopted content visibly when its host is hidden', () => {
      const el = makeTrigger()
      const { host, content } = makeContentHost('Scanning... 42,000 entries')
      host.hidden = true
      tooltip(el, { contentEl: content })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip).not.toBeNull()
      expect(tip?.textContent).toContain('Scanning... 42,000 entries')
      // The adopted element carries no `hidden` attribute and isn't inside a hidden subtree, so it shows.
      expect(content.hidden).toBe(false)
      expect(content.closest('[hidden]')).toBeNull()
      expect(tip?.contains(content)).toBe(true)
    })

    // The anti-pattern, pinned so the "pass the content, not the host" rule has teeth: the action adopts
    // whatever element it's given verbatim and never strips `hidden`, so adopting the hidden host itself
    // lands a `hidden` element in the tooltip — it renders invisible even though it's structurally present.
    it('keeps a hidden adopted element invisible (why callers pass the content, not the host)', () => {
      const el = makeTrigger()
      const host = document.createElement('div')
      host.hidden = true
      host.textContent = 'Scanning... 42,000 entries'
      document.body.appendChild(host)

      tooltip(el, { contentEl: host })
      el.dispatchEvent(new MouseEvent('mouseenter'))
      vi.advanceTimersByTime(500)

      const tip = document.querySelector('.cmdr-tooltip.visible')
      expect(tip?.contains(host)).toBe(true)
      // Structurally present but still `hidden`, hence the rule to adopt the content child instead.
      expect(host.hidden).toBe(true)
    })
  })
})
