/**
 * Tests for the sort-shortcut tooltip on column headers.
 *
 * The tooltip must be truthful: the sort shortcut acts on the focused pane only,
 * so it appears only when the header's pane is focused, and it must track both
 * focus flips and shortcut rebinds live (the binding is user-customizable).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

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

vi.mock('$lib/ipc/bindings', () => ({
  commands: { updateMenuAccelerator: vi.fn(() => Promise.resolve({ status: 'ok' })) },
}))

import SortableHeader from './SortableHeader.svelte'
import { setShortcut, resetShortcut } from '$lib/shortcuts/shortcuts-store'
import type { SortColumn } from '../types'

const TOOLTIP_SHOW_DELAY_MS = 400

interface HeaderProps {
  column: SortColumn
  label: string
  currentSortColumn: SortColumn
  currentSortOrder: 'ascending' | 'descending'
  onClick: (column: SortColumn) => void
  isFocused: boolean
}

function setup(overrides: Partial<HeaderProps> = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const props: HeaderProps = $state({
    column: 'size',
    label: 'Size',
    currentSortColumn: 'name',
    currentSortOrder: 'ascending',
    onClick: vi.fn(),
    isFocused: true,
    ...overrides,
  })
  const component = mount(SortableHeader, { target, props })
  flushSync() // Run the effect phase so the `use:tooltip` action attaches its listeners
  const button = target.querySelector('button')
  if (!button) throw new Error('SortableHeader did not render a button')
  return { target, props, component, button }
}

function hover(button: HTMLElement): void {
  button.dispatchEvent(new MouseEvent('mouseenter'))
  vi.advanceTimersByTime(TOOLTIP_SHOW_DELAY_MS + 10)
}

function tooltipEl(): HTMLElement | null {
  return document.querySelector('.cmdr-tooltip')
}

function tooltipKbd(): HTMLElement | null {
  return tooltipEl()?.querySelector('kbd.cmdr-tooltip-kbd') ?? null
}

describe('SortableHeader sort-shortcut tooltip', () => {
  let cleanups: (() => void)[] = []

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    for (const cleanup of cleanups) cleanup()
    cleanups = []
    resetShortcut('sort.bySize')
    vi.useRealTimers()
  })

  function setupTracked(overrides: Partial<HeaderProps> = {}) {
    const result = setup(overrides)
    cleanups.push(() => {
      result.button.dispatchEvent(new MouseEvent('mouseleave'))
      void unmount(result.component)
      result.target.remove()
    })
    return result
  }

  it('shows the command name and the shortcut when the pane is focused', () => {
    const { button } = setupTracked()
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    // The default binding is ⌘6 (Ctrl+6 off-macOS); both contain '6'
    expect(tooltipKbd()?.textContent).toContain('6')
  })

  it('omits the shortcut when the pane is not focused (it would sort the other pane)', () => {
    const { button } = setupTracked({ isFocused: false })
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    expect(tooltipKbd()).toBeNull()
  })

  it('removes the shortcut live when focus flips away mid-hover', () => {
    const { button, props } = setupTracked()
    hover(button)
    expect(tooltipKbd()).not.toBeNull()

    props.isFocused = false
    flushSync()

    expect(tooltipEl()?.textContent).toContain('Sort by size')
    expect(tooltipKbd()).toBeNull()
  })

  it('updates the shown shortcut live when the user rebinds it', () => {
    const { button } = setupTracked()
    hover(button)
    expect(tooltipKbd()?.textContent).toContain('6')

    setShortcut('sort.bySize', 0, '⌘9')
    flushSync()

    expect(tooltipKbd()?.textContent).toBe('⌘9')
  })

  it('shows no shortcut for a column whose sort command has no binding', () => {
    const { button } = setupTracked({ column: 'created', label: 'Created' })
    hover(button)

    expect(tooltipEl()?.textContent).toContain('Sort by date created')
    expect(tooltipKbd()).toBeNull()
  })
})
