/**
 * Rendering tests for `PtpcameradDialog.svelte`.
 *
 * The named-process sentence goes through `<Trans>`, which merges the tag
 * snippets into the interpolation params. A tag whose name equals a param name
 * therefore overwrites the param, and the sentence renders the stringified
 * handler instead of the process name.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import PtpcameradDialog from './PtpcameradDialog.svelte'

vi.mock('$lib/tauri-commands', () => ({
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
