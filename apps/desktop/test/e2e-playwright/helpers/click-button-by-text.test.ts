/**
 * Unit tests for `clickButtonByText`, the E2E suite's only sanctioned button press.
 *
 * They run against happy-dom rather than the app because the trap they anchor is
 * pure DOM semantics: `element.click()` on a `disabled` button dispatches nothing
 * and returns normally, so a helper that presses and reports success claims an
 * answer that never reached the backend. happy-dom reproduces that suppression
 * faithfully (probed 2026-08-18, happy-dom via `vitest.config.ts`), which is what
 * makes a browserless anchor for it possible at all.
 *
 * The payload under test is the `evaluate` STRING the helper ships into the
 * webview, so these tests execute that exact string; paraphrasing it in TypeScript
 * would test a different program than the one the suite runs.
 */

import { beforeEach, describe, expect, it } from 'vitest'
import type { PageLike } from './core.js'
import { clickButtonByText } from './overlays-and-dialogs.js'

/** Trimmed labels of every press whose handler actually ran, in order. */
let presses: string[] = []

/**
 * A `PageLike` whose `evaluate` runs the helper's real payload against the
 * happy-dom document. `pollUntil` ignores the page argument, and the helper only
 * ever calls `evaluate`, so nothing else needs standing up.
 */
const page = {
  evaluate: (js: string): Promise<unknown> => {
    // eslint-disable-next-line @typescript-eslint/no-implied-eval -- the evaluate payload IS the code under test; the whole point is to run it verbatim.
    const run = new Function(`return ${js}`) as () => unknown
    return Promise.resolve(run())
  },
} as unknown as PageLike

/** Appends a button to the row, recording its presses. `label` is used verbatim. */
function addButton(label: string, options: { disabled?: boolean; ariaDisabled?: boolean } = {}): HTMLButtonElement {
  const button = document.createElement('button')
  button.textContent = label
  if (options.disabled === true) button.disabled = true
  if (options.ariaDisabled === true) button.setAttribute('aria-disabled', 'true')
  button.addEventListener('click', () => {
    presses.push(label.trim())
  })
  document.querySelector('.row')?.appendChild(button)
  return button
}

describe('clickButtonByText', () => {
  beforeEach(() => {
    presses = []
    document.body.innerHTML = '<div class="row"></div>'
  })

  it('waits out the disabled window rather than reporting a press the DOM swallowed', async () => {
    // The shape that wedged CI: a dialog disables its buttons for the whole IPC
    // round trip of the previous answer, and the spec presses inside that window.
    // Pre-fix the helper pressed once, saw `click()` return, and reported success
    // while `presses` stayed empty — so the answer never reached the backend and
    // the operation parked until teardown.
    const button = addButton('Overwrite', { disabled: true })
    setTimeout(() => {
      button.disabled = false
    }, 120)

    await clickButtonByText(page, '.row button', 'Overwrite', 2000)

    expect(presses).toEqual(['Overwrite'])
  })

  it('fails naming the disabled button when it never becomes actionable', async () => {
    addButton('Skip', { disabled: true })

    await expect(clickButtonByText(page, '.row button', 'Skip', 200)).rejects.toThrow(/`disabled` the whole time/)
    expect(presses).toEqual([])
  })

  it('fails naming the missing button, the opposite bug from a stuck-disabled one', async () => {
    addButton('Skip')

    await expect(clickButtonByText(page, '.row button', 'Rename', 200)).rejects.toThrow(
      /no element under that selector carried that exact trimmed text/,
    )
  })

  it('matches trimmed text exactly, so "Skip" never presses "Skip all"', async () => {
    addButton('Skip all')

    await expect(clickButtonByText(page, '.row button', 'Skip', 200)).rejects.toThrow(/no element under that selector/)

    // Svelte renders a button's label with surrounding whitespace, so the match
    // has to trim while staying exact.
    addButton('\n  Skip\n')
    await clickButtonByText(page, '.row button', 'Skip', 500)

    expect(presses).toEqual(['Skip'])
  })

  it('presses an aria-disabled button, which stays clickable so its handler can explain the block', async () => {
    addButton('Overwrite all smaller', { ariaDisabled: true })

    await clickButtonByText(page, '.row button', 'Overwrite all smaller', 500)

    expect(presses).toEqual(['Overwrite all smaller'])
  })
})
