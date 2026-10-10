/**
 * Tier 3 a11y tests for `MultiRenameDialog.svelte`: the sheet with a preview of
 * ready, unchanged, blocked, and missing rows, with a mask error showing, and with
 * every whole-name option on and Undo rename in the footer, and with names typed in
 * Results (a marked row, the notice and its links); and
 * its tooltip bodies, `SearchOptionChips` (chips on and off, a tooltip showing) and
 * `PlaceholderTip`, with examples rendered.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import SearchOptionChips from './SearchOptionChips.svelte'
import PlaceholderTip from './PlaceholderTip.svelte'
import { DEFAULT_SPEC } from './spec'
import { PLACEHOLDER_HELP, hintMask } from './placeholder-help'
import { MARK_END, MARK_START } from './rename-examples'
import { searchExampleKey } from './search-option-help'
import { setLastMultiRenameRun } from './last-run.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { previewMultiRename, getMultiRenameLastSettings } = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenameLastSettings: vi.fn(() => Promise.resolve(null as unknown)),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  previewMultiRename,
  getMultiRenamePreviewRows: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  renderMultiRenameExamples: vi.fn(() => Promise.resolve([])),
  applyMultiRename: vi.fn(() => Promise.resolve({ ok: false, error: { type: 'nothingToRename' } })),
  getMultiRenamePresets: vi.fn(() => Promise.resolve([{ id: 'p1', name: 'Bez diakritiky', spec: {} }])),
  saveMultiRenamePreset: vi.fn(() => Promise.resolve()),
  deleteMultiRenamePreset: vi.fn(() => Promise.resolve()),
  getMultiRenameLastSettings,
  rollbackOperation: vi.fn(() => Promise.resolve({ inverseOpId: 'inv' })),
  saveMultiRenameLastSettings: vi.fn(() => Promise.resolve()),
  getMultiRenameHistory: vi.fn(() => Promise.resolve([])),
  writeMultiRenameNames: vi.fn(() => Promise.resolve({ ok: true, value: '/tmp/S.txt' })),
  readMultiRenameNames: vi.fn(() => Promise.resolve({ ok: true, value: 1 })),
}))

vi.mock('$lib/text-editor/open-file-in-editor', () => ({ openFileInEditor: vi.fn(() => Promise.resolve(true)) }))

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
      onUndoStarted: () => {},
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

  it('with every whole-name option on and Undo rename offered has no violations', async () => {
    previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 1, counts: { ready: 1, unchanged: 1, problems: 3 }, rows: ROWS },
    })
    getMultiRenameLastSettings.mockResolvedValueOnce({
      spec: { ...DEFAULT_SPEC, greekToLatin: true, removeDiacritics: true, normalizeUnicode: true },
      preset: null,
    })
    setLastMultiRenameRun({ operationId: 'op1', renaming: 2 })
    await expectNoA11yViolations(await mountSheet())
    setLastMultiRenameRun(null)
  })

  it('with a name typed in Results, marked in its row, and the notice showing has no violations', async () => {
    const edited = [{ ...ROWS[0], edited: true }, ...ROWS.slice(1)]
    previewMultiRename.mockResolvedValue({
      ok: true,
      value: { previewId: 1, counts: { ready: 1, unchanged: 1, problems: 3 }, rows: edited },
    })
    const root = await mountSheet()
    root
      .querySelector<HTMLInputElement>('input')
      ?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', altKey: true, bubbles: true, cancelable: true }),
      )
    await new Promise((resolve) => setTimeout(resolve, 0))
    window.dispatchEvent(new FocusEvent('focus'))
    await new Promise((resolve) => setTimeout(resolve, 0))
    await tick()
    expect(root.querySelector('.results-notice')).not.toBeNull()
    expect(root.querySelector('.edited-mark')).not.toBeNull()
    await expectNoA11yViolations(root)
  })

  it('search option chips, some on, with a tooltip showing, have no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const pic = `${MARK_START}pic${MARK_END}`
    const rendered = new Map([
      [searchExampleKey('caseSensitive', true), `Photo ${pic}.jpg`],
      [searchExampleKey('caseSensitive', false), `${pic} ${pic}.jpg`],
    ])
    mount(SearchOptionChips, {
      target,
      props: { spec: { ...DEFAULT_SPEC, caseSensitive: true, regex: true }, onToggle: () => {}, rendered },
    })
    await tick()
    target.querySelector<HTMLButtonElement>('button')?.focus()
    await new Promise((resolve) => setTimeout(resolve, 450))
    await expectNoA11yViolations(document.body)
  })

  it('a placeholder tooltip body with its examples has no violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const help = PLACEHOLDER_HELP[1]
    const rendered = new Map([
      [hintMask({ mask: '[E]' }), `${MARK_START}jpg${MARK_END}`],
      [hintMask(help.syntax[1]), `j${MARK_START}pg${MARK_END}`],
    ])
    mount(PlaceholderTip, { target, props: { help, rendered } })
    await tick()
    // The body lives in a hidden host until a tooltip adopts it; show it to check it.
    const host = target.querySelector<HTMLElement>('[hidden]')
    host?.removeAttribute('hidden')
    await expectNoA11yViolations(target)
  })
})
