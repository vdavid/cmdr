/**
 * Fixtures and gallery rows have to agree, or a Debug button opens nothing.
 *
 * `DialogGallery.svelte` looks every fixture up by the state id its row
 * advertises, so a typo on either side is a dead button that only shows up when
 * someone clicks it mid-review. These walk the SAME `fixtureRecords` object the
 * harness reads, so the check can't pass while the harness disagrees.
 */

import { describe, expect, it } from 'vitest'
import { DIALOG_GALLERY_ENTRIES } from './gallery-registry'
import { fixtureRecords } from './fixtures'

/** Dialogs that take callbacks only, so they have no fixture record by design. */
const CALLBACK_ONLY = new Set(['about', 'license', 'commercial-reminder', 'connect-to-server', 'mtp-permission'])

const readyEntries = DIALOG_GALLERY_ENTRIES.filter((entry) => entry.status === 'ready')
const records: Record<string, Record<string, unknown> | undefined> = fixtureRecords

describe('dialog gallery fixtures', () => {
  it.each(Object.keys(fixtureRecords))('%s: fixture keys match the row’s state ids', (dialogId) => {
    const entry = DIALOG_GALLERY_ENTRIES.find((candidate) => candidate.dialogId === dialogId)
    expect(entry, `no gallery row for ${dialogId}`).toBeDefined()
    const stateIds = [...(entry?.states ?? [])].map((state) => state.id).sort()
    expect(Object.keys(records[dialogId] ?? {}).sort()).toEqual(stateIds)
  })

  it('resolves every state of every ready row to a defined fixture', () => {
    const dead: string[] = []
    for (const entry of readyEntries) {
      if (CALLBACK_ONLY.has(entry.dialogId)) continue
      const record = records[entry.dialogId]
      if (!record) {
        dead.push(`${entry.dialogId} (no fixture record)`)
        continue
      }
      for (const state of entry.states) {
        if (record[state.id] === undefined) dead.push(`${entry.dialogId} / ${state.id}`)
      }
    }
    expect(dead).toEqual([])
  })

  it('keeps the callback-only list honest: those rows expose exactly one state', () => {
    for (const entry of readyEntries) {
      if (!CALLBACK_ONLY.has(entry.dialogId)) continue
      expect(entry.states, `${entry.dialogId} should expose one state`).toHaveLength(1)
      // Whatever a reviewer sees there comes from live app state, not a fixture,
      // so the row has to say so rather than implying a curated preview.
      expect(entry.note?.trim(), `${entry.dialogId} must disclose that it has no fixture`).toBeTruthy()
    }
  })
})
