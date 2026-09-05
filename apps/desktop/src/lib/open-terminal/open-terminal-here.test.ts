/**
 * What `openTerminalHereForFolder` does around the launch: the first-use picker's
 * side effects (write the adopted app, spend the hint flag, raise the toast), and
 * the four things that can come back other than a plain success.
 *
 * The picker's decision table itself is pinned in `first-use-pick.test.ts`; this
 * suite is about the orchestration around it, so the IPC, the settings store, and
 * the toast surface are all mocked.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TerminalApp } from '$lib/ipc/bindings'

// Untyped `vi.fn()`s on purpose: a typed stub of `openTerminalHere` would declare
// three same-typed positional parameters, which `cmdr/no-confusable-callback-params`
// rightly refuses. The assertions below name the arguments they expect.
const m = vi.hoisted(() => ({
  listTerminalApps: vi.fn(),
  openTerminalHere: vi.fn(),
  terminalAppDisplayName: vi.fn(),
  addToast: vi.fn(),
  settings: new Map<string, unknown>(),
}))

/** The typed refusal `openTerminalHere` throws, as much of it as this suite reads. */
interface TerminalRefusal {
  type: 'launchRefused' | 'timedOut'
  errno?: number | null
}

/** Stands in for the wrapper's `TypedFailure` subclass, which `asOpenTerminalError` unwraps. */
class FakeOpenTerminalFailure extends Error {
  constructor(readonly failure: TerminalRefusal) {
    super('refused')
  }
}

vi.mock('$lib/tauri-commands', () => ({
  listTerminalApps: m.listTerminalApps,
  openTerminalHere: m.openTerminalHere,
  terminalAppDisplayName: m.terminalAppDisplayName,
  asOpenTerminalError: (error: unknown) => (error instanceof FakeOpenTerminalFailure ? error.failure : null),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: m.addToast }))

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => m.settings.get(id),
  setSetting: (id: string, value: unknown) => {
    m.settings.set(id, value)
  },
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(),
  settingAnchorId: (id: string) => `setting-${id}`,
}))

vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))

import { openTerminalHereForFolder } from './open-terminal-here'

const { listTerminalApps, openTerminalHere, terminalAppDisplayName, addToast, settings } = m
const APP_KEY = 'behavior.openTerminalHereApp'
const SEEN_KEY = 'behavior.openTerminalHereToastSeen'
const TERMINAL = 'com.apple.Terminal'
const WARP = 'dev.warp.Warp-Stable'

function app(id: string, isRunning = false): TerminalApp {
  return { id, displayName: id, icon: null, isRunning }
}

/** The list query answering with these apps, un-timed-out. */
function installed(...apps: TerminalApp[]): void {
  listTerminalApps.mockResolvedValue({ data: { apps, chosenId: apps[0]?.id ?? null }, timedOut: false })
}

beforeEach(() => {
  vi.clearAllMocks()
  settings.clear()
  settings.set(APP_KEY, TERMINAL)
  settings.set(SEEN_KEY, false)
  openTerminalHere.mockResolvedValue('opened')
  terminalAppDisplayName.mockResolvedValue(null)
  installed(app(TERMINAL))
})

describe('openTerminalHereForFolder: the ordinary run', () => {
  it('launches the stored app at the folder and says nothing', async () => {
    settings.set(SEEN_KEY, true)
    settings.set(APP_KEY, WARP)

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(openTerminalHere).toHaveBeenCalledWith('/Users/dave/code', 'root', WARP)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('skips the list query once the hint has been spent', async () => {
    settings.set(SEEN_KEY, true)

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(listTerminalApps).not.toHaveBeenCalled()
  })
})

describe('openTerminalHereForFolder: the first-use picker', () => {
  it('adopts the one running terminal, writes it down, and raises the hint once', async () => {
    installed(app(TERMINAL), app(WARP, true))

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(openTerminalHere).toHaveBeenCalledWith('/Users/dave/code', 'root', WARP)
    expect(settings.get(APP_KEY)).toBe(WARP)
    expect(settings.get(SEEN_KEY)).toBe(true)
    expect(addToast).toHaveBeenCalledOnce()
  })

  it('leaves the hint unspent when Terminal is the only terminal installed', async () => {
    installed(app(TERMINAL, true))

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(openTerminalHere).toHaveBeenCalledWith('/Users/dave/code', 'root', TERMINAL)
    expect(settings.get(SEEN_KEY)).toBe(false)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('still launches when the app list times out', async () => {
    listTerminalApps.mockResolvedValue({ data: { apps: [], chosenId: null }, timedOut: true })

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(openTerminalHere).toHaveBeenCalledWith('/Users/dave/code', 'root', TERMINAL)
    expect(settings.get(SEEN_KEY)).toBe(false)
  })
})

describe('openTerminalHereForFolder: the outcomes it reports', () => {
  it('resets the setting and names the app when the chosen one is gone', async () => {
    settings.set(SEEN_KEY, true)
    settings.set(APP_KEY, WARP)
    openTerminalHere.mockResolvedValue('app_missing_opened_terminal_instead')
    terminalAppDisplayName.mockResolvedValue('Warp')

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(terminalAppDisplayName).toHaveBeenCalledWith(WARP)
    expect(settings.get(APP_KEY)).toBe(TERMINAL)
    expect(addToast).toHaveBeenCalledOnce()
    expect(addToast.mock.calls[0][1]).toMatchObject({ props: { appName: 'Warp' } })
  })

  it('says the folder has no path a terminal can open', async () => {
    settings.set(SEEN_KEY, true)
    openTerminalHere.mockResolvedValue('not_a_local_path')

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'mtp-1' })

    expect(addToast).toHaveBeenCalledWith('commands.handler.openTerminalHere.noPath', expect.anything())
  })

  it('words a refused launch without saying anything went wrong twice', async () => {
    settings.set(SEEN_KEY, true)
    openTerminalHere.mockRejectedValue(new FakeOpenTerminalFailure({ type: 'launchRefused', errno: 2 }))

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(addToast).toHaveBeenCalledWith(
      'commands.handler.openTerminalHere.launchRefused',
      expect.objectContaining({ level: 'error' }),
    )
  })

  it('words a launch that outlived its deadline', async () => {
    settings.set(SEEN_KEY, true)
    openTerminalHere.mockRejectedValue(new FakeOpenTerminalFailure({ type: 'timedOut' }))

    await openTerminalHereForFolder({ folder: '/Users/dave/code', volumeId: 'root' })

    expect(addToast).toHaveBeenCalledWith(
      'commands.handler.openTerminalHere.timedOut',
      expect.objectContaining({ level: 'error' }),
    )
  })
})

describe('openTerminalHereForFolder: nothing to open', () => {
  it('says so and launches nothing when the pane resolved no folder', async () => {
    settings.set(SEEN_KEY, true)

    await openTerminalHereForFolder({ folder: null, volumeId: 'mtp-1' })

    expect(openTerminalHere).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith('commands.handler.openTerminalHere.noPath', expect.anything())
  })
})
