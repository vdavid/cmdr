/**
 * Behavior tests for the "Edit files in" control.
 *
 * What matters here is that the row tells the truth about this Mac: it's usable
 * the moment a complete answer lands (a Mac whose only editor is TextEdit
 * included), it waits rather than guessing when the answer timed out, a
 * "Choose an app…" pick is stored the way Rust canonicalizes it, and browsing
 * never writes.
 *
 * The menu is driven the way `TerminalAppSelect.svelte.test.ts` drives it: click
 * the trigger, then click the row by its `data-value`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import TextEditorSelect from './TextEditorSelect.svelte'
import type { TextEditorApp, TextEditorList } from '$lib/ipc/bindings'
import { CHOOSE_APP_VALUE } from './app-choice-options'

const SETTING_ID = 'behavior.textEditorApp'

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

const listTextEditors = vi.hoisted(() => vi.fn())
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listTextEditors,
}))

const openAppPicker = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: openAppPicker }))

function app(id: string, displayName: string): TextEditorApp {
  return { id, displayName, icon: null }
}

const SUBLIME = app('com.sublimetext.4', 'Sublime Text')
const XCODE = app('com.apple.dt.Xcode', 'Xcode')
const SUBLIME_PATH = '/Applications/Sublime Text.app'

function editors(overrides: Partial<TextEditorList> = {}): TextEditorList {
  return { defaultAppName: 'TextEdit', defaultAppIcon: null, apps: [], chosenId: 'system', ...overrides }
}

/** Arms the IPC with one answer for every call, the shape `TimedOut<TextEditorList>` has. */
function answerWith(list: TextEditorList, timedOut = false): void {
  listTextEditors.mockResolvedValue({ data: list, timedOut })
}

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

async function mountRow(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = { target, instance: mount(TextEditorSelect, { target, props: { ariaLabel: 'Edit files in' } }) }
  // One tick for the mount, one for the IPC answer to land in `$state`.
  await tick()
  await tick()
  return target
}

function trigger(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>('.select-trigger')
}

/** Every menu row, as `[value, label]`. The menu portals to `document.body`. */
function rows(): [string, string][] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]')).map((el) => [
    el.getAttribute('data-value') ?? '',
    el.textContent.trim(),
  ])
}

async function pick(target: HTMLElement, value: string): Promise<void> {
  trigger(target)?.click()
  await tick()
  Array.from(document.querySelectorAll<HTMLElement>('[data-part="item"]'))
    .find((el) => el.getAttribute('data-value') === value)
    ?.click()
  await tick()
  await tick()
}

beforeEach(() => {
  settingsMap[SETTING_ID] = 'system'
  setSetting.mockClear()
  listTextEditors.mockReset()
  openAppPicker.mockReset()
  answerWith(editors({ apps: [XCODE, SUBLIME] }))
})

afterEach(() => {
  if (mounted) {
    void unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
  document.body.innerHTML = ''
})

describe('TextEditorSelect', () => {
  it('is ready on a Mac whose only editor is TextEdit', async () => {
    // `apps` leaves out the system default, so a complete answer can be empty.
    answerWith(editors({ apps: [] }))
    const target = await mountRow()

    expect(trigger(target)?.disabled).toBe(false)
    expect(target.querySelector('.select-value')?.textContent.trim()).toBe('System default (TextEdit)')
    trigger(target)?.click()
    await tick()
    expect(rows()).toEqual([
      ['system', 'System default (TextEdit)'],
      [CHOOSE_APP_VALUE, 'Choose an app…'],
    ])
  })

  it('lists the other editors by name, passing the stored choice down', async () => {
    const target = await mountRow()
    expect(listTextEditors).toHaveBeenCalledWith('system')
    trigger(target)?.click()
    await tick()

    expect(rows()).toEqual([
      ['system', 'System default (TextEdit)'],
      ['com.sublimetext.4', 'Sublime Text'],
      ['com.apple.dt.Xcode', 'Xcode'],
      [CHOOSE_APP_VALUE, 'Choose an app…'],
    ])
  })

  it('waits, disabled, when the answer timed out', async () => {
    answerWith(editors({ apps: [], chosenId: null }), true)
    const target = await mountRow()

    expect(trigger(target)?.disabled).toBe(true)
    expect(trigger(target)?.textContent.trim()).toBe('Checking your apps…')
  })

  it('stores the bundle id of the editor that was picked', async () => {
    const target = await mountRow()
    await pick(target, 'com.sublimetext.4')

    expect(setSetting).toHaveBeenCalledWith(SETTING_ID, 'com.sublimetext.4')
  })

  it('stores a "Choose an app…" pick the way Rust canonicalizes it', async () => {
    openAppPicker.mockResolvedValue(SUBLIME_PATH)
    listTextEditors.mockImplementation((appChoice: string) =>
      Promise.resolve({
        data: editors({
          apps: [XCODE, SUBLIME],
          chosenId: appChoice === SUBLIME_PATH ? 'com.sublimetext.4' : 'system',
        }),
        timedOut: false,
      }),
    )
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    await vi.waitFor(() => {
      expect(setSetting).toHaveBeenCalledWith(SETTING_ID, 'com.sublimetext.4')
    })
    expect(listTextEditors).toHaveBeenCalledWith(SUBLIME_PATH)
    expect(setSetting).toHaveBeenCalledOnce()
  })

  it('stores the picked path when that question timed out', async () => {
    openAppPicker.mockResolvedValue(SUBLIME_PATH)
    listTextEditors.mockImplementation((appChoice: string) =>
      Promise.resolve(
        appChoice === SUBLIME_PATH
          ? { data: editors({ chosenId: null }), timedOut: true }
          : { data: editors({ apps: [XCODE, SUBLIME] }), timedOut: false },
      ),
    )
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    await vi.waitFor(() => {
      expect(setSetting).toHaveBeenCalledWith(SETTING_ID, SUBLIME_PATH)
    })
  })

  it('leaves the setting alone when the picker is cancelled', async () => {
    openAppPicker.mockResolvedValue(null)
    const target = await mountRow()
    await pick(target, CHOOSE_APP_VALUE)

    expect(setSetting).not.toHaveBeenCalled()
    expect(settingsMap[SETTING_ID]).toBe('system')
  })

  it('shows the system default when the chosen editor is gone, without writing', async () => {
    settingsMap[SETTING_ID] = 'com.barebones.bbedit'
    // BBEdit is gone, so the backend reports no choice.
    answerWith(editors({ apps: [XCODE, SUBLIME], chosenId: null }))
    const target = await mountRow()

    expect(target.querySelector('.select-value')?.textContent.trim()).toBe('System default (TextEdit)')
    expect(setSetting).not.toHaveBeenCalled()
  })
})
