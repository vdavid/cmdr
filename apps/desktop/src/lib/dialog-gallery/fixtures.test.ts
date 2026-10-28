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
import { serverSignInFixtures } from './fixtures/servers'
import { seedStore } from './store-seeding'

/** Dialogs that take callbacks only, so they have no fixture record by design. */
const CALLBACK_ONLY = new Set(['about', 'acknowledgements', 'license', 'commercial-reminder', 'mtp-permission'])

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

/**
 * The sign-in sheet's fixtures carry an `attempt`, which is a real prop the
 * component really calls. These cells prove each state answers what its gallery
 * row's note advertises, so a design review of the refusal or the key step is
 * looking at the outcome it was promised.
 */
describe('the sign-in sheet fixtures answer what their rows advertise', () => {
  const cases: [string, string][] = [
    ['add', 'refused'],
    ['add-prefilled', 'refused'],
    ['add-s3', 'refused'],
    ['sign-in', 'refused'],
    ['sign-in-s3', 'refused'],
    ['sign-in-guest', 'refused'],
    ['host-key-first-contact', 'connected'],
    ['host-key-changed', 'connected'],
    ['certificate-first-contact', 'connected'],
    ['certificate-changed', 'connected'],
  ]

  it.each(cases)('%s answers %s', async (stateId, kind) => {
    const request = serverSignInFixtures[stateId]?.request
    expect(request, `no fixture for ${stateId}`).toBeDefined()
    if (!request || request.mode === 'edit') throw new Error(`${stateId} has no attempt`)
    const outcome = await request.attempt({
      mode: 'add_smb',
      address: 'naspolya',
      name: '',
      username: null,
      intent: 'open',
    })
    expect(outcome.kind).toBe(kind)
  })

  it('edit mode carries no attempt: Save writes and closes, nothing dials', () => {
    const request = serverSignInFixtures.edit?.request
    expect(request?.mode).toBe('edit')
  })
})

/**
 * Regression: the 60,000-file group held its first 200 rows and paged the rest from the real
 * store, which has no fixture group and answered a total of 0, so scrolling past row 200
 * emptied the list. The patch's own `rowSource` answers instead.
 */
describe('the AI suggestions fixture pages its 60,000-file group', () => {
  it('keeps every row and the total as the list scrolls past the held window', async () => {
    const { suggestedOpsFixtures } = await import('./fixtures/suggested-ops')
    const trigger = await import('#lib/suggested-ops/suggested-ops-trigger.svelte.js')
    const undo = seedStore(trigger.suggestedOpsState, suggestedOpsFixtures['hard-cases']())
    try {
      await trigger.showRange(203, 30_000, 30_060)

      const review = trigger.reviewOf(203)
      expect(review?.windowError).toBe(false)
      expect(review?.window?.total).toBe(60_000)
      for (let index = 30_000; index < 30_060; index++) {
        expect(trigger.opAt(203, index)?.sourcePath).toBe(`/Volumes/SD/DCIM/DSC${String(10_000 + index)}.ARW`)
        expect(review?.liveFacts.get(index + 1)?.kind).toBe('present')
      }
    } finally {
      undo()
    }
  })
})
