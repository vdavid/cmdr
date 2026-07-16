/**
 * Tier 3 a11y + visibility tests for `MediaIndexClipModel.svelte` (the on-device CLIP
 * semantic-search model control inside the "Image search" settings card).
 *
 * The control renders nothing on unsupported hardware, and otherwise shows one of a ready
 * line, a "coming soon" note, or a download button — each driven off a mocked status. The
 * download round-trip is backend work; here we pin that each visible state is accessible and
 * that unsupported hardware renders nothing.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { ClipModelStatus } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const clipModelStatus = vi.fn<() => Promise<ClipModelStatus>>()

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexClipModelStatus: () => clipModelStatus(),
  mediaIndexDownloadClipModel: vi.fn(),
}))

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
  mount(MediaIndexClipModel, { target })
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
    clipModelStatus.mockResolvedValue(status())
  })
  afterEach(() => {
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('offers an accessible download button when supported but not installed', async () => {
    const target = await mountClipModel()
    expect(target.querySelector('button.cm-download')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows an accessible ready line once the model is installed', async () => {
    clipModelStatus.mockResolvedValue(status({ installed: true }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
    expect(target.querySelector('button.cm-download')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('shows a "coming soon" note when the model is not published yet', async () => {
    clipModelStatus.mockResolvedValue(status({ configured: false }))
    const target = await mountClipModel()
    expect(target.querySelector('.cm-note')).not.toBeNull()
    expect(target.querySelector('button.cm-download')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('renders nothing on unsupported hardware', async () => {
    clipModelStatus.mockResolvedValue(status({ supported: false }))
    const target = await mountClipModel()
    expect(target.querySelector('.clip-model')).toBeNull()
  })
})
