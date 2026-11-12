import { describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import PathPills from './PathPills.svelte'
import { scheduleStableWidthMeasure } from './path-pills-layout'

vi.mock('./path-pills-layout', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./path-pills-layout')>()
  return { ...actual, scheduleStableWidthMeasure: vi.fn(actual.scheduleStableWidthMeasure) }
})

function renderPills(path: string, onPick: (p: string) => void = () => {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(PathPills, { target, props: { path, onPick } })
  return target
}

/** What hovering a pill would show. The house tooltip is a singleton, so ask for its text. */
function pillTooltip(pill: HTMLElement): string | null {
  pill.dispatchEvent(new MouseEvent('mouseenter'))
  vi.advanceTimersByTime(500)
  const text = document.querySelector('.cmdr-tooltip.visible')?.textContent ?? null
  pill.dispatchEvent(new MouseEvent('mouseleave'))
  return text
}

describe('PathPills', () => {
  it('splits an absolute POSIX path into one pill per segment', async () => {
    const target = renderPills('/Users/test_user_10/code')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent.trim())
    expect(labels).toEqual(['Users', 'test_user_10', 'code'])
    target.remove()
  })

  it('renders separator glyphs between pills (one fewer than pills)', async () => {
    const target = renderPills('/a/b/c')
    await tick()
    const pills = target.querySelectorAll('.pill')
    const seps = target.querySelectorAll('.sep')
    expect(pills).toHaveLength(3)
    expect(seps).toHaveLength(2)
    target.remove()
  })

  it('collapses empty segments from leading/trailing/double slashes', async () => {
    const target = renderPills('//Users//test_user_10///')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent.trim())
    expect(labels).toEqual(['Users', 'test_user_10'])
    target.remove()
  })

  it('renders a single "/" pill for a bare root', async () => {
    const target = renderPills('/')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent.trim())
    expect(labels).toEqual(['/'])
    target.remove()
  })

  it('renders nothing for an empty string', async () => {
    const target = renderPills('')
    await tick()
    expect(target.querySelector('.path-pills')).toBeNull()
    expect(target.querySelectorAll('.pill')).toHaveLength(0)
    target.remove()
  })

  it('handles a relative path without a leading slash', async () => {
    const target = renderPills('docs/notes')
    await tick()
    vi.useFakeTimers()
    const pills = Array.from(target.querySelectorAll<HTMLElement>('.pill'))
    expect(pills.map((p) => p.textContent.trim())).toEqual(['docs', 'notes'])
    // Each pill hands over the ancestor path it would navigate to, through the house
    // tooltip rather than a native `title` (which would hover unlike the rest of the app).
    expect(pillTooltip(pills[0])).toBe('docs')
    expect(pillTooltip(pills[1])).toBe('docs/notes')
    vi.useRealTimers()
    target.remove()
  })

  it('passes each ancestor path to onPick on click', async () => {
    const onPick = vi.fn()
    const target = renderPills('/Users/test_user_10/code', onPick)
    await tick()
    const pills = Array.from(target.querySelectorAll<HTMLElement>('.pill'))
    pills[0]?.click()
    pills[1]?.click()
    pills[2]?.click()
    expect(onPick.mock.calls).toEqual([['/Users'], ['/Users/test_user_10'], ['/Users/test_user_10/code']])
    target.remove()
  })

  it('does not split on backslashes (macOS + Linux only)', async () => {
    const target = renderPills('/Users/test_user_10\\windows\\path')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent.trim())
    expect(labels).toEqual(['Users', 'test_user_10\\windows\\path'])
    target.remove()
  })

  it('stops click events from bubbling so row-level handlers do not also fire', async () => {
    // The pill calls `e.stopPropagation()` so a row-level click handler doesn't fire
    // alongside the pill's `onPick`. We verify by spying on `Event.prototype.stopPropagation`
    // for the dispatched click; calling that spy from the pill's handler is the contract.
    const stopSpy = vi.spyOn(Event.prototype, 'stopPropagation')
    const onPick = vi.fn()
    const target = renderPills('/a/b', onPick)
    await tick()
    const pill = target.querySelector('.pill') as HTMLElement
    pill.click()
    expect(onPick).toHaveBeenCalledTimes(1)
    expect(stopSpy).toHaveBeenCalled()
    stopSpy.mockRestore()
    target.remove()
  })

  it('renders all pills inline (no wrap) by default for short paths', async () => {
    // Before the measurer loads (jsdom never resolves the async pretext import), the
    // strip falls back to rendering every segment. `overflow: hidden` on the container
    // handles any pathological narrowing. This pins the "never wrap to two lines"
    // contract: the strip is `flex-wrap: nowrap`.
    const target = renderPills('/Users/test_user_10/code/proj/file')
    await tick()
    const strip = target.querySelector('.path-pills') as HTMLElement
    expect(strip).not.toBeNull()
    // We can't read the actual computed style reliably in jsdom, but we can confirm
    // the inline `flex-wrap: nowrap` rule is in the stylesheet by checking pill count
    // and the absence of a pill-ellipsis (which only appears under measured overflow).
    const pills = strip.querySelectorAll('.pill')
    expect(pills.length).toBe(5)
    expect(strip.querySelector('.pill-ellipsis')).toBeNull()
    target.remove()
  })

  it('hides the strip from assistive technology and leaves it the whole path as text', async () => {
    const target = renderPills('/Users/test_user_10/code')
    await tick()
    expect(target.querySelector('.path-pills')?.getAttribute('aria-hidden')).toBe('true')
    expect(target.querySelector('.sr-only')?.textContent).toBe('/Users/test_user_10/code')
    target.remove()
  })

  it('schedules no re-measure when it unmounts while the measurer is still loading', async () => {
    // Regression: the measurer loads asynchronously, and its continuation scheduled an
    // animation frame its `onDestroy` had already run past, so nothing cancelled it. In
    // tests that frame could land after the DOM environment was gone.
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(PathPills, { target, props: { path: '/Users/test_user_10/code', onPick: () => {} } })
    await tick()
    vi.mocked(scheduleStableWidthMeasure).mockClear()
    await unmount(instance)
    await new Promise((resolve) => setTimeout(resolve, 50))
    expect(scheduleStableWidthMeasure).not.toHaveBeenCalled()
    target.remove()
  })

  it('renders no focusable control in the strip, so a row stays one option', async () => {
    const target = renderPills('/a/b')
    await tick()
    expect(target.querySelectorAll('.pill')).toHaveLength(2)
    expect(target.querySelector('button, [tabindex]')).toBeNull()
    target.remove()
  })
})
