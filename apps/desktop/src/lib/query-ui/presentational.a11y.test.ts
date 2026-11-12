/**
 * Tier 3 a11y tests for the query-ui pieces that mock nothing: the AI transparency
 * strip, the empty state, the mode chips, the path pills, and the search bar.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions; `Props` and `baseProps` stay inside their block because four of them
 * define a different one.
 *
 * `QueryDialog` and `QueryResults` stay in `dialog.a11y.test.ts`: their
 * `#lib/tauri-commands`, `#lib/settings`, and `#lib/icon-cache` stubs would apply
 * file-wide, and the components here use all three for real.
 */

import { describe, it, afterEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import AiTransparencyStrip from './AiPromptStrip.svelte'
import EmptyState from './EmptyState.svelte'
import SearchModeChips from './ModeChips.svelte'
import PathPills from './PathPills.svelte'
import SearchBar from './QueryBar.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

// These components share one jsdom document, and axe resolves ARIA id references
// document-wide. Clearing between tests keeps each audit looking at its own
// container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier-3 a11y tests for `AiTransparencyStrip.svelte`.
 *
 * Pins that the strip is axe-clean with and without a caveat, and that the disabled "Refine…"
 * button doesn't trip nested-interactive or hidden-content rules.
 */
describe('AiTransparencyStrip a11y', () => {
  type Props = ComponentProps<typeof AiTransparencyStrip>

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      aiPrompt: 'screenshots from this week',
      caveat: '',
      summary: { pattern: null, patternKind: null, filters: [] },
      ...overrides,
    }
  }

  it('has no a11y violations with a prompt only', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiTransparencyStrip, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('has no a11y violations with a full summary, filters, and a caveat', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiTransparencyStrip, {
      target,
      props: baseProps({
        caveat: "I treated 'big' as larger than 10 MB.",
        summary: {
          pattern: '*.{jpg,png,heic}',
          patternKind: 'glob',
          filters: [
            { label: 'Size', value: '> 10 MB' },
            { label: 'Type', value: 'Files only' },
          ],
        },
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier-3 a11y tests for `EmptyState.svelte`.
 *
 * The empty state surfaces a "Try…" line, three example chips (AI prompts or
 * filename patterns depending on `aiEnabled`), an index-size status line, and
 * a keyboard-shortcut tip. Covered variants: AI-on and AI-off chip sets.
 */
describe('EmptyState a11y', () => {
  type Props = ComponentProps<typeof EmptyState>

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      aiEnabled: true,
      indexEntryCount: 10_123_456,
      onPick: () => {},
      ...overrides,
    }
  }

  it('AI-on variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI-off variant has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps({ aiEnabled: false }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('zero-entry index has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(EmptyState, { target, props: baseProps({ indexEntryCount: 0 }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier-3 a11y tests for `SearchModeChips.svelte`.
 *
 * Covers AI-on (four chips) and AI-off (three chips) states, plus the disabled state. The Content
 * chip is visible-disabled, so its disabled-but-described pattern lives in every case.
 */
describe('SearchModeChips a11y', () => {
  type Props = ComponentProps<typeof SearchModeChips>

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      mode: 'filename',
      aiEnabled: true,
      disabled: false,
      onSelect: () => {},
      ...overrides,
    }
  }

  it('AI-on (four chips) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI-off (three chips) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ aiEnabled: false, mode: 'filename' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI mode active has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ mode: 'ai' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y test for `PathPills.svelte`.
 *
 * The strip is mouse-only and hidden from assistive technology; the path stays
 * readable as `sr-only` text. `PathPills.svelte.test.ts` pins the shape, and
 * `dialog.a11y.test.ts` audits it inside a real `role="option"` row.
 */
describe('PathPills a11y', () => {
  it('renders without axe-core violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PathPills, {
      target,
      props: { path: '/Users/test_user_10/code', onPick: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier-3 a11y tests for `SearchBar.svelte`.
 *
 * The bar's a11y surface: the house `TextInput` pill (per-mode `aria-label`, decorative
 * magnifier), the dropdown-trigger chevron, and the run `Button`. Covered states: each mode
 * plus the disabled state.
 */
describe('SearchBar a11y', () => {
  type Props = ComponentProps<typeof SearchBar>

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      inputElement: undefined,
      query: '',
      mode: 'filename',
      disabled: false,
      aiHighlight: false,
      showRunHint: false,
      runHintCopy: 'Press Enter to search',
      recentOpen: false,
      listboxId: 'results',
      listExpanded: false,
      onInput: () => {},
      onRun: () => {},
      onToggleRecent: () => {},
      recentTriggerLabel: 'All recent searches',
      recentTriggerTooltip: 'Show all recent searches',
      onCompositionStart: () => {},
      onCompositionEnd: () => {},
      ...overrides,
    }
  }

  it('filename mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ mode: 'ai' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('regex mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ mode: 'regex' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
