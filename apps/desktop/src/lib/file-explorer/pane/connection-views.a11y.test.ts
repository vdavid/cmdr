/**
 * Tier 3 a11y tests for the full-pane views that stand in for a listing: the
 * error pane and the SMB / MTP connection states.
 *
 * One file per view would cost about four times as much: `svelte-tests` charges
 * per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its view's own doc comment, props, and
 * assertions.
 *
 * `NetworkMountView` and `SearchResultsView` stay in their own files: each
 * mocks a dozen modules nothing else here touches, and folding them in would
 * hand every other block stubs it never had.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { RECONNECT_DELAYS_MS, type ReconnectState } from '../network/smb-reconnect-manager.svelte'

// `null` means "use the real `isMacOS`". Only `ErrorPane` forces it, because
// only that block depends on the macOS-only branch rendering; forcing it
// file-wide would change what the other blocks render on a Linux CI runner.
const stubs = vi.hoisted(() => ({ isMacOS: null as (() => boolean) | null }))

// The union of what these four views reach for, over the real module: each
// source file mocked a disjoint slice of it, so a bare union would hand a view
// a missing export it never had.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openPrivacySettings: vi.fn(() => Promise.resolve()),
  reconnectSmbVolume: vi.fn(),
  // Never resolves: `SmbReauthView` audits the form before any round-trip lands.
  reconnectSmbVolumeWithCredentials: vi.fn(() => new Promise<never>(() => {})),
  // `NetworkLoginForm` (rendered inside `SmbReauthView`) pre-fills the username from these on mount.
  getUsernameHint: vi.fn(() => Promise.resolve(null)),
}))

// Partial mock: `ShortcutChip` (rendered inside the Go back / Go home buttons and
// the Technical details summary) needs the real `toDisplayShortcut`.
vi.mock('$lib/shortcuts/key-capture', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/shortcuts/key-capture')>()
  return {
    ...actual,
    isMacOS: () => (stubs.isMacOS ? stubs.isMacOS() : actual.isMacOS()),
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

// Don't resolve: `MtpConnectionView` auto-connects on mount, but a pending
// promise keeps the UI in the "Connecting..." state we want to audit.
vi.mock('$lib/mtp/mtp-store.svelte', () => ({
  connect: vi.fn(() => new Promise<never>(() => {})),
}))

vi.mock('$lib/mtp', () => ({
  isMtpVolumeId: (id: string) => id.startsWith('mtp-'),
  constructMtpPath: (device: string, storage: number) => `mtp://${device}/${String(storage)}`,
}))

import ErrorPane from './ErrorPane.svelte'
import MtpConnectionView from './MtpConnectionView.svelte'
import SmbReauthView from './SmbReauthView.svelte'
import SmbReconnectingView from './SmbReconnectingView.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  stubs.isMacOS = null
})

/**
 * Tier 3 a11y tests for `ErrorPane.svelte`.
 *
 * Full-pane error display. Renders markdown content plus optional
 * retry and "Open System Settings" buttons based on the FriendlyError
 * category.
 */
describe('ErrorPane a11y', () => {
  // Only `isMacOS` is forced, so the "Open System Settings" branch renders on any host.
  beforeEach(() => {
    stubs.isMacOS = () => true
  })

  const transientError = {
    category: 'transient' as const,
    title: 'Couldn’t reach the drive',
    explanation: 'The network folder didn’t respond. It may be offline.',
    suggestion: 'Check your Wi-Fi and try again.',
    rawDetail: 'EIO: timed out after 2000ms',
    retryHint: true,
  }

  const seriousError = {
    category: 'serious' as const,
    title: 'Couldn’t read this folder',
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

  it('transient error (retry button visible) has no a11y violations', async () => {
    const target = container()
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
    const target = container()
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
    const target = container()
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

  // The busiest row the pane can render: an error-specific CTA plus both ways out,
  // each carrying a shortcut chip.
  it('both ways out visible (Go back + Go home, with shortcut chips) has no a11y violations', async () => {
    const target = container()
    mount(ErrorPane, {
      target,
      props: {
        friendly: transientError,
        folderPath: '/Volumes/External/photos',
        onRetry: () => {},
        canGoBack: true,
        onGoBack: () => {},
        onGoHome: () => {},
        isFocused: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SmbReauthView.svelte`.
 *
 * The sign-in prompt shown when an SMB reconnect gave up on an auth failure.
 * Audits the default state (stale-password message + login form).
 */
describe('SmbReauthView a11y', () => {
  it('default state (stale-password message + form) has no a11y violations', async () => {
    const target = container()
    mount(SmbReauthView, {
      target,
      props: { volumeId: 'smb-test', serverLabel: 'Test server', onCancel: vi.fn() },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `SmbReconnectingView.svelte`.
 *
 * Covers the three cycle states (waiting, attempting, gave-up; but the pane
 * never renders the gave-up state itself; the parent swaps to
 * `VolumeUnreachableBanner`). Validates structural a11y in each phase and that
 * the buttons stay accessible when "Retry now" is disabled mid-attempt.
 */
describe('SmbReconnectingView a11y', () => {
  function waitingState(attemptIndex = 0): ReconnectState {
    return {
      status: 'waiting',
      attemptIndex,
      currentDelayMs: RECONNECT_DELAYS_MS[attemptIndex],
      waitStartedAt: performance.now(),
    }
  }

  function attemptingState(attemptIndex = 0): ReconnectState {
    return {
      status: 'attempting',
      attemptIndex,
      currentDelayMs: RECONNECT_DELAYS_MS[attemptIndex],
      waitStartedAt: performance.now(),
    }
  }

  it('first wait (no body 2) has no violations', async () => {
    const target = container()
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: waitingState(0),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mid-cycle wait with body 2 has no violations', async () => {
    const target = container()
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: waitingState(2),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('attempting state (Retry now disabled) has no violations', async () => {
    const target = container()
    mount(SmbReconnectingView, {
      target,
      props: {
        volumeId: 'volumesnaspi',
        shareName: 'naspi',
        cycleState: attemptingState(1),
        onCancel: () => {},
        onDisconnect: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `MtpConnectionView.svelte`.
 *
 * Only renders when the current volume is a device-only MTP ID. Tests
 * verify that the connecting and error UIs have no violations.
 */
describe('MtpConnectionView a11y', () => {
  it('connecting state (device-only volumeId) has no a11y violations', async () => {
    const target = container()
    mount(MtpConnectionView, {
      target,
      props: { volumeId: 'mtp-336592896' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('non-MTP volume (no render) has no a11y violations', async () => {
    const target = container()
    mount(MtpConnectionView, {
      target,
      props: { volumeId: 'root' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
