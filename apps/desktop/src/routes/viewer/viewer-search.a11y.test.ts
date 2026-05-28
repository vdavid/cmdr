/**
 * Tier-3 a11y test for the viewer's search-bar toggles.
 *
 * The toggles live inline in `+page.svelte`. Mounting the entire viewer here
 * would pull in the IPC, virtual scroll, and window-management graph; we'd be
 * testing infrastructure, not a11y. Instead we materialize the same markup
 * the viewer renders and run axe against it. The byte-for-byte fidelity is
 * the pre-condition: any future change to the toolbar must also update this
 * fixture.
 */

import { describe, it, expect } from 'vitest'
import { expectNoA11yViolations } from '$lib/test-a11y'

function renderToolbarFixture(opts: {
  useRegex: boolean
  caseSensitive: boolean
  searchError?: string | null
}): HTMLElement {
  const target = document.createElement('div')
  target.innerHTML = `
    <div class="search-bar" role="search">
      <input
        type="text"
        placeholder="Find in file..."
        aria-label="Search text"
        class="search-input"
      />
      <button
        type="button"
        class="search-toggle ${opts.caseSensitive ? 'active' : ''}"
        aria-pressed="${String(opts.caseSensitive)}"
        aria-label="Case sensitive"
      >Aa</button>
      <button
        type="button"
        class="search-toggle ${opts.useRegex ? 'active' : ''}"
        aria-pressed="${String(opts.useRegex)}"
        aria-label="Regex"
      >.*</button>
      <span class="match-count" aria-live="polite">
        ${opts.searchError ? `<span class="search-error" role="alert">${opts.searchError}</span>` : ''}
      </span>
      <button type="button" aria-label="Previous match">▲</button>
      <button type="button" aria-label="Next match">▼</button>
      <button type="button" aria-label="Close search">✕</button>
    </div>
  `
  document.body.appendChild(target)
  return target
}

describe('viewer search-bar a11y', () => {
  it('default state (case on, regex off) has no a11y violations', async () => {
    const target = renderToolbarFixture({ useRegex: false, caseSensitive: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('regex enabled has no a11y violations', async () => {
    const target = renderToolbarFixture({ useRegex: true, caseSensitive: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('invalid-query error variant has no a11y violations', async () => {
    const target = renderToolbarFixture({
      useRegex: true,
      caseSensitive: true,
      searchError: 'Invalid regex: parse error',
    })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('toggles expose aria-pressed and aria-label', () => {
    const target = renderToolbarFixture({ useRegex: true, caseSensitive: false })
    const caseToggle = target.querySelector<HTMLButtonElement>('button[aria-label="Case sensitive"]')
    const regexToggle = target.querySelector<HTMLButtonElement>('button[aria-label="Regex"]')
    expect(caseToggle?.getAttribute('aria-pressed')).toBe('false')
    expect(regexToggle?.getAttribute('aria-pressed')).toBe('true')
    target.remove()
  })

  it('error variant has role="alert"', () => {
    const target = renderToolbarFixture({
      useRegex: true,
      caseSensitive: true,
      searchError: 'Bad regex',
    })
    const alert = target.querySelector('[role="alert"]')
    expect(alert).not.toBeNull()
    expect(alert?.textContent.trim()).toBe('Bad regex')
    target.remove()
  })
})
