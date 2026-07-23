/**
 * Tests for `MediaIndexClipModel.svelte`: the Semantic search card body. It self-gates on
 * Apple Silicon support, shows the on/off toggle, and (when on) the model state: a
 * "Download (~X MB)" button when not installed, a ready line + "Delete model" button when
 * installed. Downloads/deletes on click and refreshes. Tauri, confirm, and settings are
 * mocked so the test runs with no runtime.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { ClipModelStatus } from '$lib/ipc/bindings'
// Static import so the `no-isolated-tests` lint sees the exercised source; `vi.mock`
// below is hoisted above it, so the component still resolves the mocked dependencies.
import MediaIndexClipModel from './MediaIndexClipModel.svelte'

const settingValues: Record<string, unknown> = {
  'mediaIndex.semanticSearch.enabled': true,
}
vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string) => settingValues[id],
  setSetting: vi.fn(),
  onSpecificSettingChange: () => () => {},
}))

const { statusMock, downloadMock, deleteMock, confirmMock } = vi.hoisted(() => ({
  statusMock: vi.fn(),
  downloadMock: vi.fn(),
  deleteMock: vi.fn(),
  confirmMock: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexClipModelStatus: statusMock,
  mediaIndexDownloadClipModel: downloadMock,
  mediaIndexDeleteClipModel: deleteMock,
}))
vi.mock('$lib/utils/confirm-dialog', () => ({ confirmDialog: confirmMock }))
vi.mock('$lib/settings/reactive-settings.svelte', () => ({ formatFileSize: (b: number) => `${String(b)}B` }))

function status(overrides: Partial<ClipModelStatus> = {}): ClipModelStatus {
  return { supported: true, installed: false, configured: true, downloadBytes: 392_000_000, ...overrides }
}

async function flush(): Promise<void> {
  await tick()
  await Promise.resolve()
  await Promise.resolve()
  await tick()
}

async function mountModel(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexClipModel, { target, props: {} })
  await flush()
  return target
}

beforeEach(() => {
  settingValues['mediaIndex.semanticSearch.enabled'] = true
  statusMock.mockReset()
  downloadMock.mockReset()
  deleteMock.mockReset()
  confirmMock.mockReset()
})
afterEach(() => {
  document.body.innerHTML = ''
})

describe('MediaIndexClipModel', () => {
  it('renders no model-management block on unsupported hardware', async () => {
    statusMock.mockResolvedValue(status({ supported: false }))
    const target = await mountModel()
    expect(target.querySelector('.clip-model')).toBeNull()
    // The not-supported explanation still renders.
    expect(target.querySelector('.cm-note')).not.toBeNull()
  })

  it('shows a download button when supported, on, but not installed', async () => {
    statusMock.mockResolvedValue(status())
    const target = await mountModel()
    expect(target.querySelector('.clip-model button')).not.toBeNull()
    expect(target.querySelector('.cm-ready')).toBeNull()
  })

  it('hides the download button while the toggle is off', async () => {
    settingValues['mediaIndex.semanticSearch.enabled'] = false
    statusMock.mockResolvedValue(status())
    const target = await mountModel()
    // Off + not installed ⇒ no download button (nothing to manage yet).
    expect(target.querySelector('.clip-model button')).toBeNull()
  })

  it('shows "coming soon" when no artifact is published yet', async () => {
    statusMock.mockResolvedValue(status({ configured: false }))
    const target = await mountModel()
    expect(target.querySelector('.clip-model button')).toBeNull()
    expect(target.querySelector('.cm-note')).not.toBeNull()
  })

  it('shows the ready line and a delete button once installed', async () => {
    statusMock.mockResolvedValue(status({ installed: true }))
    const target = await mountModel()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
    expect(target.querySelector('.clip-model button')).not.toBeNull()
  })

  it('downloads on click and refreshes to the installed state', async () => {
    statusMock.mockResolvedValueOnce(status()).mockResolvedValueOnce(status({ installed: true }))
    downloadMock.mockResolvedValue(undefined)
    const target = await mountModel()

    const btn = target.querySelector<HTMLButtonElement>('.clip-model button')
    expect(btn).not.toBeNull()
    btn?.click()
    await flush()

    expect(downloadMock).toHaveBeenCalledOnce()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
  })

  it('surfaces a failed download without crashing', async () => {
    statusMock.mockResolvedValue(status())
    downloadMock.mockRejectedValue(new Error('network'))
    const target = await mountModel()

    target.querySelector<HTMLButtonElement>('.clip-model button')?.click()
    await flush()

    expect(target.querySelector('.cm-note')).not.toBeNull()
  })

  it('deletes the model after confirmation and refreshes to not-installed', async () => {
    statusMock.mockResolvedValueOnce(status({ installed: true })).mockResolvedValueOnce(status({ installed: false }))
    confirmMock.mockResolvedValue(true)
    deleteMock.mockResolvedValue(undefined)
    const target = await mountModel()

    const btn = target.querySelector<HTMLButtonElement>('.clip-model button')
    expect(btn).not.toBeNull()
    btn?.click()
    await flush()

    expect(confirmMock).toHaveBeenCalledOnce()
    expect(deleteMock).toHaveBeenCalledOnce()
    // Back to not-installed ⇒ the ready line is gone and a download button shows.
    expect(target.querySelector('.cm-ready')).toBeNull()
    expect(target.querySelector('.clip-model button')).not.toBeNull()
  })

  it('does not delete when the confirmation is dismissed', async () => {
    statusMock.mockResolvedValue(status({ installed: true }))
    confirmMock.mockResolvedValue(false)
    const target = await mountModel()

    target.querySelector<HTMLButtonElement>('.clip-model button')?.click()
    await flush()

    expect(confirmMock).toHaveBeenCalledOnce()
    expect(deleteMock).not.toHaveBeenCalled()
  })
})
