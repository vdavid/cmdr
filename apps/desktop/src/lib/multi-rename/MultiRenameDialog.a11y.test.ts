/**
 * Tier 3 a11y tests for `MultiRenameDialog.svelte`: the sheet with a preview of
 * ready, unchanged, blocked, and missing rows, and with a mask error showing.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { previewMultiRename } = vi.hoisted(() => ({ previewMultiRename: vi.fn() }))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  previewMultiRename,
  getMultiRenamePreviewRows: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  renderMultiRenameExamples: vi.fn(() => Promise.resolve(null)),
  applyMultiRename: vi.fn(() => Promise.resolve({ ok: false, error: { type: 'nothingToRename' } })),
  getMultiRenamePresets: vi.fn(() => Promise.resolve([{ id: 'p1', name: 'Bez diakritiky', spec: {} }])),
  saveMultiRenamePreset: vi.fn(() => Promise.resolve()),
  deleteMultiRenamePreset: vi.fn(() => Promise.resolve()),
}))

const ROWS = [
  { row: 0, oldName: 'Žádost o přezkum.pdf', newName: 'Zadost o prezkum.pdf', status: { type: 'ready' } },
  { row: 1, oldName: 'plain.pdf', newName: 'plain.pdf', status: { type: 'unchanged' } },
  { row: 2, oldName: 'a.pdf', newName: 'b.pdf', status: { type: 'targetExists' } },
  {
    row: 3,
    oldName: 'c.pdf',
    newName: 'x/y.pdf',
    status: { type: 'invalidName', reason: { type: 'disallowedCharacter', character: '/' } },
  },
  { row: 4, oldName: 'gone.pdf', newName: 'gone.pdf', status: { type: 'missing' } },
]

async function mountSheet(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MultiRenameDialog, {
    target,
    props: {
      session: { sessionId: 'S', count: 5 },
      onApplied: () => {},
      onClose: () => {},
    },
  })
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
  return target
}

describe('MultiRenameDialog a11y', () => {
  it('with a preview of mixed rows has no violations', async () => {
    previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 1, counts: { ready: 1, unchanged: 1, problems: 3 }, rows: ROWS },
    })
    await expectNoA11yViolations(await mountSheet())
  })

  it('with a mask error showing has no violations', async () => {
    previewMultiRename.mockResolvedValue({
      ok: false,
      error: { type: 'spec', error: { type: 'nameMask', error: { type: 'unclosed', at: 0 } } },
    })
    await expectNoA11yViolations(await mountSheet())
  })
})
