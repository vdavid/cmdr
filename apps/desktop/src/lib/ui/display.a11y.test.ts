/**
 * Tier 3 a11y tests for the presentational primitives: chips, badges, glyphs,
 * spinners, progress, the two formatted-value labels, the column list, and the live
 * announcer.
 *
 * One file per primitive would cost about ten times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its primitive's own doc comment, props, and
 * assertions.
 *
 * The `reactive-settings` mock is the union of what `DateLabel` and `Size` each
 * stubbed; neither reads the other's export, so both see exactly what they did
 * before. `ShortcutChip`'s two mocks are its own, untouched by the rest.
 *
 * Sibling files: `controls.a11y.test.ts`, `text-inputs.a11y.test.ts`,
 * `overlays.a11y.test.ts`.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, createRawSnippet, type ComponentProps } from 'svelte'
import { expectNoA11yViolations, type A11yAuditOptions } from '#lib/test-a11y.js'

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  formattedDate: (t: number | undefined) =>
    t
      ? {
          text: '2025-03-14 10:30',
          segments: [
            { text: '2025', ageClass: 'age-fresh' as const },
            { text: '-03-14 ', ageClass: null },
            { text: '10:30', ageClass: null },
          ],
        }
      : { text: '', segments: [] },
}))

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: vi.fn(() => Promise.resolve(undefined)),
      set: vi.fn(() => Promise.resolve()),
      save: vi.fn(() => Promise.resolve()),
      keys: vi.fn(() => Promise.resolve([])),
      delete: vi.fn(() => Promise.resolve()),
    }),
  ),
}))

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import Chip from './Chip.svelte'
import ColumnList from './ColumnList.svelte'
import type { ColumnListColumn } from './column-list-types'
import DateLabel from './DateLabel.svelte'
import Icon from './Icon.svelte'
import InfoTip from './InfoTip.svelte'
import LiveAnnouncer from './LiveAnnouncer.svelte'
import LoadingIcon from './LoadingIcon.svelte'
import ProgressBar from './ProgressBar.svelte'
import SectionCard from './SectionCard.svelte'
import ShortcutChip from './ShortcutChip.svelte'
import Size from './Size.svelte'
import Spinner from './Spinner.svelte'
import StatusBadge from './StatusBadge.svelte'
import StatusGlyph from './StatusGlyph.svelte'
import { ICON_COMPONENTS, type IconName } from './icons/icon-map'

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
// document-wide. Clearing between tests keeps each audit to its own container.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier-3 a11y tests for `ColumnList.svelte`.
 *
 * Both semantics: a listbox of options (the cursor row `aria-selected`, a decorative icon
 * column's empty header hidden) and a table whose group heading is a spanning row header and
 * whose not-yet-loaded rows are hidden placeholders.
 */
describe('ColumnList a11y', () => {
  interface Row {
    name: string
    note: string
  }
  const nameCell = createRawSnippet<[{ row: Row }]>((ctx) => ({ render: () => `<span>${ctx().row.name}</span>` }))
  const noteCell = createRawSnippet<[{ row: Row }]>((ctx) => ({ render: () => `<span>${ctx().row.note}</span>` }))
  const iconCell = createRawSnippet(() => ({ render: () => '<span aria-hidden="true">·</span>' }))
  const columns: ColumnListColumn<Row>[] = [
    { id: 'icon', label: '', width: { kind: 'fixed', px: 24 }, cell: iconCell },
    { id: 'name', label: 'Name', width: { kind: 'share', minPx: 80 }, emphasis: true, cell: nameCell },
    { id: 'note', label: 'Note', width: { kind: 'fixed', ch: 10 }, tone: 'tertiary', cell: noteCell },
  ]
  const rows: Row[] = [
    { name: 'a.txt', note: 'one' },
    { name: 'b.txt', note: 'two' },
  ]

  it('listbox with a cursor row has no a11y violations', async () => {
    const target = container()
    mount(ColumnList<Row>, { target, props: { columns, rows, ariaLabel: 'Files', cursorIndex: 1 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('table with a group heading and a loading row has no a11y violations', async () => {
    const target = container()
    const all: Row[] = [{ name: '~/Photos', note: '' }, ...rows]
    mount(ColumnList<Row>, {
      target,
      props: {
        columns,
        rows: { count: 4, getRow: (index: number) => all[index] },
        semantics: 'table',
        ariaLabel: 'Renames',
        isGroupHeading: (row: Row) => row.note === '',
        groupHeading: nameCell,
      },
    })
    await tick()
    expect(target.querySelector('.column-list-row.is-placeholder')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier-3 a11y tests for `Chip.svelte`.
 *
 * Covers the filter variant (default, configured, disabled, open), the toggle variant (off and
 * on), the insert variant, and the recent variant (with a leading mode badge). The chip is a
 * single `<button>`; the filter variant carries `aria-haspopup="dialog"` + `aria-expanded`, the
 * toggle `aria-pressed`, and the `×` clear control is decorative (the keyboard path is
 * Backspace), so axe shouldn't flag a nested-interactive pattern.
 */
describe('Chip a11y', () => {
  // The text-face half of Chip's props; the glyph case below mounts its own.
  type Props = ComponentProps<typeof Chip> & { glyph?: false }

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      label: 'Size',
      configured: false,
      isOpen: false,
      onActivate: () => {},
      onClear: () => {},
      ...overrides,
    }
  }

  async function mountAndAudit(props: Props, audit: A11yAuditOptions = {}): Promise<void> {
    const target = container()
    mount(Chip, { target, props })
    await tick()
    await expectNoA11yViolations(target, audit)
    target.remove()
  }

  it('filter default state has no a11y violations', async () => {
    await mountAndAudit(baseProps())
  })

  it('filter configured state (label + value + clear) has no a11y violations', async () => {
    await mountAndAudit(baseProps({ configured: true, value: '> 100 MB' }))
  })

  it('filter open state (aria-expanded=true) has no a11y violations', async () => {
    await mountAndAudit(baseProps({ isOpen: true }))
  })

  it('filter disabled state has no a11y violations', async () => {
    await mountAndAudit(baseProps({ disabled: true }))
  })

  it('toggle variant, off and on (aria-pressed), has no a11y violations', async () => {
    await mountAndAudit(baseProps({ variant: 'toggle', label: 'Aa', ariaLabel: 'Match case', mono: true }))
    await mountAndAudit(
      baseProps({
        variant: 'toggle',
        size: 'field',
        label: '.*',
        ariaLabel: 'Regular expression',
        mono: true,
        pressed: true,
      }),
    )
  })

  // A glyph face paints from `data-glyph`, so "1×" is no visible word the full name lacks (WCAG 2.5.3).
  it('glyph toggle, named in full, has no a11y violations', async () => {
    const target = container()
    mount(Chip, {
      target,
      props: {
        variant: 'toggle',
        mono: true,
        glyph: true,
        label: '1×',
        ariaLabel: 'First match only',
        onActivate: () => {},
      },
    })
    await tick()
    expect(target.querySelector('[data-glyph]')?.getAttribute('data-glyph')).toBe('1×')
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('insert variant has no a11y violations', async () => {
    await mountAndAudit(baseProps({ variant: 'insert', label: '[N]', mono: true }))
  })

  it('recent variant with a leading badge has no a11y violations', async () => {
    await mountAndAudit(
      baseProps({
        variant: 'recent',
        label: '*.log',
        ariaLabel: 'Run recent filename search: *.log',
        // The mode badge is a glyph, painted from `data-glyph`: a text "Aa" would be a visible
        // word the name doesn't hold (WCAG 2.5.3).
        leading: createRawSnippet(() => ({ render: () => '<span data-glyph="Aa"></span>' })),
      }),
    )
  })
})

describe('DateLabel a11y', () => {
  it('with a timestamp has no a11y violations', async () => {
    const target = container()
    mount(DateLabel, { target, props: { modifiedAt: 1710409800 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a null timestamp has no a11y violations', async () => {
    const target = container()
    mount(DateLabel, { target, props: { modifiedAt: null } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `Icon.svelte`.
 *
 * Icon renders an inline glyph from the shared registry. It's decorative unless named: with no
 * `aria-label` / `aria-labelledby` it's `aria-hidden`, and with one it's `role="img"`. axe
 * confirms each shape produces clean markup. Contrast is a wrapper concern (the glyph inherits
 * `currentColor`), so there's none to validate here.
 */
describe('Icon a11y', () => {
  async function renderIcon(props: ComponentProps<typeof Icon>): Promise<HTMLElement> {
    const target = container()
    mount(Icon, { target, props })
    await tick()
    return target
  }

  it('hides an unnamed glyph from assistive tech by default', async () => {
    const svg = (await renderIcon({ name: 'triangle-alert', size: 16 })).querySelector('svg')
    expect(svg?.getAttribute('aria-hidden')).toBe('true')
    expect(svg?.hasAttribute('role')).toBe(false)
  })

  it('exposes a named glyph as an image, and keeps it visible to assistive tech', async () => {
    const target = await renderIcon({ name: 'bot', size: 12, 'aria-label': 'Open the wake thread' })
    const svg = target.querySelector('svg')
    expect(svg?.hasAttribute('aria-hidden')).toBe(false)
    expect(svg?.getAttribute('role')).toBe('img')
    await expectNoA11yViolations(target)
  })

  it('decorative icon (aria-hidden) has no a11y violations', async () => {
    const target = await renderIcon({ name: 'triangle-alert', size: 16, 'aria-hidden': 'true' })
    await expectNoA11yViolations(target)
  })

  it('meaningful icon (role=img + aria-label) has no a11y violations', async () => {
    const target = await renderIcon({
      name: 'hourglass',
      size: 12,
      role: 'img',
      'aria-label': 'Size updating',
    })
    await expectNoA11yViolations(target)
  })

  it('custom glyph (eject) renders and has no a11y violations', async () => {
    const target = await renderIcon({ name: 'eject', size: 14, 'aria-hidden': 'true' })
    await expectNoA11yViolations(target)
  })

  it('every registered glyph renders without throwing', async () => {
    for (const name of Object.keys(ICON_COMPONENTS) as IconName[]) {
      const target = await renderIcon({ name, size: 16, 'aria-hidden': 'true' })
      await expectNoA11yViolations(target)
    }
  })
})

/**
 * Tier 3 a11y tests for `InfoTip.svelte`.
 *
 * The glyph has no visible text, so its `aria-label` is the whole accessible name; axe
 * catches a button that lost one. Both bodies are covered: the plain-text form, and the
 * snippet form, whose markup lives in a `hidden` host until the tooltip adopts it.
 */
describe('InfoTip a11y', () => {
  it('with a plain-text body has no a11y violations', async () => {
    const target = container()
    mount(InfoTip, { target, props: { label: 'More about drive indexing', text: 'Takes about 1 GB.' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a snippet body has no a11y violations', async () => {
    const target = container()
    const children = createRawSnippet(() => ({
      render: () => '<p>Takes about 1 GB.</p><p>You can change this any time in Settings.</p>',
    }))
    mount(InfoTip, { target, props: { label: 'More about drive indexing', children } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `StatusGlyph.svelte`.
 *
 * It's a `role="img"` span with no visible text, so `aria-label` is its whole accessible
 * name and axe catches a labelless one. Both wirings are covered, since the tooltip-less
 * form is the one that could quietly lose its name if the label ever followed the tooltip.
 */
describe('StatusGlyph a11y', () => {
  it('with a tooltip has no a11y violations', async () => {
    const target = container()
    mount(StatusGlyph, { target, props: { name: 'info', label: 'Restricted folder' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('keeps the SHORT label as its accessible name when the tooltip is a paragraph', async () => {
    // The name is read out on every row carrying the marker, so it must not grow into the
    // instruction the tooltip holds.
    const target = container()
    mount(StatusGlyph, {
      target,
      props: { name: 'info', label: 'Restricted folder', tooltip: 'Access to this folder is limited. Grant Cmdr…' },
    })
    await tick()
    expect(target.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Restricted folder')
    await expectNoA11yViolations(target)
  })

  it('without a tooltip keeps its accessible name', async () => {
    const target = container()
    mount(StatusGlyph, { target, props: { name: 'info', label: 'Restricted folder', tooltip: null } })
    await tick()
    expect(target.querySelector('[role="img"]')?.getAttribute('aria-label')).toBe('Restricted folder')
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `LiveAnnouncer.svelte`: a polite live region, mounted empty with its
 * owner (a region inserted with its first message announces nothing). Extra attributes land
 * on the region, which is how a file pane tags its own.
 */
describe('LiveAnnouncer a11y', () => {
  it('mounts an empty polite region with no a11y violations', async () => {
    const target = container()
    mount(LiveAnnouncer, { target, props: { 'data-pane-announcer': true } })
    await tick()
    const region = target.querySelector('[data-live-announcer]')
    expect(region?.getAttribute('aria-live')).toBe('polite')
    expect(region?.hasAttribute('data-pane-announcer')).toBe(true)
    expect(region?.textContent).toBe('')
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `LoadingIcon.svelte`.
 *
 * Covers each of the four progressive-status states (default, opening,
 * loadedCount, finalizingCount) plus the optional cancel hint.
 */
describe('LoadingIcon a11y', () => {
  it('default "Loading…" state has no a11y violations', async () => {
    const target = container()
    mount(LoadingIcon, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('"Opening folder…" state has no a11y violations', async () => {
    const target = container()
    mount(LoadingIcon, { target, props: { openingFolder: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('loadedCount (plural) state has no a11y violations', async () => {
    const target = container()
    mount(LoadingIcon, { target, props: { loadedCount: 1200 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('finalizingCount state has no a11y violations', async () => {
    const target = container()
    mount(LoadingIcon, { target, props: { finalizingCount: 42000 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with showCancelHint has no a11y violations', async () => {
    const target = container()
    mount(LoadingIcon, { target, props: { loadedCount: 500, showCancelHint: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ProgressBar.svelte`.
 *
 * The bar uses `role="progressbar"` with `aria-valuenow/min/max`. These
 * tests check the ARIA wiring at empty, partial, full progress, and with
 * an explicit aria-label.
 */
describe('ProgressBar a11y', () => {
  it('empty progress with ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(ProgressBar, { target, props: { value: 0, ariaLabel: 'Download progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('50% progress with ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(ProgressBar, { target, props: { value: 0.5, ariaLabel: 'Upload progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('complete (100%) with ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(ProgressBar, { target, props: { value: 1, ariaLabel: 'Transfer progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('small size with ariaLabel has no a11y violations', async () => {
    const target = container()
    mount(ProgressBar, { target, props: { value: 0.25, size: 'sm', ariaLabel: 'Indexing progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  // A reader says "42 percent" from `aria-valuenow` alone; the words on screen ("1.2 GB of
  // 3 GB") ride `aria-valuetext`, and the bar names no value text it wasn't given.
  it('says the on-screen amount through aria-valuetext when given one', async () => {
    const target = container()
    mount(ProgressBar, {
      target,
      props: { value: 0.42, ariaLabel: 'Size progress', ariaValuetext: '1.2 GB of 3 GB' },
    })
    await tick()
    const bar = target.querySelector('[role="progressbar"]')
    expect(bar?.getAttribute('aria-valuetext')).toBe('1.2 GB of 3 GB')
    expect(bar?.getAttribute('aria-valuenow')).toBe('42')
    await expectNoA11yViolations(target)

    const plain = container()
    mount(ProgressBar, { target: plain, props: { value: 0.42, ariaLabel: 'Size progress' } })
    await tick()
    expect(plain.querySelector('[role="progressbar"]')?.hasAttribute('aria-valuetext')).toBe(false)
  })
})

describe('SectionCard a11y', () => {
  it('unlabelled card has no a11y violations', async () => {
    const target = container()
    mount(SectionCard, { target, props: { children: snip('Body') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('labelled card has no a11y violations', async () => {
    const target = container()
    mount(SectionCard, { target, props: { label: 'Theme', children: snip('Body') } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('gated card has no a11y violations', async () => {
    const target = container()
    mount(SectionCard, { target, props: { label: 'Downloads notifications', gated: true, children: snip('Body') } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y test for ShortcutChip. Covers both rendered shapes: the non-clickable
 * <kbd> and the clickable <button> (which must carry an accessible name). Mocks the
 * store + bindings the same way the behavior test does so jsdom can render without a
 * live shortcuts store.
 */
describe('ShortcutChip a11y', () => {
  async function mountChip(props: Record<string, unknown>): Promise<HTMLElement> {
    const target = container()
    mount(ShortcutChip, { target, props })
    await tick()
    return target
  }

  it('literal (non-clickable) chip has no a11y violations', async () => {
    const target = await mountChip({ key: '⏎' })
    await expectNoA11yViolations(target)
  })

  it('clickable commandId chip has no a11y violations', async () => {
    const target = await mountChip({ commandId: 'downloads.goToLatest' })
    await expectNoA11yViolations(target)
  })

  it('non-clickable commandId chip has no a11y violations', async () => {
    const target = await mountChip({ commandId: 'downloads.goToLatest', clickable: false })
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `Size.svelte`.
 *
 * Renders the human-friendly byte string in one or more colored spans. There's
 * no interactive surface, ARIA, or labelling to validate; axe just confirms
 * the produced markup has no structural a11y violations. Contrast for the
 * `.size-*` color classes is covered by tier 1 (`scripts/check-a11y-contrast`).
 */
describe('Size a11y', () => {
  it('typical byte count has no a11y violations', async () => {
    const target = container()
    mount(Size, { target, props: { bytes: 1_234_567 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('null bytes (renders fallback) has no a11y violations', async () => {
    const target = container()
    mount(Size, { target, props: { bytes: null, fallback: '—' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `Spinner.svelte`.
 *
 * The shared loading spinner. Decorative by default (`aria-hidden`, for the common case where
 * adjacent text already says "Loading…"); when it's the sole loading signal the caller passes
 * `label`, which becomes an `aria-label` on a `role="status"`. axe confirms both shapes are clean.
 */
describe('Spinner a11y', () => {
  async function renderSpinner(props: ComponentProps<typeof Spinner>): Promise<HTMLElement> {
    const target = container()
    mount(Spinner, { target, props })
    await tick()
    return target
  }

  it('decorative spinner (default) has no a11y violations', async () => {
    for (const size of ['sm', 'md', 'lg'] as const) {
      const target = await renderSpinner({ size })
      await expectNoA11yViolations(target)
    }
  })

  it('labeled spinner (sole loading indicator) has no a11y violations', async () => {
    const target = await renderSpinner({ size: 'sm', label: 'Loading suggestions' })
    await expectNoA11yViolations(target)
  })
})

describe('StatusBadge a11y', () => {
  it('alpha badge has no a11y violations', async () => {
    const target = container()
    mount(StatusBadge, { target, props: { status: 'alpha' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('beta badge has no a11y violations', async () => {
    const target = container()
    mount(StatusBadge, { target, props: { status: 'beta' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
