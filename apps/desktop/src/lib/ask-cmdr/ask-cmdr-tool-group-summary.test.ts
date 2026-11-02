/**
 * A tool group's one-line summary: what its calls looked at, counted per kind of thing, in our
 * words, so a collapsed group of 12 calls still says what happened.
 */

import { describe, expect, it } from 'vitest'
import type { ToolSubject } from '#lib/ipc/bindings.js'
import type { RailToolCall } from './ask-cmdr-messages'
import { GROUP_CATEGORY_KEYS, SUBJECT_CATEGORY, groupSummaryText } from './ask-cmdr-tool-group-summary'
import { tString } from '#lib/intl/messages.svelte.js'

function call(subject: ToolSubject | null, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId: 'c',
    tool: 'list_dir',
    running: false,
    subject,
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  }
}

const folder = (path: string): ToolSubject => ({ kind: 'folder', path: { text: path, linkable: true } })

describe('groupSummaryText', () => {
  it('counts each kind of thing the calls looked at, in a fixed order', () => {
    const calls = [
      call({ kind: 'search', query: '*.pdf', scopePaths: [] }),
      call(folder('/a')),
      call(folder('/b')),
      call({ kind: 'photoSearch', query: 'cats' }),
    ]
    expect(groupSummaryText(calls)).toBe('Worked through: 2 folders, 2 searches')
  })

  it('counts every file a call asked about, not the call', () => {
    const calls = [call({ kind: 'files', firstPath: { text: '/a.txt', linkable: true }, count: 3 }), call(folder('/a'))]
    expect(groupSummaryText(calls)).toBe('Worked through: 1 folder, 3 files')
  })

  it('says it is still looking while a line runs', () => {
    const calls = [call(folder('/a')), call(folder('/b'), { running: true })]
    expect(groupSummaryText(calls)).toBe('Working through: 2 folders')
  })

  it('counts a line whose call is still unresolved under a neutral kind, keeping the header whole', () => {
    const calls = [call(folder('/a')), call(null, { running: true })]
    expect(groupSummaryText(calls)).toBe('Working through: 1 folder, 1 other step')
  })

  it('has words for every category', () => {
    for (const key of Object.values(GROUP_CATEGORY_KEYS)) {
      expect(tString(key, { count: 2, countText: '2' }).length).toBeGreaterThan(0)
    }
    // Exhaustive by type; this pins that every subject kind lands in a category with words.
    for (const category of Object.values(SUBJECT_CATEGORY)) {
      expect(GROUP_CATEGORY_KEYS[category]).toBeDefined()
    }
  })
})
