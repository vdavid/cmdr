import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import PathPills from './PathPills.svelte'

function renderPills(path: string, onPick: (p: string) => void = () => {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(PathPills, { target, props: { path, onPick } })
  return target
}

describe('PathPills', () => {
  it('splits an absolute POSIX path into one pill per segment', async () => {
    const target = renderPills('/Users/dave/code')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent?.trim() ?? '')
    expect(labels).toEqual(['Users', 'dave', 'code'])
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
    const target = renderPills('//Users//dave///')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent?.trim() ?? '')
    expect(labels).toEqual(['Users', 'dave'])
    target.remove()
  })

  it('renders a single "/" pill for a bare root', async () => {
    const target = renderPills('/')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent?.trim() ?? '')
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
    const pills = Array.from(target.querySelectorAll('.pill')) as HTMLButtonElement[]
    expect(pills.map((p) => p.textContent?.trim())).toEqual(['docs', 'notes'])
    expect(pills[0].title).toBe('docs')
    expect(pills[1].title).toBe('docs/notes')
    target.remove()
  })

  it('passes each ancestor path to onPick on click', async () => {
    const onPick = vi.fn()
    const target = renderPills('/Users/dave/code', onPick)
    await tick()
    const pills = Array.from(target.querySelectorAll('.pill')) as HTMLButtonElement[]
    pills[0].click()
    pills[1].click()
    pills[2].click()
    expect(onPick.mock.calls).toEqual([['/Users'], ['/Users/dave'], ['/Users/dave/code']])
    target.remove()
  })

  it('does not split on backslashes (macOS + Linux only)', async () => {
    const target = renderPills('/Users/dave\\windows\\path')
    await tick()
    const labels = Array.from(target.querySelectorAll('.pill')).map((p) => p.textContent?.trim() ?? '')
    expect(labels).toEqual(['Users', 'dave\\windows\\path'])
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
    const pill = target.querySelector('.pill') as HTMLButtonElement
    pill.click()
    expect(onPick).toHaveBeenCalledTimes(1)
    expect(stopSpy).toHaveBeenCalled()
    stopSpy.mockRestore()
    target.remove()
  })
})
