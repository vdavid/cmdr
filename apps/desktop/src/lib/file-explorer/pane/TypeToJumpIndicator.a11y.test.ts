/**
 * Tier 3 a11y tests for `TypeToJumpIndicator.svelte`.
 *
 * Tooltip-like overlay surfaced in the bottom-right of the pane while the
 * user is typing for in-directory navigation. Three states:
 *
 * - Hidden — the component renders nothing (no DOM node).
 * - Active — visible with a fresh buffer, indicator says "Jump: …".
 * - Stale — buffer reset fired, indicator still visible, italic + dim. Still
 *   needs to announce (the live region must stay polite, not off).
 *
 * Plus a `prefers-reduced-motion: reduce` check that the CSS turns off the
 * opacity/font-style transitions.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import TypeToJumpIndicator from './TypeToJumpIndicator.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const here = path.dirname(fileURLToPath(import.meta.url))

describe('TypeToJumpIndicator a11y', () => {
  it('hidden state renders nothing (no DOM node)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: '', visible: false, stale: false },
    })
    await tick()
    // Nothing visible — the {#if visible} guard removes the element entirely.
    expect(target.querySelector('.type-to-jump-indicator')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('active state carries role="status", aria-live="polite", and the buffer in its accessible name', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: 'fil', visible: true, stale: false },
    })
    await tick()

    const el = target.querySelector('.type-to-jump-indicator')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('role')).toBe('status')
    expect(el?.getAttribute('aria-live')).toBe('polite')
    // Accessible name surfaces the buffer so screen-reader users hear "Jump to fil".
    expect(el?.getAttribute('aria-label')).toBe('Jump to fil')
    // Visible text still includes the buffer for sighted users.
    expect(el?.textContent).toContain('fil')

    await expectNoA11yViolations(target)
  })

  it('stale state still announces (live region stays polite, not off)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: 'co', visible: true, stale: true },
    })
    await tick()

    const el = target.querySelector('.type-to-jump-indicator')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('role')).toBe('status')
    // Critical: the live region must NOT be flipped to `aria-live="off"` when
    // the indicator shifts to stale — that would suppress the announcement
    // for the next keystroke. The component leaves it polite.
    expect(el?.getAttribute('aria-live')).toBe('polite')
    expect(el?.classList.contains('is-stale')).toBe(true)

    await expectNoA11yViolations(target)
  })

  it('prefers-reduced-motion: reduce disables the CSS transition', () => {
    // jsdom doesn't evaluate `prefers-reduced-motion` against `getComputedStyle`,
    // and the Svelte vite plugin processes the component's scoped CSS through
    // a separate stylesheet that doesn't materialize as a `<style>` tag in
    // jsdom either. So we assert the contract at the source: the component
    // contains a `prefers-reduced-motion: reduce` block setting `transition:
    // none` on the indicator. If the rule disappears, this catches it.
    const source = readFileSync(path.join(here, 'TypeToJumpIndicator.svelte'), 'utf8')
    expect(source).toMatch(/prefers-reduced-motion:\s*reduce/)
    expect(source).toMatch(/transition:\s*none/)
  })
})
