import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushSync } from 'svelte'

vi.mock('./ai-state.svelte', () => ({
  getAiState: vi.fn(),
  handleCancel: vi.fn(),
  handleGotIt: vi.fn(),
}))

import { getAiState, handleCancel, handleGotIt } from './ai-state.svelte'
import AiToastContent from './AiToastContent.svelte'

type AiNotificationState = 'hidden' | 'downloading' | 'installing' | 'ready' | 'starting'

let mockState = {
  notificationState: 'hidden' as AiNotificationState,
  downloadProgress: null as { bytesDownloaded: number; totalBytes: number; speed: number; etaSeconds: number } | null,
  progressText: '',
  modelInfo: {
    id: 'ministral-3b-instruct-q4km',
    displayName: 'Ministral 3B',
    sizeBytes: 2147023008,
    sizeFormatted: '2.1 GB',
    kvBytesPerToken: 106496,
    baseOverheadBytes: 3500000000,
  },
  downloadToastUserDismissed: false,
}

function mountToast() {
  const target = document.createElement('div')
  mount(AiToastContent, { target })
  return target
}

describe('AiToastContent', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockState = {
      notificationState: 'hidden',
      downloadProgress: null,
      progressText: '',
      modelInfo: {
        id: 'ministral-3b-instruct-q4km',
        displayName: 'Ministral 3B',
        sizeBytes: 2147023008,
        sizeFormatted: '2.1 GB',
        kvBytesPerToken: 106496,
        baseOverheadBytes: 3500000000,
      },
      downloadToastUserDismissed: false,
    }
    vi.mocked(getAiState).mockReturnValue(mockState)
  })

  it('renders nothing when state is hidden', () => {
    const target = mountToast()
    expect(target.querySelector('.ai-content')).toBeNull()
  })

  it('renders downloading state with progress text', () => {
    mockState.notificationState = 'downloading'
    mockState.downloadProgress = { bytesDownloaded: 500000, totalBytes: 4000000, speed: 100000, etaSeconds: 35 }
    mockState.progressText = '12% · 500.0 KB / 4.0 MB · 100.0 KB/s · 35s remaining'

    const target = mountToast()

    const title = target.querySelector('.ai-title')
    expect(title?.textContent).toBe('Downloading AI model...')

    const progressText = target.querySelector('.ai-progress-text')
    expect(progressText?.textContent).toContain('12%')
  })

  it('renders downloading state with "Starting download..." when no total', () => {
    mockState.notificationState = 'downloading'
    mockState.downloadProgress = null

    const target = mountToast()

    const progressText = target.querySelector('.ai-progress-text')
    expect(progressText?.textContent).toBe('Starting download...')
  })

  it('calls handleCancel when Cancel is clicked in downloading state', () => {
    mockState.notificationState = 'downloading'
    mockState.downloadProgress = { bytesDownloaded: 100, totalBytes: 1000, speed: 50, etaSeconds: 18 }

    const target = mountToast()

    const cancelButton = target.querySelector('.btn-secondary') as HTMLButtonElement
    cancelButton.click()
    flushSync()

    expect(handleCancel).toHaveBeenCalledOnce()
  })

  it('renders installing state', () => {
    mockState.notificationState = 'installing'

    const target = mountToast()

    const title = target.querySelector('.ai-title')
    expect(title?.textContent).toBe('Setting up AI...')

    const description = target.querySelector('.ai-description')
    expect(description?.textContent).toBe('Starting server')

    // No buttons in installing state
    expect(target.querySelectorAll('.ai-actions button')).toHaveLength(0)
  })

  it('renders ready state with Got it button', () => {
    mockState.notificationState = 'ready'

    const target = mountToast()

    const title = target.querySelector('.ai-title')
    expect(title?.textContent).toBe('AI ready')

    const button = target.querySelector('.btn-primary') as HTMLButtonElement
    expect(button.textContent).toBe('Got it')
  })

  it('calls handleGotIt when Got it is clicked', () => {
    mockState.notificationState = 'ready'

    const target = mountToast()

    const button = target.querySelector('.btn-primary') as HTMLButtonElement
    button.click()
    flushSync()

    expect(handleGotIt).toHaveBeenCalledOnce()
  })

  it('shows progress bar when downloading with known total', () => {
    mockState.notificationState = 'downloading'
    mockState.downloadProgress = { bytesDownloaded: 2000000, totalBytes: 4000000, speed: 100000, etaSeconds: 20 }
    mockState.progressText = '50% · 2.0 MB / 4.0 MB'

    const target = mountToast()

    const progressBar = target.querySelector('.progress-bar-fill') as HTMLElement
    expect(progressBar).not.toBeNull()
    expect(progressBar.style.width).toBe('50%')
  })
})
