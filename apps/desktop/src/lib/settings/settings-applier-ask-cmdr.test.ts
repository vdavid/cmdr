/**
 * The settings-applier's Ask Cmdr rows.
 *
 * The backend reads Ask Cmdr's switch and the proactive cadence fresh from `settings.json`, and
 * follows them itself once the shared store holds a change (`settings::store_watch`). The applier
 * used to push them, which ran before the changing window's debounced save and re-read the old file
 * (#1194), so a push coming back from here is a regression.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const { order, pushAdbConfigToBackend, scheduleAdbConfigPush, cancelScheduledAdbConfigPush, noteSlotSettingChanged } =
  vi.hoisted(() => ({
    order: [] as string[],
    pushAdbConfigToBackend: vi.fn<() => Promise<void>>(() => Promise.resolve()),
    scheduleAdbConfigPush: vi.fn<() => void>(),
    cancelScheduledAdbConfigPush: vi.fn<() => void>(),
    noteSlotSettingChanged: vi.fn<() => void>(),
  }))

/** The change listener the applier registers, captured so the test can fire it. */
let changeListener: ((change: { id: string; value: unknown }) => void) | undefined

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const stubbed: Record<string, unknown> = {}
  for (const key of Object.keys(actual)) {
    // Every backend call records its name, so a test can tell what a change reached.
    stubbed[key] =
      typeof actual[key] === 'function'
        ? () => {
            order.push(key)
            return Promise.resolve()
          }
        : actual[key]
  }
  return stubbed
})

vi.mock('#lib/settings/index.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('#lib/settings/index.js')>()
  return {
    ...actual,
    initializeSettings: vi.fn().mockResolvedValue(undefined),
    forceSave: () => {
      order.push('save')
      return Promise.resolve(true)
    },
    onSettingChange: (listener: (change: { id: string; value: unknown }) => void) => {
      changeListener = listener
      return () => {
        changeListener = undefined
      }
    },
  }
})

vi.mock('#lib/ask-cmdr/ask-cmdr-trigger.svelte.js', () => ({ noteSlotSettingChanged }))

vi.mock('#lib/adb/adb-settings.js', () => ({
  cancelScheduledAdbConfigPush,
  pushAdbConfigToBackend,
  scheduleAdbConfigPush,
}))

import { initSettingsApplier, cleanupSettingsApplier } from './settings-applier'

beforeEach(() => {
  order.length = 0
  changeListener = undefined
  pushAdbConfigToBackend.mockClear()
  scheduleAdbConfigPush.mockClear()
  cancelScheduledAdbConfigPush.mockClear()
  noteSlotSettingChanged.mockClear()
})

afterEach(() => {
  cleanupSettingsApplier()
})

describe('settings-applier: the keys the backend follows itself', () => {
  it.each(['askCmdr.enabled', 'askCmdr.proactive', 'askCmdr.wakeDelay', 'askCmdr.wakeToast'])(
    '%s neither saves nor pushes from here',
    async (id) => {
      await initSettingsApplier()
      order.length = 0
      changeListener?.({ id, value: true })
      await Promise.resolve()
      // Startup's own backend pushes may still be landing, so only what this change could send counts.
      expect(order.filter((name) => name === 'save' || name.startsWith('askCmdr'))).toEqual([])
    },
  )
})

describe('settings-applier: fileOperations.adbBinaryPath', () => {
  it('routes path edits through the debounced ADB push', async () => {
    await initSettingsApplier()

    changeListener?.({ id: 'fileOperations.adbBinaryPath', value: '/a' })
    changeListener?.({ id: 'fileOperations.adbBinaryPath', value: '/adb' })
    expect(scheduleAdbConfigPush).toHaveBeenCalledTimes(2)
  })
})

describe('settings-applier: ai.reasoningEfforts', () => {
  it('nudges the rail, since the effort moves the reply room and so the budget it shows', async () => {
    await initSettingsApplier()

    changeListener?.({ id: 'ai.reasoningEfforts', value: '{"anthropic/claude-opus-5-5":"max"}' })
    expect(noteSlotSettingChanged).toHaveBeenCalledOnce()
  })
})
