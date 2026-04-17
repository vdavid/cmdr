/**
 * Tier 3 a11y tests for `AiToastContent.svelte`.
 *
 * The component renders one of five notification states driven by the
 * module-level `aiState` in `./ai-state.svelte`. Each state shows a
 * different combination of title, description, progress bar, and
 * actions. We mutate the state directly via `resetForTesting` plus the
 * exported `getAiState()` reference and remount for each case.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import AiToastContent from './AiToastContent.svelte'
import { getAiState, resetForTesting } from './ai-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  cancelAiDownload: vi.fn(() => Promise.resolve()),
  dismissAiOffer: vi.fn(() => Promise.resolve()),
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
  formatDuration: vi.fn((s: number) => `${String(s)}s`),
  getAiModelInfo: vi.fn(() => Promise.resolve({ sizeFormatted: '~2 GB' })),
  getAiStatus: vi.fn(() => Promise.resolve('offer')),
  optOutAi: vi.fn(() => Promise.resolve()),
  startAiDownload: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 'local'),
  setSetting: vi.fn(),
}))

describe('AiToastContent a11y', () => {
  beforeEach(() => {
    resetForTesting()
  })

  it('offer state has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'offer'
    state.modelInfo = { sizeFormatted: '~2 GB' } as unknown as typeof state.modelInfo

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('downloading state with progress has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'downloading'
    state.downloadProgress = {
      bytesDownloaded: 500_000_000,
      totalBytes: 2_000_000_000,
      speed: 10_000_000,
      etaSeconds: 150,
    }
    state.progressText = '25% — 500 MB / 2 GB — 10 MB/s — 2m 30s remaining'

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('downloading state before progress data has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'downloading'
    state.downloadProgress = null

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('installing state has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'installing'

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('ready state has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'ready'

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('starting state has no a11y violations', async () => {
    const state = getAiState()
    state.notificationState = 'starting'

    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AiToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
