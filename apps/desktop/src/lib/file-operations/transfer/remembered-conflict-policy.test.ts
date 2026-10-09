/**
 * The remembered "files already exist" choice: what the dialog opens on, what a
 * confirm dispatches, when a pick is written back, and when the overwrite note
 * shows. The settings store is stubbed; the dialog wiring is covered in
 * `TransferDialog.remembered-policy.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DefaultConflictPolicy } from '$lib/settings'

const stored: { value: unknown } = { value: 'stop' }
const setSettingMock = vi.fn<(id: string, value: unknown) => void>()

vi.mock('$lib/settings', () => ({
  getSetting: () => stored.value,
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
}))

import {
  dispatchedConflictPolicy,
  overwriteNoteReach,
  readRememberedConflictPolicy,
  rememberConflictPolicy,
  REMEMBERED_CONFLICT_POLICY_SETTING,
} from './remembered-conflict-policy'

beforeEach(() => {
  stored.value = 'stop'
  setSettingMock.mockReset()
})

describe('readRememberedConflictPolicy', () => {
  it('returns each of the five dialog choices as stored', () => {
    for (const value of ['stop', 'skip', 'overwrite', 'overwrite_smaller', 'overwrite_older'] as const) {
      stored.value = value
      expect(readRememberedConflictPolicy()).toBe(value)
    }
  })

  it('falls back to asking for anything it does not recognize', () => {
    for (const value of ['rename', 'overwrite_all', 500, undefined, null]) {
      stored.value = value
      expect(readRememberedConflictPolicy()).toBe('stop')
    }
  })
})

describe('dispatchedConflictPolicy', () => {
  it('sends a remembered policy only while its radios are on screen', () => {
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'remembered', choiceVisible: true })).toBe(
      'overwrite',
    )
    // Still checking, couldn't check, or nothing clashes: the person never saw it, so ask.
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'remembered', choiceVisible: false })).toBe('stop')
    expect(dispatchedConflictPolicy({ policy: 'skip', source: 'remembered', choiceVisible: false })).toBe('stop')
  })

  it('keeps a picked or an explicitly named policy whether or not the radios show', () => {
    expect(dispatchedConflictPolicy({ policy: 'skip', source: 'picked', choiceVisible: false })).toBe('skip')
    expect(dispatchedConflictPolicy({ policy: 'overwrite', source: 'explicit', choiceVisible: false })).toBe(
      'overwrite',
    )
  })
})

describe('rememberConflictPolicy', () => {
  it('writes a person’s confirmed pick that differs from the saved one', () => {
    rememberConflictPolicy({ policy: 'overwrite_older', source: 'picked', operationType: 'copy' })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'overwrite_older')
  })

  it('remembers a move’s pick too', () => {
    rememberConflictPolicy({ policy: 'skip', source: 'picked', operationType: 'move' })
    expect(setSettingMock).toHaveBeenCalledWith(REMEMBERED_CONFLICT_POLICY_SETTING, 'skip')
  })

  it('writes nothing when the pick matches the saved choice (sparse persistence stays sparse)', () => {
    stored.value = 'skip'
    rememberConflictPolicy({ policy: 'skip', source: 'picked', operationType: 'copy' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes a remembered or an explicitly named (MCP) policy', () => {
    rememberConflictPolicy({ policy: 'overwrite', source: 'remembered', operationType: 'copy' })
    rememberConflictPolicy({ policy: 'overwrite', source: 'explicit', operationType: 'copy' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes from compress, which has no conflict choice', () => {
    rememberConflictPolicy({ policy: 'overwrite', source: 'picked', operationType: 'compress' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('never writes a policy the setting cannot hold', () => {
    rememberConflictPolicy({ policy: 'rename', source: 'picked', operationType: 'copy' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('overwriteNoteReach', () => {
  const cases: [DefaultConflictPolicy, ReturnType<typeof overwriteNoteReach>][] = [
    ['overwrite', 'all'],
    ['overwrite_smaller', 'smaller'],
    ['overwrite_older', 'older'],
    ['skip', null],
    ['stop', null],
  ]

  it.each(cases)('a remembered %s gives %s', (policy, reach) => {
    expect(overwriteNoteReach({ policy, source: 'remembered' })).toBe(reach)
  })

  it('stays quiet once the person picked, or for an explicitly named policy', () => {
    expect(overwriteNoteReach({ policy: 'overwrite', source: 'picked' })).toBeNull()
    expect(overwriteNoteReach({ policy: 'overwrite', source: 'explicit' })).toBeNull()
  })
})
