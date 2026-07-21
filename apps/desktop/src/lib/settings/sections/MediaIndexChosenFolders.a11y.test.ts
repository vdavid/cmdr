/**
 * Tier 3 a11y + behavior tests for `MediaIndexChosenFolders.svelte` (the list of folders
 * the user picked for image indexing).
 *
 * Covers the empty state, a rendered row, adding through the native folder picker,
 * removing a row, and the two paths that must NOT write: a cancelled picker and a folder
 * that's already on the list.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let chosen: string[] = []
const openPicker = vi.fn<() => Promise<string | string[] | null>>()
const setFolderChosen = vi.fn<(folder: string, chosen: boolean) => Promise<void>>()

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: () => openPicker(),
}))

vi.mock('$lib/settings', () => ({
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/media-index/always-index-folders', () => ({
  getChosenFolders: () => chosen,
  isFolderChosen: (folder: string) => chosen.includes(folder),
  setFolderChosen: (folder: string, on: boolean) => setFolderChosen(folder, on),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const { default: MediaIndexChosenFolders } = await import('./MediaIndexChosenFolders.svelte')

function mountList(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexChosenFolders, { target })
  flushSync()
  return target
}

/** The "Add a folder…" button (the only regular-size button in this component). */
function addButton(target: HTMLElement): HTMLElement {
  return target.querySelector('.mi-folders-actions button') as HTMLElement
}

describe('MediaIndexChosenFolders', () => {
  beforeEach(() => {
    chosen = []
    setFolderChosen.mockResolvedValue(undefined)
  })

  afterEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('says the list is empty and offers to add, with no a11y violations', async () => {
    const target = mountList()
    expect(target.querySelector('.mi-folders-empty')?.textContent ?? '').toContain('No folders yet')
    expect(target.querySelector('.mi-folders-list')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows each chosen folder by name and full path', async () => {
    chosen = ['/Users/dave/Photos', '/Volumes/naspi/Archive']
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
    openPicker.mockResolvedValue('/Users/dave/Photos')
    const target = mountList()
    addButton(target).click()
    await vi.waitFor(() => {
      expect(setFolderChosen).toHaveBeenCalledWith('/Users/dave/Photos', true)
    })
  })

  it('writes nothing when the picker is cancelled', async () => {
    openPicker.mockResolvedValue(null)
    const target = mountList()
    addButton(target).click()
    await tick()
    expect(setFolderChosen).not.toHaveBeenCalled()
  })

  it('writes nothing when the folder is already on the list', async () => {
    // The backend stores a set, so a re-add is a no-op there — but it would render a
    // duplicate row, so the component drops it first.
    chosen = ['/Users/dave/Photos']
    openPicker.mockResolvedValue('/Users/dave/Photos')
    const target = mountList()
    addButton(target).click()
    await tick()
    expect(setFolderChosen).not.toHaveBeenCalled()
  })

  it('removes the folder on its row', async () => {
    chosen = ['/Users/dave/Photos']
    const target = mountList()
    const remove = target.querySelector('.mi-folders-row button') as HTMLElement
    remove.click()
    await vi.waitFor(() => {
      expect(setFolderChosen).toHaveBeenCalledWith('/Users/dave/Photos', false)
    })
  })
})
