/**
 * Tier 3 a11y tests for the MTP surfaces: the connected toast, the Linux
 * permission dialog, and the macOS ptpcamerad dialog.
 *
 * One file per component would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * `isMacOS` needs care: the toast block forces it BOTH ways, and file-wide that
 * would decide what the two platform dialogs render on whichever runner the lane
 * happens to use. `null` means "use the real export", which is what those two
 * blocks, which never stubbed it, always saw.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import MtpConnectedToastContent from './MtpConnectedToastContent.svelte'
import MtpPermissionDialog from './MtpPermissionDialog.svelte'
import PtpcameradDialog from './PtpcameradDialog.svelte'
import { setLastConnectedDeviceName } from './mtp-connected-toast-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// `vi.hoisted`, not a plain `let`: spreading the real `$lib/settings` pulls in the
// command registry, which calls `isMacOS()` while the mocks are still being wired,
// and a module-level `let` is still in its temporal dead zone at that point.
const macState: { value: boolean | null } = vi.hoisted(() => ({ value: null }))

vi.mock('$lib/ui/toast', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  setSetting: vi.fn(),
}))

vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<{ isMacOS: () => boolean }>()
  return {
    ...actual,
    isMacOS: () => (macState.value === null ? actual.isMacOS() : macState.value),
  }
})

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
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
 * `ptpcamerad` note), which we simulate by mocking `isMacOS()`.
 */
describe('MtpConnectedToastContent a11y', () => {
  afterEach(() => {
    macState.value = null
  })

  it('macOS variant has no a11y violations', async () => {
    macState.value = true
    setLastConnectedDeviceName('Pixel 8')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('non-macOS variant has no a11y violations', async () => {
    macState.value = false
    setLastConnectedDeviceName('Pixel 8')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: {} })
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
