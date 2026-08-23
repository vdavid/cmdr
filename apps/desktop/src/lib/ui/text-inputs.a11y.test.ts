/**
 * Tier 3 a11y tests for the value-entry primitives: the two text fields, the
 * number field, the slider, and the copyable code box.
 *
 * One file per primitive would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its primitive's own doc comment, props, and
 * assertions. Only `CopyBox` mocks anything, and nothing else here imports that
 * module, so the mock reaches exactly what it did before.
 *
 * Sibling files: `controls.a11y.test.ts`, `overlays.a11y.test.ts`,
 * `display.a11y.test.ts`.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { createRawSnippet, mount, tick, type ComponentProps } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

import CopyBox from './CopyBox.svelte'
import NumberInput from './NumberInput.svelte'
import Slider from './Slider.svelte'
import TextArea from './TextArea.svelte'
import TextInput from './TextInput.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

// These primitives share one jsdom document, and axe resolves ARIA id references
// document-wide (two blocks below name their field through an external
// `<label for>`). Clearing between tests keeps each audit to its own container.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for the `TextInput` primitive.
 *
 * Covers the naming surfaces (`ariaLabel` vs an external `<label for>`), the leading-icon and
 * trailing-control shapes, and the invalid / disabled states. Contrast is tier 1's job, focus
 * behavior across siblings is tier 2's.
 */
describe('TextInput a11y', () => {
  /** A trailing control the way real call sites pass one (a reveal toggle / clear button). */
  const trailingButton = createRawSnippet(() => ({
    render: () => '<button type="button" aria-label="Clear the search">x</button>',
  }))

  it('named by ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(TextInput, { target, props: { value: 'Documents', ariaLabel: 'Folder name' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('named by an external <label for> has no a11y violations', async () => {
    const target = container()
    const label = document.createElement('label')
    label.htmlFor = 'server-address'
    label.textContent = 'Server address'
    target.appendChild(label)
    mount(TextInput, { target, props: { value: 'smb://nas.local', id: 'server-address' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a leading icon has no a11y violations', async () => {
    const target = container()
    mount(TextInput, {
      target,
      props: { value: '', placeholder: 'Search', radius: 'full', leadingIcon: 'search', ariaLabel: 'Search settings' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a trailing control has no a11y violations', async () => {
    const target = container()
    mount(TextInput, {
      target,
      props: { value: 'query', ariaLabel: 'Search settings', trailing: trailingButton },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('invalid state has no a11y violations', async () => {
    const target = container()
    mount(TextInput, { target, props: { value: 'nope', invalid: true, ariaLabel: 'Server address' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = container()
    mount(TextInput, { target, props: { value: 'locked', disabled: true, ariaLabel: 'API key' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('password type has no a11y violations', async () => {
    const target = container()
    mount(TextInput, { target, props: { value: 'hunter2', type: 'password', ariaLabel: 'Archive password' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for the `TextArea` primitive (`TextInput`'s multi-line sibling). */
describe('TextArea a11y', () => {
  it('named by ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(TextArea, { target, props: { value: 'It crashed when I hit F5.', ariaLabel: 'What happened?', rows: 4 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('named by an external <label for> has no a11y violations', async () => {
    const target = container()
    const label = document.createElement('label')
    label.htmlFor = 'feedback-body'
    label.textContent = 'Your feedback'
    target.appendChild(label)
    mount(TextArea, { target, props: { value: '', id: 'feedback-body', rows: 4 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read-only, non-resizable state has no a11y violations', async () => {
    const target = container()
    mount(TextArea, {
      target,
      props: { value: 'Copy failed on 3 files.', readonly: true, resizable: false, ariaLabel: 'Error detail', rows: 6 },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('invalid and disabled states have no a11y violations', async () => {
    const target = container()
    mount(TextArea, { target, props: { value: 'x', invalid: true, disabled: true, ariaLabel: 'Notes', rows: 3 } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for the `NumberInput` primitive.
 *
 * Covers the plain field, one with a unit, and the disabled state. Asserts axe-clean, a named
 * spinbutton, that both steppers carry an accessible name naming the field, and that an emptied
 * field doesn't commit `NaN`. Color contrast is tier 1's job; focus traps tier 2's.
 */
describe('NumberInput a11y', () => {
  function mountInput(props: ComponentProps<typeof NumberInput>): HTMLDivElement {
    const target = container()
    mount(NumberInput, { target, props })
    return target
  }

  it('plain field has no a11y violations', async () => {
    const target = mountInput({ value: 5, onChange: () => {}, min: 0, max: 10, ariaLabel: 'Parallel workers' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('field with a unit has no a11y violations', async () => {
    const target = mountInput({
      value: 400,
      onChange: () => {},
      min: 250,
      max: 1000,
      step: 25,
      unit: 'px',
      ariaLabel: 'Column width limit',
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled field has no a11y violations', async () => {
    const target = mountInput({
      value: 12,
      onChange: () => {},
      min: 0,
      max: 99,
      ariaLabel: 'Disabled number input',
      disabled: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('names the field and both steppers', async () => {
    const target = mountInput({ value: 5, onChange: () => {}, min: 0, max: 10, ariaLabel: 'Parallel workers' })
    await tick()

    const input = target.querySelector('input')
    expect(input?.getAttribute('aria-label')).toBe('Parallel workers')

    const stepperNames = [...target.querySelectorAll('button')].map((b) => b.getAttribute('aria-label'))
    expect(stepperNames).toEqual(['Decrease Parallel workers', 'Increase Parallel workers'])
  })

  it('clamps to the bounds and never commits an emptied field', async () => {
    const seen: number[] = []
    const target = mountInput({
      value: 5,
      onChange: (v: number) => {
        seen.push(v)
      },
      min: 1,
      max: 9,
      ariaLabel: 'Compression level',
    })
    await tick()

    const input = target.querySelector('input')
    if (!input) throw new Error('number input not found')

    // Ark reads the field through its focused-input handler, so focus first: an `input` event on
    // an unfocused field is ignored and this test would silently assert nothing.
    input.focus()
    input.value = '50'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    // Emptying the field parses as NaN. Committing it would write a broken number to the store,
    // so it's swallowed until Ark's clamp-on-blur restores a real value.
    input.value = ''
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    expect(seen).toEqual([9])
  })
})

/**
 * Tier 3 a11y tests for the `Slider` primitive.
 *
 * Covers the bare track, the decorated shape (ticks, end labels, readout), and the disabled
 * state. Asserts axe-clean, a named `role="slider"` carrying the value, that the decorations
 * stay out of the accessibility tree, and that `ariaValueText` names the value when the raw
 * number wouldn't mean anything. Color contrast is tier 1's job; focus traps tier 2's.
 */
describe('Slider a11y', () => {
  const BUCKETS = ['Only my most-used', 'Often used', 'Sometimes used', 'Most folders', 'Everywhere']

  function mountSlider(props: ComponentProps<typeof Slider>): HTMLDivElement {
    const target = container()
    mount(Slider, { target, props })
    return target
  }

  it('bare slider has no a11y violations', async () => {
    const target = mountSlider({ value: 50, onChange: () => {}, min: 0, max: 100, ariaLabel: 'Plain slider' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('decorated slider (ticks, end labels, readout) has no a11y violations', async () => {
    const target = mountSlider({
      value: 100,
      onChange: () => {},
      min: 75,
      max: 150,
      step: 5,
      ariaLabel: 'Text size',
      ticks: [75, 100, 125, 150],
      snapTargets: [75, 100, 125, 150],
      endLabels: ['Smaller', 'Larger'],
      valueLabel: '100%',
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled slider has no a11y violations', async () => {
    const target = mountSlider({
      value: 40,
      onChange: () => {},
      min: 0,
      max: 100,
      ariaLabel: 'Disabled slider',
      disabled: true,
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('exposes a named slider carrying the value', async () => {
    const target = mountSlider({ value: 30, onChange: () => {}, min: 0, max: 100, ariaLabel: 'Text size' })
    await tick()

    const slider = target.querySelector('[role="slider"]')
    expect(slider).not.toBeNull()
    expect(slider?.getAttribute('aria-label')).toBe('Text size')
    expect(slider?.getAttribute('aria-valuenow')).toBe('30')
    expect(slider?.getAttribute('aria-valuemin')).toBe('0')
    expect(slider?.getAttribute('aria-valuemax')).toBe('100')
  })

  it('announces a named value when the raw number is meaningless', async () => {
    const target = mountSlider({
      value: 3,
      onChange: () => {},
      min: 0,
      max: 4,
      ariaLabel: 'Coverage',
      ariaValueText: (v: number) => BUCKETS[v],
      valueLabel: BUCKETS[3],
      valueLabelPlacement: 'above',
    })
    await tick()

    const slider = target.querySelector('[role="slider"]')
    expect(slider?.getAttribute('aria-valuetext')).toBe('Most folders')
  })

  it('keeps the readout, ticks, and end labels out of the accessibility tree', async () => {
    const target = mountSlider({
      value: 6,
      onChange: () => {},
      min: 1,
      max: 9,
      ariaLabel: 'Compression level',
      ticks: [1, 5, 9],
      endLabels: ['Faster', 'Smaller'],
      valueLabel: '6',
    })
    await tick()

    // Duplicating the value and the track's decoration for a screen reader would just be noise:
    // the slider already announces its own value and bounds.
    for (const selector of ['.sl-value', '.sl-ticks', '.sl-ends']) {
      const el = target.querySelector(selector)
      expect(el, selector).not.toBeNull()
      expect(el?.getAttribute('aria-hidden'), selector).toBe('true')
    }
  })

  it('does not render a hidden input inside the thumb', async () => {
    // A focusable input nested in the thumb trips axe's nested-interactive rule, and nothing
    // here posts a form. Guards against a well-meaning "Ark supports HiddenInput" edit.
    const target = mountSlider({ value: 1, onChange: () => {}, min: 0, max: 2, ariaLabel: 'No hidden input' })
    await tick()
    expect(target.querySelector('input')).toBeNull()
  })
})

/**
 * Tier 3 a11y tests for `CopyBox.svelte`.
 *
 * Checks that the monospace text + Copy button combo exposes a labelled button
 * and doesn't fall into any common axe traps.
 */
describe('CopyBox a11y', () => {
  it('default (short command) has no a11y violations', async () => {
    const target = container()
    mount(CopyBox, { target, props: { text: 'ls -la' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long multi-argument command has no a11y violations', async () => {
    const target = container()
    mount(CopyBox, {
      target,
      props: { text: 'sudo defaults write com.apple.Finder AppleShowAllFiles -bool true' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a path with a shortened display and its own copy label has no a11y violations', async () => {
    const target = container()
    mount(CopyBox, {
      target,
      props: {
        text: '/Volumes/Naspolya/media/photos/2026/07-summer-archive/DSC09241.arw',
        displayText: '/Volumes/Naspolya/media/…/DSC09241.arw',
        copyAriaLabel: 'Copy path to clipboard',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
