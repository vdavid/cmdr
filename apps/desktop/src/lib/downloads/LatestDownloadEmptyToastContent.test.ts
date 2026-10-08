import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { dismissToastMock } = vi.hoisted(() => ({ dismissToastMock: vi.fn() }))

vi.mock('$lib/ui/toast', () => ({
  dismissToast: dismissToastMock,
}))

import LatestDownloadEmptyToastContent from './LatestDownloadEmptyToastContent.svelte'

async function render(onGoToDownloads: () => void = () => {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(LatestDownloadEmptyToastContent, { target, props: { toastId: 'downloads:empty', onGoToDownloads } })
  await tick()
  return target
}

function button(target: HTMLElement, label: string): HTMLButtonElement {
  const match = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === label)
  if (!match) throw new Error(`no "${label}" button`)
  return match
}

describe('LatestDownloadEmptyToastContent', () => {
  beforeEach(() => {
    dismissToastMock.mockReset()
    document.body.innerHTML = ''
  })

  it('tells the user the Downloads folder is empty', async () => {
    const target = await render()
    expect(target.textContent).toContain('Your Downloads folder is empty')
  })

  it('"Go to Downloads" runs the captured navigation, then closes its own toast', async () => {
    const onGoToDownloads = vi.fn()
    const target = await render(onGoToDownloads)

    button(target, 'Go to Downloads').click()

    expect(onGoToDownloads).toHaveBeenCalledOnce()
    expect(dismissToastMock).toHaveBeenCalledWith('downloads:empty')
  })

  it('"Dismiss" closes the toast without navigating', async () => {
    const onGoToDownloads = vi.fn()
    const target = await render(onGoToDownloads)

    button(target, 'Dismiss').click()

    expect(onGoToDownloads).not.toHaveBeenCalled()
    expect(dismissToastMock).toHaveBeenCalledWith('downloads:empty')
  })
})
