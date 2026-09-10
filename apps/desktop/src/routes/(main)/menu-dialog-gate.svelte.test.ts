/**
 * The native menu's dialog-refusal sync: which commands the menu is told a dialog refuses.
 *
 * Chrome for the regular items (the dispatch core refuses those commands itself), but the two
 * check items revert a refused click from this same list, so the list has to be right.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import { startMenuDialogGate } from './menu-dialog-gate.svelte'
import { setCommandsRefusedOverDialog } from '$lib/tauri-commands'
import type { DialogsOnScreen } from './command-dispatch-context'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/tauri-commands')>()),
  setCommandsRefusedOverDialog: vi.fn(() => Promise.resolve()),
}))

/** What's up, as a reactive set the gate's effect can track. */
const open = new SvelteSet<'dialog' | 'palette'>()
const onScreen = (): DialogsOnScreen => ({ dialogOpen: open.has('dialog'), paletteOpen: open.has('palette') })

let stop: (() => void) | null = null

beforeEach(() => {
  vi.clearAllMocks()
  open.clear()
})

afterEach(() => {
  stop?.()
  stop = null
})

/** Starts the sync and settles the initial push. */
function start() {
  stop = startMenuDialogGate(onScreen)
  flushSync()
}

/** The command ids of the most recent push. */
function lastPush(): string[] {
  const calls = vi.mocked(setCommandsRefusedOverDialog).mock.calls
  return calls[calls.length - 1]?.[0] ?? []
}

describe('what the menu is told', () => {
  it('starts by refusing nothing', () => {
    start()

    expect(setCommandsRefusedOverDialog).toHaveBeenCalledWith([])
  })

  it('refuses the blocked commands while a dialog is up, and nothing once it closes', () => {
    start()

    open.add('dialog')
    flushSync()
    expect(lastPush()).toContain('tab.close')
    expect(lastPush()).toContain('servers.connect')
    // The check items revert a refused click from this list.
    expect(lastPush()).toContain('view.showHidden')
    expect(lastPush()).toContain('view.setMode')

    open.delete('dialog')
    flushSync()
    expect(lastPush()).toEqual([])
  })

  it('leaves out the commands that run over dialogs, and the ones that depend on focus', () => {
    start()

    open.add('dialog')
    flushSync()
    expect(lastPush()).not.toContain('app.settings')
    expect(lastPush()).not.toContain('view.zoom.in')
    // The menu can't see focus, and these items forward the native edit actions elsewhere.
    expect(lastPush()).not.toContain('edit.paste')
    expect(lastPush()).not.toContain('selection.selectAll')
  })

  it('counts the command palette as open too', () => {
    start()

    open.add('palette')
    flushSync()
    expect(lastPush()).toContain('tab.close')
  })

  it('stays quiet while the verdict is unchanged', () => {
    start()
    vi.mocked(setCommandsRefusedOverDialog).mockClear()

    open.add('dialog')
    flushSync()
    open.add('palette')
    flushSync()

    expect(setCommandsRefusedOverDialog).toHaveBeenCalledTimes(1)
  })

  it('stops pushing once torn down', () => {
    start()
    stop?.()
    stop = null
    vi.mocked(setCommandsRefusedOverDialog).mockClear()

    open.add('dialog')
    flushSync()

    expect(setCommandsRefusedOverDialog).not.toHaveBeenCalled()
  })
})
