/**
 * Tier 3 a11y tests for the MTP surfaces: the connected toast, the Linux
 * permission dialog, and the macOS ptpcamerad dialog.
 *
 * One file per component would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The platform needs care: the toast block runs on both, each for one test, so
 * the two platform dialogs keep Vitest's default.
 */

import { describe, it, vi, afterEach, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { mount, tick } from 'svelte'
import MtpConnectedToastContent from './MtpConnectedToastContent.svelte'
import MtpPermissionDialog from './MtpPermissionDialog.svelte'
import PtpcameradDialog from './PtpcameradDialog.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/ui/toast/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dismissToast: vi.fn(),
}))

vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  setSetting: vi.fn(),
}))

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
  getPtpcameradWorkaroundCommand: vi.fn(() =>
    Promise.resolve('sudo launchctl kickstart -k gui/501/com.apple.ptpcamerad'),
  ),
}))

// These components share one jsdom document, the dialogs portal into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `MtpConnectedToastContent.svelte`.
 *
 * Sticky toast shown after an MTP device connects. The only state that
 * matters for a11y is the "Don't show again" checkbox and the two
 * action buttons. The body text is platform-dependent (macOS adds a
 * `ptpcamerad` note), which each test picks with `withPlatform()`.
 */
describe('MtpConnectedToastContent a11y', () => {
  it('macOS variant has no a11y violations', async () => {
    onTestFinished(withPlatform('macos'))
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: { toastId: 'mtp-connected', deviceName: 'Pixel 8' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('non-macOS variant has no a11y violations', async () => {
    onTestFinished(withPlatform('linux'))
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: { toastId: 'mtp-connected', deviceName: 'Pixel 8' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `MtpPermissionDialog.svelte`.
 *
 * Linux-specific help dialog with a copyable install command.
 */
describe('MtpPermissionDialog a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpPermissionDialog, {
      target,
      props: { onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `PtpcameradDialog.svelte`.
 *
 * macOS helper dialog for the ptpcamerad workaround.
 */
describe('PtpcameradDialog a11y', () => {
  it('with blocking process name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PtpcameradDialog, {
      target,
      props: { blockingProcess: 'pid 45145, ptpcamerad', onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('without blocking process name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PtpcameradDialog, {
      target,
      props: { onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
