/**
 * Tier 3 a11y tests for `ErrorPane.svelte`.
 *
 * Full-pane error display. Renders markdown content plus optional
 * retry and "Open System Settings" buttons based on the FriendlyError
 * category.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ErrorPane from './ErrorPane.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  openPrivacySettings: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/shortcuts/key-capture', () => ({
  isMacOS: vi.fn(() => true),
}))

const transientError = {
  category: 'transient' as const,
  title: 'Couldn\u2019t reach the drive',
  explanation: 'The network folder didn\u2019t respond. It may be offline.',
  suggestion: 'Check your Wi-Fi and try again.',
  rawDetail: 'EIO: timed out after 2000ms',
  retryHint: true,
}

const seriousError = {
  category: 'serious' as const,
  title: 'Couldn\u2019t read this folder',
  explanation: 'The folder is damaged or in an unknown format.',
  suggestion: 'Try a different tool to recover the data.',
  rawDetail: 'EBADF: bad file descriptor',
  retryHint: false,
}

const permissionError = {
  category: 'needs_action' as const,
  title: 'We have no permission to read this folder',
  explanation: 'macOS protects some folders until you grant access.',
  suggestion: 'Open System Settings > Privacy & Security and add Cmdr.',
  rawDetail: 'EACCES: permission denied',
  retryHint: false,
}

describe('ErrorPane a11y', () => {
  it('transient error (retry button visible) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorPane, {
      target,
      props: {
        friendly: transientError,
        folderPath: '/Volumes/External/photos',
        onRetry: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('serious error (no retry) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorPane, {
      target,
      props: {
        friendly: seriousError,
        folderPath: '/Volumes/External/corrupt',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('permission-denied (Open System Settings visible on macOS) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorPane, {
      target,
      props: {
        friendly: permissionError,
        folderPath: '/Users/test/Documents',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
