/**
 * Tier 3 a11y tests for the `TextInput` primitive.
 *
 * Covers the naming surfaces (`ariaLabel` vs an external `<label for>`), the leading-icon and
 * trailing-control shapes, and the invalid / disabled states. Contrast is tier 1's job, focus
 * behavior across siblings is tier 2's.
 */

import { describe, it } from 'vitest'
import { createRawSnippet, mount, tick } from 'svelte'
import TextInput from './TextInput.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function mountTarget(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

/** A trailing control the way real call sites pass one (a reveal toggle / clear button). */
const trailingButton = createRawSnippet(() => ({
  render: () => '<button type="button" aria-label="Clear the search">x</button>',
}))

describe('TextInput a11y', () => {
  it('named by ariaLabel has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: 'Documents', ariaLabel: 'Folder name' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('named by an external <label for> has no a11y violations', async () => {
    const target = mountTarget()
    const label = document.createElement('label')
    label.htmlFor = 'server-address'
    label.textContent = 'Server address'
    target.appendChild(label)
    mount(TextInput, { target, props: { value: 'smb://nas.local', id: 'server-address' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a leading icon has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, {
      target,
      props: { value: '', placeholder: 'Search', radius: 'full', leadingIcon: 'search', ariaLabel: 'Search settings' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a trailing control has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, {
      target,
      props: { value: 'query', ariaLabel: 'Search settings', trailing: trailingButton },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('invalid state has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: 'nope', invalid: true, ariaLabel: 'Server address' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: 'locked', disabled: true, ariaLabel: 'API key' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('password type has no a11y violations', async () => {
    const target = mountTarget()
    mount(TextInput, { target, props: { value: 'hunter2', type: 'password', ariaLabel: 'Archive password' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
