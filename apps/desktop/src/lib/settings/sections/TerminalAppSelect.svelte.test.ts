/**
 * Behavior tests for the "Open terminal here uses" control.
 *
 * What matters here is that the row tells the truth about this Mac: its options
 * come from `list_terminal_apps` on every render (never a cached or hardcoded
 * list), picking one stores exactly the id the backend will be handed back, the
 * "Choose an app…" row stores a `.app` path instead of a bundle id, and a
 * cancelled picker changes nothing.
 *
 * The menu is driven the way `OnboardingLanguagePicker.test.ts` drives it: click
 * the trigger, then click the row by its `data-value`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TerminalAppSelect from './TerminalAppSelect.svelte'
import type { TerminalApp, TerminalAppList } from '$lib/ipc/bindings'
import { CHOOSE_APP_VALUE, TERMINAL_APP_BUNDLE_ID } from './terminal-app-options'

const SETTING_ID = 'behavior.openTerminalHereApp'

const settingsMap: Record<string, unknown> = {}
const setSetting = vi.fn((id: string, value: unknown) => {
  settingsMap[id] = value
  return Promise.resolve()
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  return {
    ...actual,
    getSetting: (id: string) => settingsMap[id],
    setSetting: (id: string, value: unknown) => setSetting(id, value),
    onSpecificSettingChange: () => () => {},
  }
})

const listTerminalApps = vi.hoisted(() => vi.fn())
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listTerminalApps,
}))

const openAppPicker = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: openAppPicker }))

function app(id: string, displayName: string, icon: string | null = null): TerminalApp {
  return { id, displayName, icon, isRunning: false }
}

const TERMINAL = app(TERMINAL_APP_BUNDLE_ID, 'Terminal')
const GHOSTTY = app('com.mitchellh.ghostty', 'Ghostty', 'data:image/webp;base64,Z2g=')

/** Arms the IPC with one answer, the shape `TimedOut<TerminalAppList>` has. */
function answerWith(list: TerminalAppList, timedOut = false): void {
  listTerminalApps.mockResolvedValue({ data: list, timedOut })
}

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function mountRow(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = { target, instance: mount(TerminalAppSelect, { target, props: { ariaLabel: 'Open terminal here uses' } }) }
  // One tick for the mount, one for the IPC answer to land in `$state`.
  await tick()
  await tick()
  return target
}

/** Every menu row, as `[value, label]`. The menu portals to `document.body`. */
function rows(): [string, string][] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]')).map((el) => [
    el.getAttribute('data-value') ?? '',
    el.textContent.trim(),
  ])
}

async function openMenu(target: HTMLElement): Promise<void> {
  target.querySelector<HTMLButtonElement>('.select-trigger')?.click()
  await tick()
}

async function pick(target: HTMLElement, value: string): Promise<void> {
  await openMenu(target)
  Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]'))
    .find((el) => el.getAttribute('data-value') === value)
    ?.click()
  await tick()
  await tick()
}

beforeEach(() => {
  settingsMap[SETTING_ID] = TERMINAL_APP_BUNDLE_ID
  setSetting.mockClear()
  listTerminalApps.mockReset()
  openAppPicker.mockReset()
  answerWith({ apps: [TERMINAL, GHOSTTY], chosenId: TERMINAL_APP_BUNDLE_ID })
})

afterEach(() => {
  if (mounted) {
    void unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
  document.body.innerHTML = ''
})

describe('TerminalAppSelect', () => {
  it('builds its options from what the backend found installed, passing the stored choice down', async () => {
    const target = await mountRow()
    expect(listTerminalApps).toHaveBeenCalledWith(TERMINAL_APP_BUNDLE_ID)
    await openMenu(target)

    expect(rows()).toEqual([
      [TERMINAL_APP_BUNDLE_ID, 'Terminal'],
      ['com.mitchellh.ghostty', 'Ghostty'],
      [CHOOSE_APP_VALUE, 'Choose an app…'],
    ])
  })

  it('shows the app icon the backend read off the bundle', async () => {
    const target = await mountRow()
    await openMenu(target)

    const ghostty = document.querySelector<HTMLElement>('[data-part="item"][data-value="com.mitchellh.ghostty"]')
    expect(ghostty?.querySelector('img')?.getAttribute('src')).toBe('data:image/webp;base64,Z2g=')
  })

  it('stores the bundle id of the terminal that was picked', async () => {
    const target = await mountRow()
    await pick(target, 'com.mitchellh.ghostty')

    expect(setSetting).toHaveBeenCalledWith(SETTING_ID, 'com.mitchellh.ghostty')
  })

  it('stores the `.app` path the picker returned, not a bundle id', async () => {
    openAppPicker.mockResolvedValue('/Applications/Terminus.app')
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    expect(openAppPicker).toHaveBeenCalledOnce()
    expect(setSetting).toHaveBeenCalledWith(SETTING_ID, '/Applications/Terminus.app')
    // The pick isn't in the backend's list yet, so the row re-asks for its name and icon.
    expect(listTerminalApps).toHaveBeenCalledTimes(2)
  })

  it('leaves the setting alone when the picker is cancelled', async () => {
    openAppPicker.mockResolvedValue(null)
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    expect(setSetting).not.toHaveBeenCalled()
  })

  it('never stores the picker row as if it were an app', async () => {
    openAppPicker.mockResolvedValue(null)
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    expect(settingsMap[SETTING_ID]).toBe(TERMINAL_APP_BUNDLE_ID)
  })

  it('shows Terminal when the chosen app has been uninstalled', async () => {
    settingsMap[SETTING_ID] = 'dev.warp.Warp-Stable'
    // Warp is gone, so the backend leaves it out of `apps` and reports no choice.
    answerWith({ apps: [TERMINAL, GHOSTTY], chosenId: null })
    const target = await mountRow()

    expect(target.querySelector('.select-value')?.textContent.trim()).toBe('Terminal')
    // Displaying the fallback is all this row does; rewriting the setting belongs
    // to the moment the action actually opens Terminal instead.
    expect(setSetting).not.toHaveBeenCalled()
  })

  it('waits, disabled, rather than showing a list it does not have yet', async () => {
    answerWith({ apps: [], chosenId: null }, true)
    const target = await mountRow()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger?.disabled).toBe(true)
    expect(trigger?.textContent.trim()).toBe('Checking your apps…')
  })
})
