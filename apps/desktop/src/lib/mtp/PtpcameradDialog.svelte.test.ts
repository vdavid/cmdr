/**
 * Rendering tests for `PtpcameradDialog.svelte`.
 *
 * The named-process sentence goes through `<Trans>`, which merges the tag
 * snippets into the interpolation params. A tag whose name equals a param name
 * therefore overwrites the param, and the sentence renders the stringified
 * handler instead of the process name.
 */

import { describe, it, expect, vi, onTestFinished } from 'vitest'
import { mount, unmount, flushSync, tick } from 'svelte'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getPtpcameradWorkaroundCommand } from '#lib/tauri-commands/index.js'
import PtpcameradDialog from './PtpcameradDialog.svelte'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
  getPtpcameradWorkaroundCommand: vi.fn(() =>
    Promise.resolve('sudo launchctl kickstart -k gui/501/com.apple.ptpcamerad'),
  ),
}))

describe('PtpcameradDialog', () => {
  it('names the blocking process in the description, emphasized', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(PtpcameradDialog, {
      target,
      props: { blockingProcess: 'pid 45145, ptpcamerad', onClose: () => {}, onRetry: () => {} },
    })
    flushSync()

    const description = target.querySelector('.description')
    expect(description?.textContent).toBe('The device is in use by pid 45145, ptpcamerad.')
    expect(description?.querySelector('strong')?.textContent).toBe('pid 45145, ptpcamerad')

    void unmount(component)
    target.remove()
  })

  it('falls back to the generic sentence without a blocking process', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(PtpcameradDialog, {
      target,
      props: { onClose: () => {}, onRetry: () => {} },
    })
    flushSync()

    expect(target.querySelector('.description')?.textContent).toBe(
      'Another process has exclusive access to the device.',
    )

    void unmount(component)
    target.remove()
  })
})

/**
 * The dialog opens on any exclusive-access refusal, and `ptpcamerad` exists only
 * on macOS: the backend hands every other OS an empty command. So each OS reads
 * its own cause and its own way out, and only macOS gets a command to copy.
 */
describe('PtpcameradDialog, per OS', () => {
  async function bodyOn(platform: 'macos' | 'windows' | 'linux', command: string) {
    onTestFinished(withPlatform(platform))
    vi.mocked(getPtpcameradWorkaroundCommand).mockResolvedValueOnce(command)
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(PtpcameradDialog, { target, props: { onClose: () => {}, onRetry: () => {} } })
    await vi.waitFor(() => {
      expect(getPtpcameradWorkaroundCommand).toHaveBeenCalled()
    })
    await tick()
    flushSync()
    onTestFinished(() => {
      void unmount(component)
      target.remove()
    })
    return target
  }

  it('offers the workaround command on macOS', async () => {
    const body = await bodyOn('macos', 'while true; do pkill -9 ptpcamerad; sleep 1; done')
    expect(body.querySelector('.explanation')?.textContent).toContain('ptpcamerad')
    expect(body.querySelector('.command-wrapper')).not.toBeNull()
    expect(body.querySelector('.help-text')?.textContent).toContain('Ctrl+C')
  })

  it.each(['linux', 'windows'] as const)('names no macOS daemon and shows no command box on %s', async (platform) => {
    const body = await bodyOn(platform, '')
    expect(body.textContent).not.toContain('ptpcamerad')
    expect(body.textContent).not.toContain('macOS')
    expect(body.textContent).not.toContain('Terminal')
    expect(body.querySelector('.command-wrapper')).toBeNull()
    expect(body.querySelector('.help-text')?.textContent).toContain('then retry')
  })

  it('says how to free the phone on Linux', async () => {
    const body = await bodyOn('linux', '')
    expect(body.querySelector('.help-text')?.textContent).toBe('Unmount the phone in your file manager, then retry.')
  })
})
