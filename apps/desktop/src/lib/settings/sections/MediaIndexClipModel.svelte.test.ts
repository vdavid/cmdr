/**
 * Tests for `MediaIndexClipModel.svelte`: the CLIP semantic-search model control in the
 * Image search settings card. It self-gates on Apple Silicon support, shows install state
 * (ready / coming soon / a "Download (~X MB)" button), and downloads + installs on click.
 * Tauri is mocked so the test runs with no runtime.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { statusMock, downloadMock } = vi.hoisted(() => ({
  statusMock: vi.fn(),
  downloadMock: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexClipModelStatus: statusMock,
  mediaIndexDownloadClipModel: downloadMock,
}))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/intl/number-format', () => ({ formatInteger: (n: number) => String(n) }))

import MediaIndexClipModel from './MediaIndexClipModel.svelte'

interface Status {
  supported: boolean
  installed: boolean
  configured: boolean
  downloadBytes: number
}

function status(overrides: Partial<Status> = {}): Status {
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
  statusMock.mockReset()
  downloadMock.mockReset()
})

describe('MediaIndexClipModel', () => {
  it('renders nothing on unsupported hardware', async () => {
    statusMock.mockResolvedValue(status({ supported: false }))
    const target = await mountModel()
    expect(target.querySelector('.clip-model')).toBeNull()
  })

  it('shows a download button with the honest size when configured but not installed', async () => {
    statusMock.mockResolvedValue(status())
    const target = await mountModel()
    // Configured + not installed ⇒ the download button is shown (the "~X MB" size is
    // interpolated into the message, resolved by the real `tString` at runtime).
    expect(target.querySelector('.cm-download')).not.toBeNull()
    expect(target.querySelector('.cm-ready')).toBeNull()
  })

  it('shows "coming soon" when no artifact is published yet', async () => {
    statusMock.mockResolvedValue(status({ configured: false }))
    const target = await mountModel()
    expect(target.querySelector('.cm-download')).toBeNull()
    expect(target.querySelector('.cm-note')).not.toBeNull()
  })

  it('shows the ready line once installed', async () => {
    statusMock.mockResolvedValue(status({ installed: true }))
    const target = await mountModel()
    expect(target.querySelector('.cm-ready')).not.toBeNull()
    expect(target.querySelector('.cm-download')).toBeNull()
  })

  it('downloads on click and refreshes to the installed state', async () => {
    // First status call: downloadable; after a successful download, installed.
    statusMock.mockResolvedValueOnce(status()).mockResolvedValueOnce(status({ installed: true }))
    downloadMock.mockResolvedValue(undefined)
    const target = await mountModel()

    const btn = target.querySelector<HTMLButtonElement>('.cm-download')
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

    target.querySelector<HTMLButtonElement>('.cm-download')?.click()
    await flush()

    expect(target.querySelector('.cm-failed')).not.toBeNull()
  })
})
