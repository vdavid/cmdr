/**
 * Tier 3 a11y + behavior tests for the two leaf cards of the image-index
 * settings: the on-device CLIP model (`MediaIndexClipModel`) and the chosen-folder
 * list (`MediaIndexChosenFolders`).
 *
 * They share a file for the reason every merged a11y file here does: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). They sit apart from `media-index.a11y.test.ts` because that file's
 * volume/IPC harness would push a single merged file past the 800-line
 * `file-length` threshold, and because these two need none of it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { ClipModelStatus } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const stubs = vi.hoisted(() => ({
  getSetting: (_id: string): unknown => undefined,
  clipModelStatus: vi.fn<() => Promise<unknown>>(),
  chosenFolders: [] as string[],
  setFolderChosen: vi.fn<(folder: string, chosen: boolean) => Promise<void>>(),
  openPicker: vi.fn<() => Promise<string | string[] | null>>(),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string) => stubs.getSetting(id),
  setSetting: vi.fn(),
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexClipModelStatus: () => stubs.clipModelStatus(),
  mediaIndexDownloadClipModel: vi.fn(),
  mediaIndexDeleteClipModel: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
  formatFileSize: (b: number) => `${String(b)}B`,
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: () => stubs.openPicker(),
}))

vi.mock('$lib/media-index/always-index-folders', () => ({
  getChosenFolders: () => stubs.chosenFolders,
  isFolderChosen: (folder: string) => stubs.chosenFolders.includes(folder),
  setFolderChosen: (folder: string, on: boolean) => stubs.setFolderChosen(folder, on),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import MediaIndexChosenFolders from './MediaIndexChosenFolders.svelte'
import MediaIndexClipModel from './MediaIndexClipModel.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

afterEach(() => {
  document.body.innerHTML = ''
  vi.clearAllMocks()
})

/**
 * Tier 3 a11y + visibility tests for `MediaIndexClipModel.svelte` (the Semantic search card
 * body: the on/off toggle plus the on-device CLIP model download/delete controls).
 *
 * The toggle always renders (disabled on unsupported hardware, with an explanation). The
 * model controls below reveal off the mocked status + the toggle: a download button when
 * supported-and-not-installed, a ready line + delete button when installed. Each visible
 * state must be accessible. The download/delete round-trips are backend work, mocked here.
 */
describe('MediaIndexClipModel', () => {
  function status(overrides: Partial<ClipModelStatus> = {}): ClipModelStatus {
    return {
      supported: true,
      installed: false,
      configured: true,
      downloadBytes: 350_000_000,
      ...overrides,
    }
  }

  async function mountClipModel(): Promise<HTMLElement> {
    const target = container()
    mount(MediaIndexClipModel, { target, props: {} })
    flushSync()
    await vi.waitFor(() => {
      // Let the onMount status fetch resolve.
      expect(stubs.clipModelStatus).toHaveBeenCalled()
    })
    await tick()
    return target
  }

  beforeEach(() => {
    const settingValues: Record<string, unknown> = { 'mediaIndex.semanticSearch.enabled': true }
    stubs.getSetting = (id: string) => settingValues[id]
    stubs.clipModelStatus.mockResolvedValue(status())
  })

  it('offers an accessible download button when supported, on, but not installed', async () => {
    const target = await mountClipModel()
    expect(target.querySelector('.clip-model button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows an accessible ready line and delete button once the model is installed', async () => {
    stubs.clipModelStatus.mockResolvedValue(status({ installed: true }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
    // The one button in the state block is now Delete, not Download.
    expect(target.querySelector('.clip-model button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows a "coming soon" note when the model is not published yet', async () => {
    stubs.clipModelStatus.mockResolvedValue(status({ configured: false }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-note')).not.toBeNull()
    expect(target.querySelector('.clip-model button')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('disables the toggle with an explanation on unsupported hardware', async () => {
    stubs.clipModelStatus.mockResolvedValue(status({ supported: false }))
    const target = await mountClipModel()
    // No model-management block, but the not-supported note renders.
    expect(target.querySelector('.clip-model')).toBeNull()
    expect(target.querySelector('.cm-note')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y + behavior tests for `MediaIndexChosenFolders.svelte` (the list of folders
 * the user picked for image indexing).
 *
 * Covers the empty state, a rendered row, adding through the native folder picker,
 * removing a row, and the two paths that must NOT write: a cancelled picker and a folder
 * that's already on the list.
 */
describe('MediaIndexChosenFolders', () => {
  function mountList(): HTMLElement {
    const target = container()
    mount(MediaIndexChosenFolders, { target, props: {} })
    flushSync()
    return target
  }

  /** The "Add a folder…" button (the only regular-size button in this component). */
  function addButton(target: HTMLElement): HTMLElement {
    return target.querySelector('.mi-folders-actions button') as HTMLElement
  }

  beforeEach(() => {
    // The component's own file mocked `$lib/settings` down to
    // `onSpecificSettingChange` alone, so nothing here reads a setting.
    stubs.getSetting = () => undefined
    stubs.chosenFolders = []
    stubs.setFolderChosen.mockResolvedValue(undefined)
  })

  it('says the list is empty and offers to add, with no a11y violations', async () => {
    const target = mountList()
    expect(target.querySelector('.mi-folders-empty')?.textContent ?? '').toContain('No folders yet')
    expect(target.querySelector('.mi-folders-list')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows each chosen folder by name and full path', async () => {
    stubs.chosenFolders = ['/Users/dave/Photos', '/Volumes/naspi/Archive']
    const target = mountList()
    const rows = target.querySelectorAll('.mi-folders-row')
    expect(rows.length).toBe(2)
    expect(rows[0].querySelector('.mi-folders-name')?.textContent).toBe('Photos')
    expect(rows[0].querySelector('.mi-folders-full')?.textContent).toBe('/Users/dave/Photos')
    // The remove button names its folder, so a screen reader hears which row it acts on.
    const remove = rows[1].querySelector('button')
    expect(remove?.getAttribute('aria-label') ?? '').toContain('/Volumes/naspi/Archive')
    await expectNoA11yViolations(target)
  })

  it('adds the picked folder', async () => {
    stubs.openPicker.mockResolvedValue('/Users/dave/Photos')
    const target = mountList()
    addButton(target).click()
    await vi.waitFor(() => {
      expect(stubs.setFolderChosen).toHaveBeenCalledWith('/Users/dave/Photos', true)
    })
  })

  it('writes nothing when the picker is cancelled', async () => {
    stubs.openPicker.mockResolvedValue(null)
    const target = mountList()
    addButton(target).click()
    await tick()
    expect(stubs.setFolderChosen).not.toHaveBeenCalled()
  })

  it('writes nothing when the folder is already on the list', async () => {
    // The backend stores a set, so a re-add is a no-op there — but it would render a
    // duplicate row, so the component drops it first.
    stubs.chosenFolders = ['/Users/dave/Photos']
    stubs.openPicker.mockResolvedValue('/Users/dave/Photos')
    const target = mountList()
    addButton(target).click()
    await tick()
    expect(stubs.setFolderChosen).not.toHaveBeenCalled()
  })

  it('removes the folder on its row', async () => {
    stubs.chosenFolders = ['/Users/dave/Photos']
    const target = mountList()
    const remove = target.querySelector('.mi-folders-row button') as HTMLElement
    remove.click()
    await vi.waitFor(() => {
      expect(stubs.setFolderChosen).toHaveBeenCalledWith('/Users/dave/Photos', false)
    })
  })
})
