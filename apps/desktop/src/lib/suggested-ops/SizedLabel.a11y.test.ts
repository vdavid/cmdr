/**
 * `SizedLabel`: a button label as wide as the widest of a set. The spares that size it must
 * never reach a screen reader or the button's text: the button's name is its own label alone.
 */

import { describe, expect, it } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import SizedLabel from './SizedLabel.svelte'

function mountInButton(label: string, labels: string[]): HTMLButtonElement {
  const button = document.createElement('button')
  button.type = 'button'
  document.body.appendChild(button)
  mount(SizedLabel, { target: button, props: { label, labels } })
  flushSync()
  return button
}

describe('SizedLabel', () => {
  it('has no violations inside a button', async () => {
    const button = mountInButton('Move 3 files', ['Move 3 files', 'Move 5 files to Trash', 'Delete 2 files'])

    await expectNoA11yViolations(button.parentElement ?? document.body)
  })

  it('names the button with its own label alone', () => {
    const button = mountInButton('Move 3 files', ['Move 3 files', 'Move 5 files to Trash', 'Delete 2 files'])

    expect(button.textContent.trim()).toBe('Move 3 files')
    const spares = [...button.querySelectorAll('[data-label]')]
    expect(spares.map((spare) => spare.getAttribute('data-label'))).toEqual(['Move 5 files to Trash', 'Delete 2 files'])
    for (const spare of spares) expect(spare.getAttribute('aria-hidden')).toBe('true')
  })

  it('holds one spare per distinct other label', () => {
    const button = mountInButton('Move 3 files', ['Move 3 files', 'Copy 2 files', 'Copy 2 files', 'Move 3 files'])

    expect(button.querySelectorAll('[data-label]')).toHaveLength(1)
  })
})
