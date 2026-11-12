/**
 * The toast "Go to latest download" raises when Cmdr isn't watching the Downloads
 * folder. On macOS the cause is a missing Full Disk Access grant, and the toast
 * offers the pane that fixes it. Windows and Linux have no such permission: there
 * the watcher is down because it couldn't attach to the folder, so the toast says
 * that and offers nothing to open.
 */
import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import { mount, tick } from 'svelte'

const { dismissToastMock, openPrivacySettingsMock } = vi.hoisted(() => ({
  dismissToastMock: vi.fn(),
  openPrivacySettingsMock: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: dismissToastMock }))
vi.mock('#lib/tauri-commands/index.js', () => ({ openPrivacySettings: openPrivacySettingsMock }))

import { withCapabilities, withPlatform } from '#lib/platform/test-platform.js'
import LatestDownloadFdaToastContent from './LatestDownloadFdaToastContent.svelte'

async function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(LatestDownloadFdaToastContent, { target, props: {} })
  await tick()
  return target
}

function buttonLabels(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('button')).map((b) => b.textContent.trim())
}

describe('LatestDownloadFdaToastContent', () => {
  beforeEach(() => {
    dismissToastMock.mockReset()
    openPrivacySettingsMock.mockClear()
    document.body.innerHTML = ''
  })

  it('asks for Full Disk Access and opens its pane on macOS', async () => {
    onTestFinished(withPlatform('macos'))
    onTestFinished(withCapabilities('fullDiskAccess'))
    const target = await render()

    expect(target.textContent).toContain('Cmdr needs Full Disk Access to watch your Downloads folder.')
    expect(buttonLabels(target)).toEqual(['Dismiss', 'Open System Settings'])
    Array.from(target.querySelectorAll('button'))[1].click()
    expect(openPrivacySettingsMock).toHaveBeenCalledOnce()
  })

  it.each(['linux', 'windows'] as const)(
    'says what to check on %s, with no permission to grant and nothing to open',
    async (platform) => {
      onTestFinished(withPlatform(platform))
      const target = await render()

      expect(target.textContent).toContain(
        'Cmdr can’t watch your Downloads folder. Check that the folder exists and that you can open it.',
      )
      expect(target.textContent).not.toContain('Full Disk Access')
      expect(buttonLabels(target)).toEqual(['Dismiss'])
    },
  )
})
