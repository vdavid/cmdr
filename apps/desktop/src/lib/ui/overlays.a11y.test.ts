/**
 * Tier 3 a11y tests for the primitives that render over the page: the two
 * dialogs, the popovers, the menu, and the two Ark-backed pickers.
 *
 * One file per primitive would cost about seven times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its primitive's own doc comment, props, and
 * assertions. `AlertDialog` and `ModalDialog` mocked `$lib/tauri-commands`
 * identically, so the one mock here is theirs unchanged.
 *
 * Several of these portal to `document.body` and audit the whole body, so the
 * file-level cleanup below is load-bearing: a leftover from an earlier test
 * would land inside the next block's audit.
 *
 * Sibling files: `controls.a11y.test.ts`, `text-inputs.a11y.test.ts`,
 * `display.a11y.test.ts`.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Avoid Tauri IPC side-effects from notifyDialogOpened / notifyDialogClosed.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import AlertDialog from './AlertDialog.svelte'
import Combobox, { type ComboboxItem } from './Combobox.svelte'
import FilterPopover from './FilterPopover.svelte'
import Menu, { type MenuItem } from './Menu.svelte'
import ModalDialog from './ModalDialog.svelte'
import Popover from './Popover.svelte'
import Select, { type SelectItem } from './Select.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

// The blocks that audit `document.body` (Menu, Popover, FilterPopover) would
// otherwise pick up whatever an earlier test left behind, portaled content
// included.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `AlertDialog.svelte`.
 *
 * Alert dialogs must use role="alertdialog" with labelled title + described
 * message and a primary action button. These tests check all of that via
 * axe-core: text-only variants (short message, long message, custom button
 * label) are covered.
 */
describe('AlertDialog a11y', () => {
  it('default (single OK button) has no a11y violations', async () => {
    const target = container()
    mount(AlertDialog, {
      target,
      props: {
        title: 'Something went wrong',
        message: 'We could not complete your request.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('custom button label has no a11y violations', async () => {
    const target = container()
    mount(AlertDialog, {
      target,
      props: {
        title: 'Heads up',
        message: 'You have unsaved changes.',
        buttonText: 'Dismiss',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long message with multiple sentences has no a11y violations', async () => {
    const target = container()
    mount(AlertDialog, {
      target,
      props: {
        title: 'Read error',
        message:
          'We could not read the file. It may have been moved or deleted since you opened the folder. Try refreshing the pane.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('terse title + message has no a11y violations', async () => {
    const target = container()
    mount(AlertDialog, {
      target,
      props: {
        title: 'Error',
        message: 'Not found.',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ModalDialog.svelte`.
 *
 * ModalDialog is the base for every dialog in the app. These tests cover
 * ARIA wiring (role, aria-modal, aria-labelledby, aria-describedby) and
 * the close-button label. Focus-trap and Escape behavior are covered in
 * the E2E tier (jsdom's focus model is incomplete).
 */
describe('ModalDialog a11y', () => {
  const titleSnippet = createRawSnippet(() => ({ render: () => `<span>Dialog title</span>` }))
  const bodySnippet = createRawSnippet(() => ({
    render: () => `<div><p>Dialog body copy explaining the action.</p></div>`,
  }))

  it('renders without violations with title + children', async () => {
    const target = container()
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations when onclose adds the close button', async () => {
    const target = container()
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        onclose: () => {},
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with role="alertdialog"', async () => {
    const target = container()
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        role: 'alertdialog',
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with aria-describedby wired to body', async () => {
    const descBody = createRawSnippet(() => ({
      render: () => `<div id="test-dialog-desc">Extra description for the dialog.</div>`,
    }))
    const target = container()
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        ariaDescribedby: 'test-dialog-desc',
        title: titleSnippet,
        children: descBody,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders without violations with blur overlay and draggable=false', async () => {
    const target = container()
    mount(ModalDialog, {
      target,
      props: {
        titleId: 'test-dialog-title',
        blur: true,
        draggable: false,
        title: titleSnippet,
        children: bodySnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier-3 a11y tests for `Popover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state (renders a `role="dialog"` with
 * the slot content focusable). The anchor is a real button in the test DOM so the popover has
 * something to position against.
 */
describe('Popover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = container()
    const anchor = document.createElement('button')
    anchor.textContent = 'Anchor'
    target.appendChild(anchor)
    mount(Popover, {
      target,
      props: {
        anchor,
        open: false,
        onClose: () => {},
        children: createRawSnippet(() => ({
          render: () => '<input type="text" aria-label="Test input" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open state with a labeled input has no a11y violations', async () => {
    const target = container()
    const anchor = document.createElement('button')
    anchor.textContent = 'Anchor'
    target.appendChild(anchor)
    mount(Popover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        ariaLabel: 'Test popover',
        children: createRawSnippet(() => ({
          render: () => '<label for="popover-test">Test field</label><input id="popover-test" type="text" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})

/**
 * Tier-3 a11y tests for `FilterPopover.svelte`.
 *
 * `FilterPopover` composes `Popover` (positioning, focus trap, Esc-scoped close) with a
 * section header above the filter controls. Two header shapes need covering: a plain `<span>`
 * heading above a radio grid (the default), and a real `<label for=…>` association when the
 * header labels a single control (`labelFor`). Closed state renders nothing.
 *
 * The anchor is a real button in the test DOM so the popover has something to position against.
 */
describe('FilterPopover a11y', () => {
  function makeAnchor(target: HTMLElement): HTMLButtonElement {
    const anchor = document.createElement('button')
    anchor.textContent = 'Size'
    target.appendChild(anchor)
    return anchor
  }

  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = container()
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: false,
        onClose: () => {},
        label: 'Size',
        ariaLabel: 'Size filter',
        children: createRawSnippet(() => ({
          render: () => '<input type="radio" name="size-op" aria-label="Any size" />',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open with a span heading above a radio grid has no a11y violations', async () => {
    const target = container()
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        label: 'Modified',
        ariaLabel: 'Modified filter',
        sectionClass: 'size-grid-section',
        children: createRawSnippet(() => ({
          render: () =>
            '<label><input type="radio" name="mod-op" aria-label="Any time" />Any</label>' +
            '<label><input type="radio" name="mod-op" aria-label="After" />After</label>',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })

  it('open with a labelFor association on a single control has no a11y violations', async () => {
    const target = container()
    const anchor = makeAnchor(target)
    mount(FilterPopover, {
      target,
      props: {
        anchor,
        open: true,
        onClose: () => {},
        label: 'Search in',
        ariaLabel: 'Search in filter',
        labelFor: 'scope-textarea',
        sectionClass: 'scope-popover',
        children: createRawSnippet(() => ({
          render: () => '<textarea id="scope-textarea"></textarea>',
        })),
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})

/**
 * Tier 3 a11y tests for the presentational `Menu` primitive.
 *
 * The menu renders its content whenever mounted (the caller gates it with `{#if}`),
 * so mounting it is enough to axe the open state. It portals to `document.body`, so
 * axe the whole body. Color contrast is tier 1's job; focus traps are tier 2's.
 */
describe('Menu a11y', () => {
  const items: MenuItem[] = [
    { value: 'browse', label: 'Browse like a folder' },
    { value: 'open', label: 'Open with default app' },
    { value: 'configure', label: 'Configure…' },
  ]

  it('open menu has no a11y violations', async () => {
    const target = container()
    mount(Menu, {
      target,
      props: {
        items,
        onSelect: () => {},
        onClose: () => {},
        ariaLabel: 'Open archive or bundle',
        anchorPoint: { x: 100, y: 100 },
        highlightedValue: 'browse',
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })
})

/**
 * Tier 3 a11y tests for the generic `Select` primitive.
 *
 * Covers the closed default, a grouped list, and the disabled state. Open-dropdown state is driven
 * by Ark UI state machines we don't exercise here; axe against the closed trigger is what tier 3
 * needs to catch trigger-label / aria regressions. Color contrast is tier 1's job; focus traps are
 * tier 2's.
 */
describe('Select a11y', () => {
  const flatItems: SelectItem[] = [
    { value: 'auto', label: 'Auto', description: 'Pick the unit that reads best' },
    { value: 'binary', label: 'Binary (KiB, MiB)' },
    { value: 'decimal', label: 'Decimal (KB, MB)' },
  ]

  const groupedItems: SelectItem[] = [
    { value: 'utf-8', label: 'UTF-8', group: 'Unicode' },
    { value: 'utf-16le', label: 'UTF-16 LE', group: 'Unicode' },
    { value: 'windows-1252', label: 'Windows-1252', group: 'Western' },
  ]

  it('closed (flat list with description) has no a11y violations', async () => {
    const target = container()
    mount(Select, {
      target,
      props: { items: flatItems, value: 'auto', onChange: () => {}, ariaLabel: 'File size format' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed (grouped list) has no a11y violations', async () => {
    const target = container()
    mount(Select, {
      target,
      props: { items: groupedItems, value: 'utf-8', onChange: () => {}, ariaLabel: 'Text encoding' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(Select, {
      target,
      props: { items: flatItems, value: 'auto', onChange: () => {}, ariaLabel: 'File size format', disabled: true },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for the generic `Combobox` primitive.
 *
 * Covers the closed default, an empty (cold-start) list, the loading overlay, and the disabled
 * state. The open popup is driven by Ark UI's state machine we don't exercise here; axe against the
 * closed field catches label / aria regressions. Contrast is tier 1's job, focus traps are tier 2's.
 */
describe('Combobox a11y', () => {
  const modelItems: ComboboxItem[] = [
    { value: 'gpt-4o', label: 'gpt-4o' },
    { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
  ]

  it('closed (with suggestions) has no a11y violations', async () => {
    const target = container()
    mount(Combobox, {
      target,
      props: { items: modelItems, inputValue: 'gpt-4o', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('empty list (cold start) has no a11y violations', async () => {
    const target = container()
    mount(Combobox, {
      target,
      props: { items: [], inputValue: 'my-custom-model', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('loading overlay has no a11y violations', async () => {
    const target = container()
    mount(Combobox, {
      target,
      props: { items: [], inputValue: 'gpt-4o', onInputValueChange: () => {}, loading: true, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(Combobox, {
      target,
      props: {
        items: modelItems,
        inputValue: 'gpt-4o',
        onInputValueChange: () => {},
        disabled: true,
        ariaLabel: 'Model',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
