/**
 * `ProgressBar`'s shimmer switch.
 *
 * The shimmer says "this is moving". A bar that ISN'T moving (a paused
 * operation) must not keep sweeping, so the fill carries the `animated` class
 * only while the caller says the bar is live. The stylesheet then adds the
 * sweep, and only under `prefers-reduced-motion: no-preference`.
 */

import { describe, it, expect, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'
import ProgressBar from './ProgressBar.svelte'

function render(props: { value: number; animated?: boolean }): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ProgressBar, { target, props })
  flushSync()
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('ProgressBar', () => {
  it('shimmers by default, so every existing bar keeps its sweep', () => {
    const target = render({ value: 0.4 })
    expect(target.querySelector('.fill')?.classList.contains('animated')).toBe(true)
  })

  it('drops the shimmer when the bar is not moving', () => {
    const target = render({ value: 0.4, animated: false })
    expect(target.querySelector('.fill')?.classList.contains('animated')).toBe(false)
  })
})
