/**
 * What a screen reader gets from the Settings sidebar: the section cursor named through
 * `aria-activedescendant` (on the list and on the search field, which moves it with ↑/↓),
 * subsections grouped under their section, and the search's match count said aloud.
 *
 * ❗ The active descendant lands one frame after focus (`#lib/ui/deferred-active-descendant`),
 * so each check waits past that frame before asserting.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import { computeAccessibleName } from 'dom-accessibility-api'
import { assertFocusModel } from '#lib/test-focus-model.js'
import { eventually, untilFocusNamesARow } from '#lib/test-frames.js'
import SettingsSidebar from './SettingsSidebar.svelte'

let component: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
})

function setup(initial: { searchQuery?: string; matchingSections?: string[]; selectedSection?: string[] } = {}) {
  const props = $state({
    searchQuery: initial.searchQuery ?? '',
    matchingSections: new Set(initial.matchingSections ?? []),
    selectedSection: initial.selectedSection ?? ['Appearance', 'Colors and formats'],
  })
  const target = document.createElement('div')
  document.body.appendChild(target)
  component = mount(SettingsSidebar, {
    target,
    props: {
      get searchQuery() {
        return props.searchQuery
      },
      get matchingSections() {
        return props.matchingSections
      },
      get selectedSection() {
        return props.selectedSection
      },
      onSearch: () => {},
      onSectionSelect: (path: string[]) => {
        props.selectedSection = path
      },
    },
  })
  flushSync()
  const listbox = target.querySelector<HTMLElement>('[role="listbox"]')
  const search = target.querySelector<HTMLInputElement>('input')
  if (!listbox || !search) throw new Error('SettingsSidebar rendered no listbox or search field')
  return { target, props, listbox, search }
}

function activeName(el: HTMLElement): string {
  const id = el.getAttribute('aria-activedescendant')
  const option = id ? document.getElementById(id) : null
  return option ? computeAccessibleName(option) : ''
}

function key(el: HTMLElement, name: string): void {
  el.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }))
  flushSync()
}

describe('SettingsSidebar: the section cursor', () => {
  it('names the selected section through the focused list, and follows ↓', async () => {
    const { listbox } = setup()
    listbox.focus()
    await untilFocusNamesARow()
    assertFocusModel(listbox)
    expect(activeName(listbox)).toBe('Colors and formats')

    key(listbox, 'ArrowDown')
    await tick()
    assertFocusModel(listbox)
    expect(activeName(listbox)).not.toBe('Colors and formats')
  })

  it('names nothing until a frame after focus, so VoiceOver reads the row and not just the list', async () => {
    const { listbox } = setup()
    listbox.focus()
    flushSync()
    expect(listbox.hasAttribute('aria-activedescendant')).toBe(false)
    await untilFocusNamesARow()
    expect(document.activeElement).toBe(listbox)
  })

  it('makes the search field a combobox over the same list, since ↑/↓ there move the section', async () => {
    const { search, listbox } = setup()
    expect(search.getAttribute('role')).toBe('combobox')
    expect(search.getAttribute('aria-controls')).toBe(listbox.id)
    search.focus()
    await untilFocusNamesARow()
    // The option lives in the controlled list, not inside the field, so `assertFocusModel`
    // (which wants it inside or owned) doesn't apply; the name check covers the same ground.
    expect(activeName(search)).toBe('Colors and formats')
    key(search, 'ArrowDown')
    await tick()
    expect(activeName(search)).not.toBe('Colors and formats')
  })

  it('groups each section’s subsections under its name', () => {
    const { listbox } = setup()
    const option = [...listbox.querySelectorAll('[role="option"]')].find(
      (el) => el.textContent.trim() === 'Colors and formats',
    )
    const group = option?.closest('[role="group"]')
    expect(group).not.toBeNull()
    expect(computeAccessibleName(group as Element)).toBe('Appearance')
    // A top-level section is no group's member.
    const top = [...listbox.querySelectorAll('[role="option"]')].find((el) => el.textContent.trim() === 'Appearance')
    expect(top?.closest('[role="group"]')).toBeNull()
  })

  // Regression: VoiceOver read "(2 of 15)" on the first row it landed on (its own count) and
  // then the name alone for every row after it (listening run, 2026-10-15). Every option
  // carries its place in the list ↓ walks, subsections included.
  it('gives every section its position in the walk, renumbered when a search narrows it', () => {
    const { listbox, props } = setup()
    const positions = () =>
      [...listbox.querySelectorAll('[role="option"]')].map(
        (el) => `${el.getAttribute('aria-posinset') ?? '?'}/${el.getAttribute('aria-setsize') ?? '?'}`,
      )
    const all = positions()
    expect(all.length).toBeGreaterThan(2)
    expect(all).toEqual(all.map((_, i) => `${String(i + 1)}/${String(all.length)}`))

    props.searchQuery = 'theme'
    props.matchingSections = new Set(['Appearance', 'Appearance/Colors and formats'])
    flushSync()
    expect(positions()).toEqual(['1/2', '2/2'])
  })
})

describe('SettingsSidebar: search results', () => {
  it('says how many sections match, and when none do', async () => {
    const { target, props } = setup()
    const region = target.querySelector('[data-live-announcer]')
    expect(region?.textContent).toBe('')

    props.searchQuery = 'theme'
    props.matchingSections = new Set(['Appearance', 'Appearance/Colors and formats'])
    flushSync()
    await eventually(() => {
      expect(region?.textContent).toBe('2 sections match')
    })

    props.searchQuery = 'zzznonexistent'
    props.matchingSections = new Set()
    flushSync()
    await eventually(() => {
      expect(region?.textContent).toBe('No settings match')
    })
  })
})
