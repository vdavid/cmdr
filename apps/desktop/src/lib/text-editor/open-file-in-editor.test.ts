/**
 * What `openFileInEditor` does around the launch: which app it asks for, what it
 * writes back when that app is gone, and how it words a launch that never started.
 *
 * The IPC, the settings store, and the toast surface are all mocked, so this pins
 * the side effects (what gets written, what gets said) rather than the launch.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// Untyped `vi.fn()`s on purpose: a typed stub of `openInEditor` would declare two
// same-typed positional string parameters, which `cmdr/no-confusable-callback-params`
// rightly refuses. The assertions below name the arguments they expect.
const m = vi.hoisted(() => ({
  openInEditor: vi.fn(),
  addToast: vi.fn(),
  dismissToast: vi.fn(),
  settings: new Map<string, unknown>(),
}))

/** The typed refusal `openInEditor` throws, as much of it as this suite reads. */
interface EditorRefusal {
  type: 'launchRefused' | 'timedOut'
  errno?: number | null
}

/** Stands in for the wrapper's `TypedFailure` subclass, which `asOpenInEditorError` unwraps. */
class FakeOpenInEditorFailure extends Error {
  constructor(readonly failure: EditorRefusal) {
    super('refused')
  }
}

vi.mock('$lib/tauri-commands', () => ({
  openInEditor: m.openInEditor,
  asOpenInEditorError: (error: unknown) => (error instanceof FakeOpenInEditorFailure ? error.failure : null),
}))

vi.mock('$lib/ui/toast', () => ({ addToast: m.addToast, dismissToast: m.dismissToast }))

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

vi.mock('$lib/intl/messages.svelte', () => ({
  tString: (key: string, params?: Record<string, unknown>) => (params ? `${key} ${JSON.stringify(params)}` : key),
}))

import { openFileInEditor } from './open-file-in-editor'

const { openInEditor, addToast, dismissToast, settings } = m
const APP_KEY = 'behavior.textEditorApp'
const TOAST_ID = 'text-editor'
const SUBLIME = 'com.sublimetext.4'
const FILE = '/Users/dave/notes.txt'

/** Resolves the launch with this report, filling in the fields a test doesn't care about. */
function reports(report: { outcome?: string; openedInName?: string | null }): void {
  openInEditor.mockResolvedValue({ outcome: 'opened', openedInName: null, otherEditorsInstalled: null, ...report })
}

beforeEach(() => {
  vi.clearAllMocks()
  settings.clear()
  reports({})
})

describe('openFileInEditor: which app it asks for', () => {
  it('sends the stored choice to the launch', async () => {
    settings.set(APP_KEY, SUBLIME)

    await openFileInEditor(FILE)

    expect(openInEditor).toHaveBeenCalledExactlyOnceWith(FILE, SUBLIME, false)
  })

  it('sends the system default when nothing is stored', async () => {
    await openFileInEditor(FILE)

    expect(openInEditor).toHaveBeenCalledExactlyOnceWith(FILE, 'system', false)
  })

  it('sends the system default when the stored value is not a string', async () => {
    settings.set(APP_KEY, 100)

    await openFileInEditor(FILE)

    expect(openInEditor).toHaveBeenCalledExactlyOnceWith(FILE, 'system', false)
  })

  it('resolves true and says nothing on a plain open', async () => {
    settings.set(APP_KEY, SUBLIME)

    await expect(openFileInEditor(FILE)).resolves.toBe(true)

    expect(addToast).not.toHaveBeenCalled()
    expect(settings.get(APP_KEY)).toBe(SUBLIME)
  })
})

describe('openFileInEditor: the chosen app is gone', () => {
  beforeEach(() => {
    settings.set(APP_KEY, SUBLIME)
  })

  it('resets the setting to the system default and names the app the file opened in', async () => {
    reports({ outcome: 'chosen_app_missing_opened_default_instead', openedInName: 'TextEdit' })

    await expect(openFileInEditor(FILE)).resolves.toBe(true)

    expect(settings.get(APP_KEY)).toBe('system')
    expect(addToast).toHaveBeenCalledOnce()
    expect(addToast.mock.calls[0][1]).toMatchObject({
      id: TOAST_ID,
      dismissal: 'persistent',
      props: { message: 'fileExplorer.edit.appMissing {"app":"TextEdit"}' },
    })
  })

  it('uses the unnamed wording when no name came back', async () => {
    reports({ outcome: 'chosen_app_missing_opened_default_instead', openedInName: null })

    await openFileInEditor(FILE)

    expect(addToast.mock.calls[0][1]).toMatchObject({ props: { message: 'fileExplorer.edit.appMissingUnnamed' } })
  })

  it('dismisses what it said last before saying the next thing', async () => {
    // Both toasts this feature raises are PERSISTENT, and `addToast`'s same-id path
    // keeps the first toast's dismissal and width: sharing one id and dismissing
    // first is what makes the replacement total.
    reports({ outcome: 'chosen_app_missing_opened_default_instead', openedInName: 'TextEdit' })

    await openFileInEditor(FILE)

    expect(dismissToast).toHaveBeenCalledWith(TOAST_ID)
    expect(dismissToast.mock.invocationCallOrder[0]).toBeLessThan(addToast.mock.invocationCallOrder[0])
  })
})

describe('openFileInEditor: a launch that never started', () => {
  it('words a launch that outlived its deadline, under the one id', async () => {
    openInEditor.mockRejectedValue(new FakeOpenInEditorFailure({ type: 'timedOut' }))

    await expect(openFileInEditor(FILE)).resolves.toBe(false)

    expect(addToast).toHaveBeenCalledWith(
      'fileExplorer.edit.timedOut',
      expect.objectContaining({ id: TOAST_ID, level: 'error' }),
    )
  })

  it('words a refused launch, under the one id', async () => {
    openInEditor.mockRejectedValue(new FakeOpenInEditorFailure({ type: 'launchRefused', errno: 2 }))

    await expect(openFileInEditor(FILE)).resolves.toBe(false)

    expect(addToast).toHaveBeenCalledWith(
      'fileExplorer.edit.launchRefused',
      expect.objectContaining({ id: TOAST_ID, level: 'error' }),
    )
  })

  it('never rejects, even on a throw that carries no typed reason', async () => {
    openInEditor.mockRejectedValue(new Error('the IPC bridge went away'))

    await expect(openFileInEditor(FILE)).resolves.toBe(false)

    expect(addToast).toHaveBeenCalledWith('fileExplorer.edit.launchRefused', expect.anything())
  })

  it('leaves the stored choice alone', async () => {
    settings.set(APP_KEY, SUBLIME)
    openInEditor.mockRejectedValue(new FakeOpenInEditorFailure({ type: 'timedOut' }))

    await openFileInEditor(FILE)

    expect(settings.get(APP_KEY)).toBe(SUBLIME)
  })
})
