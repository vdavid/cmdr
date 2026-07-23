/**
 * Tier 3 a11y + visibility tests for `MediaIndexClipModel.svelte` (the Semantic search card
 * body: the on/off toggle plus the on-device CLIP model download/delete controls).
 *
 * The toggle always renders (disabled on unsupported hardware, with an explanation). The
 * model controls below reveal off the mocked status + the toggle: a download button when
 * supported-and-not-installed, a ready line + delete button when installed. Each visible
 * state must be accessible. The download/delete round-trips are backend work, mocked here.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { ClipModelStatus } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const settingValues: Record<string, unknown> = {
  'mediaIndex.semanticSearch.enabled': true,
}

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string) => settingValues[id],
  setSetting: vi.fn(),
  onSpecificSettingChange: () => () => {},
}))

const clipModelStatus = vi.fn<() => Promise<ClipModelStatus>>()
vi.mock('$lib/tauri-commands', () => ({
  mediaIndexClipModelStatus: () => clipModelStatus(),
  mediaIndexDownloadClipModel: vi.fn(),
  mediaIndexDeleteClipModel: vi.fn(),
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({ formatFileSize: (b: number) => `${String(b)}B` }))

const { default: MediaIndexClipModel } = await import('./MediaIndexClipModel.svelte')

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
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexClipModel, { target, props: {} })
  flushSync()
  await vi.waitFor(() => {
    // Let the onMount status fetch resolve.
    expect(clipModelStatus).toHaveBeenCalled()
  })
  await tick()
  return target
}

describe('MediaIndexClipModel', () => {
  beforeEach(() => {
    settingValues['mediaIndex.semanticSearch.enabled'] = true
    clipModelStatus.mockResolvedValue(status())
  })
  afterEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('offers an accessible download button when supported, on, but not installed', async () => {
    const target = await mountClipModel()
    expect(target.querySelector('.clip-model button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows an accessible ready line and delete button once the model is installed', async () => {
    clipModelStatus.mockResolvedValue(status({ installed: true }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
    // The one button in the state block is now Delete, not Download.
    expect(target.querySelector('.clip-model button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows a "coming soon" note when the model is not published yet', async () => {
    clipModelStatus.mockResolvedValue(status({ configured: false }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-note')).not.toBeNull()
    expect(target.querySelector('.clip-model button')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('disables the toggle with an explanation on unsupported hardware', async () => {
    clipModelStatus.mockResolvedValue(status({ supported: false }))
    const target = await mountClipModel()
    // No model-management block, but the not-supported note renders.
    expect(target.querySelector('.clip-model')).toBeNull()
    expect(target.querySelector('.cm-note')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
