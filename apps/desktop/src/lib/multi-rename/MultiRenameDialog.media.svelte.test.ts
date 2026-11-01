/**
 * The `media ▾` button and its field picker, the sheet side: ⌘⌥X or the button opens it on each
 * field's value from the files, the group more files have first; a click or Space inserts at the
 * name mask's caret and keeps it open, consecutive picks get a space between them, a picked field
 * shows a ✓ and is announced; Enter inserts and closes, Esc closes back to the mask, Tab moves on.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import MultiRenameDialog from './MultiRenameDialog.svelte'
import { setLastMultiRenameRun } from './last-run.svelte'
import { MEDIA_FIELDS } from './media-token'
import type { MediaField } from '#lib/tauri-commands/index.js'

const ipc = vi.hoisted(() => ({
  previewMultiRename: vi.fn(),
  getMultiRenamePreviewRows: vi.fn(),
  getMultiRenameRangeSamples: vi.fn(() => Promise.resolve({ ok: true, value: [] })),
  renderMultiRenameExamples: vi.fn((examples: unknown[]) => Promise.resolve(examples.map(() => null))),
  applyMultiRename: vi.fn(),
  getMultiRenamePresets: vi.fn(() => Promise.resolve([])),
  getMultiRenameLastSettings: vi.fn(() => Promise.resolve(null)),
  saveMultiRenameLastSettings: vi.fn(() => Promise.resolve()),
  getMultiRenameHistory: vi.fn(() => Promise.resolve([])),
  rollbackOperation: vi.fn(),
  getMultiRenameMediaProgress: vi.fn(() => Promise.resolve({ ok: true, value: { read: 2, total: 2, done: true } })),
  getMultiRenameMediaOverview: vi.fn(),
  onMultiRenameMediaProgress: vi.fn(() => Promise.resolve(() => {})),
  prioritizeMultiRenameMedia: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  ...ipc,
}))

const PREVIEW = { previewId: 1, counts: { ready: 0, unchanged: 0, problems: 0, reading: 0 }, rows: [] }

/** Two songs and no photos: music comes first. */
function overview(): unknown {
  const values: Partial<Record<MediaField, string>> = { tagArtist: 'Daft Punk', tagTitle: 'Digital Love' }
  return {
    ok: true,
    value: {
      progress: { read: 2, total: 2, done: true },
      fields: (Object.keys(MEDIA_FIELDS) as MediaField[]).map((field) => ({
        field,
        example: values[field] ?? null,
        filesWithIt: values[field] ? 2 : 0,
      })),
    },
  }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await tick()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
}

let mounted: ReturnType<typeof mount>[] = []

async function mountSheet(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted.push(
    mount(MultiRenameDialog, {
      target,
      props: {
        session: { sessionId: 'S', count: 2 },
        onApplied: vi.fn(),
        onUndoStarted: vi.fn(),
        onClose: vi.fn(),
      },
    }),
  )
  await settle()
  return target
}

function nameMask(root: HTMLElement): HTMLInputElement {
  const input = root.querySelector<HTMLInputElement>('input:not([type="checkbox"])')
  if (!input) throw new Error('no name mask')
  return input
}

function mediaButton(root: HTMLElement): HTMLButtonElement {
  const button = root.querySelector<HTMLButtonElement>('button[aria-haspopup="dialog"].chip')
  if (!button) throw new Error('no media button')
  return button
}

function filterBox(): HTMLInputElement {
  const input = document.querySelector<HTMLInputElement>('.ui-popover input[role="combobox"]')
  if (!input) throw new Error('no picker')
  return input
}

function press(el: Element, key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  el.dispatchEvent(event)
  return event
}

function activeOption(): HTMLElement | null {
  const id = filterBox().getAttribute('aria-activedescendant')
  return id ? document.getElementById(id) : document.querySelector('[role="option"][aria-selected="true"]')
}

async function openPicker(root: HTMLElement): Promise<void> {
  mediaButton(root).click()
  await settle()
}

describe('the media field picker', () => {
  // The registry reads ⌘ as ⌘ only on a Mac, and happy-dom reports a Linux UA.
  const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')
  beforeEach(() => {
    navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
    vi.clearAllMocks()
    ipc.previewMultiRename.mockResolvedValue({ ok: true, value: PREVIEW })
    ipc.getMultiRenameMediaOverview.mockResolvedValue(overview())
    setLastMultiRenameRun(null)
  })

  afterEach(() => {
    navigatorSpy.mockReset()
    for (const app of mounted) void unmount(app)
    mounted = []
    document.body.innerHTML = ''
  })

  it('opens on ⌘⌥X, music first when the files are songs, each field with its value or none', async () => {
    const root = await mountSheet()
    press(nameMask(root), '≈', { code: 'KeyX', metaKey: true, altKey: true })
    await settle()
    const groups = [...document.querySelectorAll('[role="listbox"] [role="group"]')].map((g) =>
      g.getAttribute('aria-label'),
    )
    expect(groups).toEqual(['Music (ID3, MP4, FLAC…)', 'Photos and videos (EXIF)'])
    expect(document.getElementById('media-field-tagArtist')?.textContent).toContain('Daft Punk')
    expect(document.getElementById('media-field-exifTaken')?.textContent).toContain('none in these files')
    expect(document.activeElement).toBe(filterBox())
    expect(activeOption()?.id).toBe('media-field-tagTitle')
  })

  it('inserts at the caret on Space and stays open, a space between consecutive picks, each with a ✓', async () => {
    const root = await mountSheet()
    const mask = nameMask(root)
    mask.setSelectionRange(0, 0)
    await openPicker(root)

    press(filterBox(), 'ArrowDown')
    await settle()
    expect(activeOption()?.id).toBe('media-field-tagArtist')
    press(filterBox(), ' ')
    await settle()
    press(filterBox(), 'ArrowUp')
    await settle()
    press(filterBox(), ' ')
    await settle()

    expect(mask.value).toBe('[=tag.artist] [=tag.title][N]')
    expect(document.querySelector('.ui-popover')).not.toBeNull()
    expect(document.getElementById('media-field-tagArtist')?.textContent).toContain('inserted')
    expect(document.getElementById('media-field-tagTitle')?.textContent).toContain('inserted')
    await vi.waitFor(
      () => {
        const spoken = [...root.querySelectorAll('[data-live-announcer]')].map((region) => region.textContent)
        expect(spoken).toContain('Inserted Title')
      },
      { timeout: 4000 },
    )
  })

  it('a click inserts and keeps it open; Enter inserts and closes back to the mask', async () => {
    const root = await mountSheet()
    const mask = nameMask(root)
    mask.setSelectionRange(3, 3)
    await openPicker(root)

    document.getElementById('media-field-tagAlbum')?.click()
    await settle()
    expect(document.querySelector('.ui-popover')).not.toBeNull()

    filterBox().value = 'year'
    filterBox().dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    expect([...document.querySelectorAll('.ui-popover [role="option"]')].map((o) => o.id)).toEqual([
      'media-field-tagYear',
    ])
    const enter = press(filterBox(), 'Enter')
    await settle()

    expect(enter.defaultPrevented).toBe(true)
    expect(mask.value).toBe('[N][=tag.album] [=tag.year]')
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(document.activeElement).toBe(mask)
    expect(ipc.applyMultiRename).not.toHaveBeenCalled()
  })

  it('Esc closes back to the mask and leaves the sheet open; Tab moves on past the button', async () => {
    const root = await mountSheet()
    await openPicker(root)
    press(filterBox(), 'Escape')
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    expect(document.activeElement).toBe(nameMask(root))
    expect(root.querySelector('[role="dialog"]')).not.toBeNull()

    await openPicker(root)
    press(filterBox(), 'Tab')
    await settle()
    expect(document.querySelector('.ui-popover')).toBeNull()
    const after = document.activeElement
    expect(after).not.toBe(mediaButton(root))
    expect(mediaButton(root).compareDocumentPosition(after as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()

    await openPicker(root)
    press(filterBox(), 'Tab', { shiftKey: true })
    await settle()
    expect(
      mediaButton(root).compareDocumentPosition(document.activeElement as Node) & Node.DOCUMENT_POSITION_PRECEDING,
    ).toBeTruthy()
  })

  it('Space types into a filter that has text, so a phrase like “album a” filters', async () => {
    const root = await mountSheet()
    const mask = nameMask(root)
    await openPicker(root)
    filterBox().value = 'album'
    filterBox().dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    const space = press(filterBox(), ' ')
    await settle()
    expect(space.defaultPrevented).toBe(false)
    expect(mask.value).toBe('[N]')
  })

  it('says when no field matches the filter', async () => {
    const root = await mountSheet()
    await openPicker(root)
    filterBox().value = 'zzz'
    filterBox().dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
    expect(document.querySelector('.ui-popover')?.textContent).toContain('No field matches “zzz”')
  })
})
