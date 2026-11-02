// @vitest-environment jsdom
/**
 * Speech-level expectations for the Settings sidebar, through `@guidepup/virtual-screen-reader`.
 *
 * It models the ARIA spec over jsdom, not WebKit or VoiceOver, so it guards OUR markup: ↓ in
 * the section list and in the search field each speak the selected section through
 * `aria-activedescendant`, and a search's match count arrives through the live region. What
 * VoiceOver really says is a listening check (V7 in David's guide).
 *
 * The reader reads the active descendant as "active descendant {name}" on the focused
 * element's phrase, and speaks after a key, not when an attribute changes on its own, so the
 * one-frame-late id shows up at the first key.
 */

import { describe, it, afterEach } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { virtual } from '@guidepup/virtual-screen-reader'
import SettingsSidebar from './SettingsSidebar.svelte'
import { untilFocusNamesARow } from '#lib/test-frames.js'
import { pressAndHear, untilSpoken } from '#lib/test-vsr.js'

let component: ReturnType<typeof mount> | null = null

afterEach(async () => {
  if (component) void unmount(component)
  component = null
  await virtual.stop()
  document.body.innerHTML = ''
})

async function setup() {
  const props = $state({
    searchQuery: '',
    matchingSections: new Set<string>(),
    selectedSection: ['Appearance', 'Colors and formats'],
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
  await virtual.start({ container: document.body })
  return { target, props }
}

describe('SettingsSidebar speech', () => {
  it('speaks the next section as ↓ moves through the list', async () => {
    const { target } = await setup()
    target.querySelector<HTMLElement>('[role="listbox"]')?.focus()
    await untilFocusNamesARow()
    await pressAndHear(
      'ArrowDown',
      'listbox, Settings sections, orientated vertically, active descendant Zoom and density',
    )
  })

  it('speaks the section from the search field too, which moves the same cursor', async () => {
    const { target } = await setup()
    target.querySelector<HTMLElement>('input')?.focus()
    await untilFocusNamesARow()
    await pressAndHear(
      'ArrowDown',
      'combobox, Search settings…, has popup listbox, expanded, active descendant Zoom and density, autocomplete in list, 1 control',
    )
  })

  it('says how many sections a search left', async () => {
    const { props } = await setup()
    props.searchQuery = 'theme'
    props.matchingSections = new Set(['Appearance', 'Appearance/Colors and formats'])
    flushSync()
    await untilSpoken('polite: 2 sections match')
  })
})
