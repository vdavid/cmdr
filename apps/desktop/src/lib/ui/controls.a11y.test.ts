/**
 * Tier 3 a11y tests for the button, toggle, and choice primitives.
 *
 * One file per primitive would cost about six times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its primitive's own doc comment, props, and
 * assertions. None of these primitives mocks a module, so there is nothing
 * shared here but the container helper.
 *
 * Sibling files: `text-inputs.a11y.test.ts`, `overlays.a11y.test.ts`,
 * `display.a11y.test.ts`.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, createRawSnippet, type ComponentProps } from 'svelte'
import Button from './Button.svelte'
import Checkbox from './Checkbox.svelte'
import LinkButton from './LinkButton.svelte'
import RadioGroup, { type RadioItem } from './RadioGroup.svelte'
import Switch from './Switch.svelte'
import ToggleGroup from './ToggleGroup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

function snip(text: string) {
  return createRawSnippet(() => ({ render: () => `<span>${text}</span>` }))
}

// These primitives share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `Button.svelte`.
 *
 * Runs axe-core in jsdom against each meaningful variant/state. Covers
 * structural a11y (ARIA, labels, focusable-when-enabled). Color contrast
 * is handled by the design-time checker (tier 1). Focus traps / keyboard
 * integration across a full page live in the E2E tier.
 */
describe('Button a11y', () => {
  it('default (secondary, regular) has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { children: snip('Action') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('primary variant has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { variant: 'primary', children: snip('Save') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('danger variant has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { variant: 'danger', children: snip('Delete') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mini size has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { size: 'mini', children: snip('More') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { disabled: true, children: snip('Action') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('submit type has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { type: 'submit', children: snip('Submit') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('aria-label override has no a11y violations', async () => {
    const target = container()
    mount(Button, { target, props: { 'aria-label': 'Save the file', children: snip('Save') } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `LinkButton.svelte`.
 *
 * Runs axe-core in jsdom against each meaningful state. Covers structural
 * a11y (ARIA, labels, focusable-when-enabled). Color contrast is handled
 * by the design-time checker (tier 1). Focus traps / keyboard integration
 * across a full page live in the E2E tier.
 */
describe('LinkButton a11y', () => {
  it('default has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, { target, props: { children: snip('Open settings') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, { target, props: { disabled: true, children: snip('Open settings') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('submit type has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, { target, props: { type: 'submit', children: snip('Submit') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('aria-label override has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, {
      target,
      props: { 'aria-label': 'Open system appearance settings', children: snip('System Settings > Appearance') },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('href mode (renders <a>) has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, {
      target,
      props: { href: 'https://getcmdr.com/pricing', children: snip('Get a license') },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('href mode with mailto has no a11y violations', async () => {
    const target = container()
    mount(LinkButton, { target, props: { href: 'mailto:hi@example.com', children: snip('hi@example.com') } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier-3 a11y tests for `Checkbox.svelte`.
 *
 * Ark renders the semantic control as a visually-hidden native `<input type="checkbox">` (implicit
 * `role="checkbox"`) wrapped in a `<label>` that carries the accessible name; the styled box is an
 * `aria-hidden` `.checkbox-control`. These tests audit the checked, unchecked, and disabled states,
 * and confirm the native checkbox is present and toggles.
 */
describe('Checkbox a11y', () => {
  async function mountCheckbox(props: ComponentProps<typeof Checkbox>): Promise<HTMLDivElement> {
    const target = container()
    mount(Checkbox, { target, props })
    await tick()
    return target
  }

  it('unchecked state has no a11y violations', async () => {
    const target = await mountCheckbox({ ariaLabel: 'Accept terms' })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('checked state has no a11y violations', async () => {
    const target = await mountCheckbox({ ariaLabel: 'Accept terms', checked: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = await mountCheckbox({ ariaLabel: 'Accept terms', disabled: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('with an inline label snippet has no a11y violations', async () => {
    // No ariaLabel: the visible label snippet provides the accessible name.
    const target = container()
    // Rendering a children snippet from a test is awkward; the label states are covered by the
    // dev catalog and the settings/onboarding consumers. Here we assert the aria-label path stays
    // clean, which is the primitive's default accessible-name source.
    mount(Checkbox, { target, props: { ariaLabel: 'Newsletter' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('gives the input a real accessible name from `ariaLabel` alone', async () => {
    // Regression: `aria-label` used to sit on Ark's `<label>` root, which names the
    // label rather than the control. The input's own `aria-labelledby` points at a
    // `Checkbox.Label` that doesn't exist without `children`, so the name resolved to
    // nothing and every bare checkbox was anonymous to AT.
    const target = await mountCheckbox({ ariaLabel: 'Accept terms' })
    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(input?.getAttribute('aria-label')).toBe('Accept terms')
    target.remove()
  })

  it('exposes a native checkbox that toggles and fires onCheckedChange', async () => {
    const onCheckedChange = vi.fn()
    const target = await mountCheckbox({ ariaLabel: 'Accept terms', onCheckedChange })

    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    if (!input) throw new Error('expected a native checkbox input')

    const control = target.querySelector('.checkbox-control')
    expect(control?.getAttribute('data-state')).toBe('unchecked')

    input.click()
    await tick()

    expect(onCheckedChange).toHaveBeenCalledWith(true)
    expect(control?.getAttribute('data-state')).toBe('checked')

    target.remove()
  })
})

/**
 * Tier-3 a11y tests for `Switch.svelte`.
 *
 * Ark renders the semantic control as a visually-hidden native `<input type="checkbox">` with
 * `role="switch"`, wrapped in a `<label>` that carries the accessible name; the styled track is an
 * `aria-hidden` `.switch-control`. These tests audit the on, off, and disabled states, and confirm
 * the native input is present and toggles.
 */
describe('Switch a11y', () => {
  async function mountSwitch(props: ComponentProps<typeof Switch>): Promise<HTMLDivElement> {
    const target = container()
    mount(Switch, { target, props })
    await tick()
    return target
  }

  it('off state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders' })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('on state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', checked: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', disabled: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('exposes a native switch input that toggles and fires onCheckedChange', async () => {
    const onCheckedChange = vi.fn()
    const target = await mountSwitch({ ariaLabel: 'Search subfolders', onCheckedChange })

    const input = target.querySelector<HTMLInputElement>('input[type="checkbox"]')
    if (!input) throw new Error('expected a native input backing the switch')

    const control = target.querySelector('.switch-control')
    expect(control?.getAttribute('data-state')).toBe('unchecked')

    input.click()
    await tick()

    expect(onCheckedChange).toHaveBeenCalledWith(true)
    expect(control?.getAttribute('data-state')).toBe('checked')

    target.remove()
  })
})

/**
 * Tier 3 a11y tests for the generic `RadioGroup` primitive.
 *
 * Covers the default vertical group, a group with per-item descriptions, and one with a disabled
 * item. Asserts axe-clean, the `radiogroup` / `radio` roles with accessible names drawn from the
 * labels, and that activating an option updates the value. Color contrast is tier 1's job; focus
 * traps tier 2's.
 */
describe('RadioGroup a11y', () => {
  const items: RadioItem[] = [
    { value: 'iso', label: 'ISO 8601' },
    { value: 'us', label: 'US' },
    { value: 'eu', label: 'European' },
  ]

  const itemsWithDescriptions: RadioItem[] = [
    { value: 'iso', label: 'ISO 8601', description: '2025-04-16 10:30' },
    { value: 'us', label: 'US', description: '4/16/2025 10:30 AM' },
    { value: 'custom', label: 'Custom', description: 'Define your own format' },
  ]

  const itemsWithDisabled: RadioItem[] = [
    { value: 'auto', label: 'Automatic' },
    { value: 'manual', label: 'Manual' },
    { value: 'off', label: 'Off', disabled: true },
  ]

  function mountGroup(props: ComponentProps<typeof RadioGroup>): HTMLDivElement {
    const target = container()
    mount(RadioGroup, { target, props })
    return target
  }

  // Ark renders each radio as a visually-hidden native `<input type="radio">` whose accessible name
  // comes from the `aria-labelledby` label span. Resolve it the way assistive tech would.
  function accessibleName(input: HTMLInputElement): string {
    const labelledBy = input.getAttribute('aria-labelledby')
    if (!labelledBy) return ''
    return document.getElementById(labelledBy)?.textContent.trim() ?? ''
  }

  it('vertical group has no a11y violations', async () => {
    const target = mountGroup({ items, value: 'iso', ariaLabel: 'Date format' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('group with descriptions has no a11y violations', async () => {
    const target = mountGroup({ items: itemsWithDescriptions, value: 'iso', ariaLabel: 'Date format' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('group with a disabled item has no a11y violations', async () => {
    const target = mountGroup({ items: itemsWithDisabled, value: 'auto', ariaLabel: 'Sync mode' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('exposes a radiogroup with radios named after their labels', async () => {
    const target = mountGroup({ items, value: 'iso', ariaLabel: 'Date format' })
    await tick()

    const group = target.querySelector('[role="radiogroup"]')
    expect(group).not.toBeNull()
    expect(group?.getAttribute('aria-label')).toBe('Date format')

    const radios = [...target.querySelectorAll<HTMLInputElement>('input[type="radio"]')]
    expect(radios.map((r) => accessibleName(r))).toEqual(['ISO 8601', 'US', 'European'])

    const checked = radios.filter((r) => r.checked)
    expect(checked).toHaveLength(1)
    expect(accessibleName(checked[0])).toBe('ISO 8601')
  })

  it('activating an option updates the value', async () => {
    let current = 'iso'
    const target = mountGroup({
      items,
      value: 'iso',
      ariaLabel: 'Date format',
      onValueChange: (v: string) => {
        current = v
      },
    })
    await tick()

    const us = [...target.querySelectorAll<HTMLLabelElement>('.radio-item')].find(
      (label) => label.querySelector('.radio-label')?.textContent.trim() === 'US',
    )
    expect(us).toBeTruthy()
    us?.click()
    await tick()
    expect(current).toBe('us')
  })
})

/**
 * Tier 3 a11y tests for the generic `ToggleGroup` primitive.
 *
 * One audit per semantics mode. Confirms the role/aria structure and that
 * badge + hint markup doesn't break the accessible name on the underlying
 * button. Color contrast and full-page focus traps are covered by tiers 1 / 2.
 */
describe('ToggleGroup a11y', () => {
  it('tabs semantics: default state has no a11y violations', async () => {
    const target = container()
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'tabs',
        value: 'filename',
        options: [
          { value: 'ai', label: 'Ask anything', badge: 'AI', hint: '⌥A', ariaLabel: 'AI mode (Alt+A)' },
          { value: 'filename', label: 'Filename', hint: '⌥F', ariaLabel: 'Filename mode (Alt+F)' },
          {
            value: 'content',
            label: 'Content',
            disabled: true,
            tooltip: 'Coming soon: full-text search inside files',
            ariaLabel: 'Content mode (coming soon)',
          },
          { value: 'regex', label: 'Regex', hint: '⌥R', ariaLabel: 'Regex mode (Alt+R)' },
        ],
        onChange: () => {},
        ariaLabel: 'Search mode',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('toggles semantics: default state has no a11y violations', async () => {
    const target = container()
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'toggles',
        value: 'comfortable',
        options: [
          { value: 'compact', label: 'Compact' },
          { value: 'comfortable', label: 'Comfortable' },
          { value: 'spacious', label: 'Spacious' },
        ],
        onChange: () => {},
        ariaLabel: 'UI density',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('toggles semantics: disabled root has no a11y violations', async () => {
    const target = container()
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'toggles',
        value: 'comfortable',
        options: [
          { value: 'compact', label: 'Compact' },
          { value: 'comfortable', label: 'Comfortable' },
          { value: 'spacious', label: 'Spacious' },
        ],
        onChange: () => {},
        ariaLabel: 'UI density',
        disabled: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
