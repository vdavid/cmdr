/** The post-onboarding "local model didn't finish downloading" toast: what it says, and that its
 * one button lands in Settings where the Download button lives. */

import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'

const { openSettingsWindow, dismissToast } = vi.hoisted(() => ({
  openSettingsWindow: vi.fn<(surface: string, section?: string[]) => Promise<void>>(() => Promise.resolve()),
  dismissToast: vi.fn<(id: string) => void>(),
}))
vi.mock('$lib/settings/settings-window', () => ({ openSettingsWindow }))
vi.mock('$lib/ui/toast', () => ({ dismissToast }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import LocalDownloadFailedToastContent from './LocalDownloadFailedToastContent.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
beforeEach(() => {
  vi.clearAllMocks()
})

/** Renders the toast, clicks its button, and returns the text it showed. */
async function renderAndClick(): Promise<string> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const instance = mount(LocalDownloadFailedToastContent, { target, props: { toastId: 'toast-1' } })
  flushSync()
  const text = target.textContent
  target.querySelector('button')?.click()
  for (let i = 0; i < 5; i++) await Promise.resolve()
  await tick()
  void unmount(instance)
  target.remove()
  return text
}

describe('LocalDownloadFailedToastContent', () => {
  it('says the model is not ready and opens Settings at the AI provider, then closes itself', async () => {
    const text = await renderAndClick()

    expect(text).toContain('didn’t finish downloading')
    expect(openSettingsWindow).toHaveBeenCalledWith('local-download-toast', ['AI', 'Provider'])
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })

  it('stays up when Settings would not open, so the button is still there to try again', async () => {
    openSettingsWindow.mockRejectedValueOnce(new Error('no window'))

    await renderAndClick()

    expect(dismissToast).not.toHaveBeenCalled()
  })
})
