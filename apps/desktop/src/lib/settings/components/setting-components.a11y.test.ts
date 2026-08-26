/**
 * Tier 3 a11y tests for the settings row primitives and the settings shell.
 *
 * One file per component would cost about fourteen times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The mock surface here is mutable rather than merged. Every block stubbed
 * `getSetting` / `getSettingDefinition` differently, and merging them into one
 * value would change what each component renders, so each block installs its
 * own implementation in its own `beforeEach`. `null` means "use the real
 * export", which is what the blocks that never stubbed a given module had.
 *
 * `$lib/settings` re-exports from `$lib/settings/settings-store`, and the source
 * files were split over which of the two they stubbed. Both route to the same
 * stubs here, so a component sees the same value whichever import path it uses.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const stubs = vi.hoisted(() => ({
  getSetting: (_id: string): unknown => undefined,
  settingDefinition: (_id: string): unknown => undefined,
  defaultValue: (_id: string): unknown => undefined,
  setSetting: vi.fn<(id: string, value: unknown) => unknown>(),
  // `null` keeps the real search helpers, which is what every block but
  // `SettingRow` had.
  matchIndices: null as ((payload: { query: string; settingId: string }) => number[]) | null,
  highlight: null as ((label: string, indices: number[]) => unknown) | null,
}))

const settingsApi = vi.hoisted(() => ({
  getSetting: (id: string) => stubs.getSetting(id),
  setSetting: (id: string, value: unknown) => stubs.setSetting(id, value),
  getSettingDefinition: (id: string) => stubs.settingDefinition(id),
  getDefaultValue: (id: string) => stubs.defaultValue(id),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
  // `SettingNumberInput` drives a plain-number setting, so the duration helpers
  // are passthroughs (factor 1).
  durationUnitFactor: vi.fn(() => 1),
  msToDurationValue: vi.fn((ms: number) => ms),
  durationValueToMs: vi.fn((value: number) => value),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...settingsApi,
}))

vi.mock('$lib/settings/settings-store', () => settingsApi)

vi.mock('$lib/settings/settings-search', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  // eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real (unconverted) production signature, `(query, settingId) => number[]` in `settings-search.ts`, consumed positionally by `SettingRow.svelte` and `AdvancedSection.svelte`
  const realIndices = actual.getMatchIndicesForLabel as (query: string, settingId: string) => number[]
  const realHighlight = actual.highlightMatches as (label: string, indices: number[]) => unknown
  return {
    ...actual,
    getMatchIndicesForLabel: (query: string, settingId: string) =>
      stubs.matchIndices ? stubs.matchIndices({ query, settingId }) : realIndices(query, settingId),
    highlightMatches: (label: string, indices: number[]) =>
      stubs.highlight ? stubs.highlight(label, indices) : realHighlight(label, indices),
  }
})

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openAppearanceSettings: vi.fn(() => Promise.resolve()),
  invoke: vi.fn(() => Promise.resolve(null)),
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

import SectionSummary from './SectionSummary.svelte'
import SettingCheckbox from './SettingCheckbox.svelte'
import SettingColorSwatchPicker from './SettingColorSwatchPicker.svelte'
import SettingNumberInput from './SettingNumberInput.svelte'
import SettingPasswordInput from './SettingPasswordInput.svelte'
import SettingRadioGroup from './SettingRadioGroup.svelte'
import SettingRow from './SettingRow.svelte'
import SettingSelect from './SettingSelect.svelte'
import SettingSlider from './SettingSlider.svelte'
import SettingSwitch from './SettingSwitch.svelte'
import SettingToggleGroup from './SettingToggleGroup.svelte'
import SettingsContent from './SettingsContent.svelte'
import SettingsSection from './SettingsSection.svelte'
import SettingsSidebar from './SettingsSidebar.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  stubs.getSetting = () => undefined
  stubs.settingDefinition = () => undefined
  stubs.defaultValue = () => undefined
  stubs.matchIndices = null
  stubs.highlight = null
})

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide (setting rows label their control by id). Clearing between tests
// keeps each audit to its own container.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `SectionSummary.svelte`.
 *
 * Grid of subsection cards shown for top-level sections.
 */
describe('SectionSummary a11y', () => {
  it('Appearance section (multiple subsections) has no a11y violations', async () => {
    const target = container()
    mount(SectionSummary, {
      target,
      props: { sectionName: 'Appearance', onNavigate: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('unknown section (no subsections) has no a11y violations', async () => {
    const target = container()
    mount(SectionSummary, {
      target,
      props: { sectionName: 'NonexistentSection', onNavigate: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingCheckbox.svelte`. */
describe('SettingCheckbox a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => false
    stubs.settingDefinition = () => ({ label: 'Warn on size mismatch', description: '' })
  })

  it('default (unchecked) has no a11y violations', async () => {
    const target = container()
    mount(SettingCheckbox, { target, props: { id: 'listing.sizeMismatchWarning' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingCheckbox, { target, props: { id: 'listing.sizeMismatchWarning', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingColorSwatchPicker.svelte`.
 *
 * Covers the trigger button (closed) and the open popover with the swatch
 * grid. Contrast on tinted backgrounds is checked at design time by
 * `scripts/check-a11y-contrast` (tier 1).
 */
describe('SettingColorSwatchPicker a11y', () => {
  let currentValue = 'none'

  beforeEach(() => {
    currentValue = 'none'
    stubs.getSetting = () => currentValue
    stubs.setSetting.mockImplementation((_id: string, v: unknown) => {
      currentValue = v as string
      return Promise.resolve()
    })
  })

  afterEach(() => {
    stubs.setSetting.mockReset()
  })

  it('default (closed, no tint) has no a11y violations', async () => {
    currentValue = 'none'
    const target = container()
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed with a selected color has no a11y violations', async () => {
    currentValue = 'blue'
    const target = container()
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('open popover (with swatch grid) has no a11y violations', async () => {
    currentValue = 'none'
    const target = container()
    mount(SettingColorSwatchPicker, {
      target,
      props: { id: 'appearance.tintLocal', label: 'Tint local-volume panes' },
    })
    await tick()
    // Open the popover via the trigger
    target.querySelector<HTMLButtonElement>('button.trigger')?.click()
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingNumberInput.svelte`. */
describe('SettingNumberInput a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 200
    stubs.settingDefinition = () => ({
      label: 'Max conflicts to show',
      description: '',
      constraints: { min: 10, max: 1000, step: 10 },
    })
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with unit label has no a11y violations', async () => {
    const target = container()
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow', unit: 'files' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingNumberInput, { target, props: { id: 'fileOperations.maxConflictsToShow', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingPasswordInput.svelte`.
 *
 * Masked password input with a reveal button. Tests cover empty,
 * pre-filled (masked), and controlled-mode (external value + onchange)
 * variants.
 */
describe('SettingPasswordInput a11y', () => {
  let stored = ''

  beforeEach(() => {
    stored = ''
    stubs.getSetting = () => stored
    stubs.setSetting.mockImplementation((_id: string, value: unknown) => {
      stored = value as string
    })
  })

  afterEach(() => {
    stubs.setSetting.mockReset()
  })

  it('empty (uncontrolled) has no a11y violations', async () => {
    stored = ''
    const target = container()
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.cloudProvider',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('pre-filled (masked) has no a11y violations', async () => {
    stored = 'sk-abcdef1234567890'
    const target = container()
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.cloudProvider',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('controlled mode (external value + onchange) has no a11y violations', async () => {
    const target = container()
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.cloudProvider',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
        value: 'sk-12345',
        onchange: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    stored = ''
    const target = container()
    mount(SettingPasswordInput, {
      target,
      props: {
        id: 'ai.cloudProvider',
        placeholder: 'sk-...',
        ariaLabel: 'API key',
        disabled: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingRadioGroup.svelte`. */
describe('SettingRadioGroup a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 'iso'
    stubs.settingDefinition = () => ({
      label: 'Date/time format',
      description: '',
      constraints: {
        options: [
          { value: 'iso', label: 'ISO 8601', description: '2025-04-16 10:30' },
          { value: 'us', label: 'US', description: '4/16/2025 10:30 AM' },
          { value: 'custom', label: 'Custom', description: 'Define your own format' },
        ],
      },
    })
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(SettingRadioGroup, { target, props: { id: 'appearance.dateTimeFormat' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingRadioGroup, { target, props: { id: 'appearance.dateTimeFormat', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingRow.svelte`.
 *
 * Wrapper that renders a label, description, and a slot for a control.
 * Tests mount the row with a plain `<input>` as the child so axe can
 * check label-control association.
 */
describe('SettingRow a11y', () => {
  const controlSnippet = createRawSnippet(() => ({
    render: () => `<input id="appearance.uiDensity" type="text" aria-label="control" />`,
  }))

  beforeEach(() => {
    stubs.matchIndices = () => []
    stubs.highlight = (label: string) => [{ text: label, matched: false }]
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('split layout has no a11y violations', async () => {
    const target = container()
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        split: true,
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled + requires-restart has no a11y violations', async () => {
    const target = container()
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        disabled: true,
        disabledReason: 'Preview only',
        requiresRestart: true,
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingSelect.svelte`.
 *
 * Covers the closed default. Open-dropdown state is driven by Ark UI
 * state machines we don't exercise here; axe against the closed state
 * is what tier 3 needs to catch trigger-label regressions.
 */
describe('SettingSelect a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 'auto'
    stubs.settingDefinition = () => ({
      label: 'File size format',
      description: '',
      constraints: {
        options: [
          { value: 'auto', label: 'Auto' },
          { value: 'binary', label: 'Binary (KiB, MiB)' },
          { value: 'decimal', label: 'Decimal (KB, MB)' },
        ],
      },
    })
  })

  it('closed (default value) has no a11y violations', async () => {
    const target = container()
    mount(SettingSelect, { target, props: { id: 'appearance.fileSizeFormat' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingSelect, { target, props: { id: 'appearance.fileSizeFormat', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingSlider.svelte`. */
describe('SettingSlider a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 50
    stubs.defaultValue = () => 50
    stubs.settingDefinition = () => ({
      label: 'Progress update interval',
      description: '',
      constraints: { min: 0, max: 100, step: 10, sliderStops: [0, 25, 50, 75, 100] },
    })
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with unit label has no a11y violations', async () => {
    const target = container()
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval', unit: 'ms' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingSlider, { target, props: { id: 'fileOperations.progressUpdateInterval', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingSwitch.svelte`. */
describe('SettingSwitch a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => false
    stubs.settingDefinition = () => ({ label: 'Striped rows', description: '' })
  })

  it('default (unchecked) has no a11y violations', async () => {
    const target = container()
    mount(SettingSwitch, { target, props: { id: 'listing.stripedRows' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingSwitch, { target, props: { id: 'listing.stripedRows', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `SettingToggleGroup.svelte`. */
describe('SettingToggleGroup a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 'comfortable'
    stubs.settingDefinition = () => ({
      label: 'UI density',
      description: '',
      constraints: {
        options: [
          { value: 'compact', label: 'Compact' },
          { value: 'comfortable', label: 'Comfortable' },
          { value: 'spacious', label: 'Spacious' },
        ],
      },
    })
  })

  it('default has no a11y violations', async () => {
    const target = container()
    mount(SettingToggleGroup, { target, props: { id: 'appearance.uiDensity' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = container()
    mount(SettingToggleGroup, { target, props: { id: 'appearance.uiDensity', disabled: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingsContent.svelte`.
 *
 * Dispatcher that picks which section component to render based on the
 * selectedSection path and searchQuery. Tests cover a few representative
 * paths. Child sections pull heavy state, so we rely on the file-level
 * settings stubs above.
 */
describe('SettingsContent a11y', () => {
  it('Appearance summary page has no a11y violations', async () => {
    const target = container()
    mount(SettingsContent, {
      target,
      props: {
        searchQuery: '',
        selectedSection: ['Appearance'],
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingsSection.svelte`.
 *
 * Thin wrapper: `<h2>` section title + children slot.
 */
describe('SettingsSection a11y', () => {
  const bodySnippet = createRawSnippet(() => ({
    render: () => `<div><p>Section content goes here.</p></div>`,
  }))

  it('default render has no a11y violations', async () => {
    const target = container()
    mount(SettingsSection, { target, props: { title: 'Appearance', children: bodySnippet } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SettingsSidebar.svelte`.
 *
 * The sidebar is a listbox with one or more sections + a search input.
 * These tests check the ARIA structure (role, aria-selected, aria-label
 * on the listbox and the search-clear button) across:
 *   - default (no search, first section selected)
 *   - with an active search query (clear button visible)
 *   - with a subsection selected
 *
 * The real settings tree is imported; we aren't mocking the registry
 * because it's a pure module with no IO.
 */
describe('SettingsSidebar a11y', () => {
  it('default render (first section selected) has no violations', async () => {
    const target = container()
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: '',
        matchingSections: new Set<string>(),
        selectedSection: ['Appearance', 'Colors and formats'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with search query + clear button visible has no violations', async () => {
    const target = container()
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: 'theme',
        matchingSections: new Set<string>(['Appearance', 'Appearance/Colors and formats']),
        selectedSection: ['Appearance', 'Colors and formats'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with a subsection selected has no violations', async () => {
    const target = container()
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: '',
        matchingSections: new Set<string>(),
        selectedSection: ['Appearance', 'Listing'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with empty search results (no matches) has no violations', async () => {
    const target = container()
    mount(SettingsSidebar, {
      target,
      props: {
        searchQuery: 'zzznonexistent',
        matchingSections: new Set<string>(),
        selectedSection: ['Appearance', 'Colors and formats'],
        onSearch: () => {},
        onSectionSelect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
