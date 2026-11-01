// @vitest-environment jsdom
/**
 * Speech-level expectations for the house menu, through `@guidepup/virtual-screen-reader`.
 *
 * It models the ARIA spec over jsdom, not WebKit or VoiceOver, so it guards OUR markup: the
 * focused surface's role and name, the row its `aria-activedescendant` names as the keys walk,
 * the submenu speaking as its own menu named after its row, and focus coming home on close.
 * What VoiceOver really says is recorded in `lib/ui/DETAILS.md` § Menu.
 *
 * jsdom, not the suite's happy-dom: the virtual reader drives keys through
 * `@testing-library/user-event` and reads names with `dom-accessibility-api`, both written
 * against jsdom.
 *
 * Two of its limits shape the assertions. It reads the active descendant as "active
 * descendant {name}" on the focused element's phrase, without the row's role or state, so
 * those come from walking the open menu with the reader's cursor (VO-→) at the end. And it
 * speaks on focus and after a key, not when an attribute changes on its own, so the
 * one-frame-late active descendant on open shows up at the first key.
 */

import { describe, it, expect, beforeAll, afterEach } from 'vitest'
import { mount, unmount } from 'svelte'
import { virtual } from '@guidepup/virtual-screen-reader'
import Menu from './Menu.svelte'
import { createMenu, type MenuController } from './menu-controller.svelte'
import type { MenuSection } from './menu-types'
import { eventually, untilFocusNamesARow } from '#lib/test-frames.js'
import { pressAndHear } from '#lib/test-vsr.js'

function sections(): MenuSection[] {
  return [
    {
      id: 'volumes',
      heading: 'Volumes',
      items: [
        { value: 'hd', label: 'Macintosh HD', check: { kind: 'current' } },
        {
          value: 'share',
          label: 'Team share',
          submenu: [
            { value: 'eject', label: 'Eject' },
            { value: 'fast', label: 'Use the fast connection', check: { kind: 'toggle', checked: true } },
            { value: 'wake', label: 'Reconnect automatically', check: { kind: 'toggle', checked: false } },
          ],
        },
      ],
    },
  ]
}

let cleanup: (() => void) | null = null

beforeAll(() => {
  // jsdom has no layout, so no `scrollIntoView`; the menu keeps the cursor row on screen with it.
  Element.prototype.scrollIntoView = () => {}
})

afterEach(async () => {
  cleanup?.()
  cleanup = null
  await virtual.stop()
  document.body.innerHTML = ''
})

/** Past focus effects, the portal's tick, and the one-frame-late active descendant, on the surface named `label`. */
async function untilMenuHoldsFocus(label: string): Promise<void> {
  await eventually(() => {
    expect(document.activeElement?.getAttribute('aria-label')).toBe(label)
  })
  await untilFocusNamesARow()
}

/** A trigger the way the volume chip is one, which focus returns to on close. */
async function openMenu(): Promise<{ menu: MenuController }> {
  const trigger = document.createElement('button')
  trigger.type = 'button'
  trigger.textContent = 'Macintosh HD'
  trigger.setAttribute('aria-haspopup', 'menu')
  document.body.appendChild(trigger)
  trigger.focus()

  await virtual.start({ container: document.body })
  const target = document.createElement('div')
  document.body.appendChild(target)
  const menu = createMenu({
    getSections: sections,
    onSelect: () => {},
    restoreFocus: () => {
      trigger.focus()
    },
  })
  menu.openUnder(trigger)
  const component = mount(Menu, { target, props: { menu, ariaLabel: 'Volume switcher' } })
  cleanup = () => {
    menu.destroy()
    void unmount(component)
  }
  await untilMenuHoldsFocus('Volume switcher')
  return { menu }
}

/** Walks onto `Team share` and into its submenu. */
async function enterSubmenu(): Promise<void> {
  await pressAndHear('ArrowDown', 'menu, Volume switcher, orientated vertically, active descendant Team share')
  await virtual.press('ArrowRight')
  await untilMenuHoldsFocus('Team share')
}

describe('the house menu, spoken', () => {
  it('opens as a named menu, and each ↓ names the row the cursor lands on', async () => {
    await openMenu()
    expect(await virtual.lastSpokenPhrase()).toBe('menu, Volume switcher, orientated vertically')

    await pressAndHear('ArrowDown', 'menu, Volume switcher, orientated vertically, active descendant Team share')
    await pressAndHear('ArrowUp', 'menu, Volume switcher, orientated vertically, active descendant Macintosh HD')
  })

  it('→ speaks the submenu as its own menu named after its row, and ↓ walks its rows', async () => {
    await openMenu()
    await pressAndHear('ArrowDown', 'menu, Volume switcher, orientated vertically, active descendant Team share')

    // Entering: focus moves to the submenu, whose own row lands a frame later. Asserted on the
    // phrase spoken AT focus: the reader re-reads after the key too, and whether the frame has
    // landed by then is a race a loaded CI runner loses.
    const spokenBefore = (await virtual.spokenPhraseLog()).length
    await virtual.press('ArrowRight')
    await untilMenuHoldsFocus('Team share')
    expect((await virtual.spokenPhraseLog())[spokenBefore]).toBe('menu, Team share, orientated vertically')
    await pressAndHear(
      'ArrowDown',
      'menu, Team share, orientated vertically, active descendant Use the fast connection',
    )
    await pressAndHear(
      'ArrowDown',
      'menu, Team share, orientated vertically, active descendant Reconnect automatically',
    )

    // Leaving: back on the parent surface, on the row the submenu hangs off.
    await pressAndHear('ArrowLeft', 'menu, Volume switcher, orientated vertically, active descendant Team share')
  })

  it('Escape backs out of the submenu, then closes the menu with focus back on its trigger', async () => {
    await openMenu()
    await enterSubmenu()

    await pressAndHear('Escape', 'menu, Volume switcher, orientated vertically, active descendant Team share')
    await pressAndHear('Escape', 'button, Macintosh HD, has popup menu')
    await eventually(() => {
      expect(document.querySelector('[data-menu]')).toBeNull()
    })
  })

  // The active descendant phrase carries only a name, so the rows' roles and states are heard
  // the way a VoiceOver user browses an open menu: moving the reader's own cursor over them.
  it('says each row’s role and state: current, toggles on and off, and the submenu parent', async () => {
    await openMenu()
    await enterSubmenu()

    const heard: string[] = []
    for (let i = 0; i < 16; i++) {
      await virtual.next()
      heard.push(await virtual.lastSpokenPhrase())
    }
    const said = (start: string): boolean => heard.some((phrase) => phrase.startsWith(start))
    expect(said('menuitem, Macintosh HD, current location')).toBe(true)
    // "1 control" is the row's `aria-controls`, pointing at the open submenu.
    expect(said('menuitem, Team share, 1 control, expanded, has popup menu')).toBe(true)
    expect(said('menuitemcheckbox, Use the fast connection, checked')).toBe(true)
    expect(said('menuitemcheckbox, Reconnect automatically, not checked')).toBe(true)
  })
})
