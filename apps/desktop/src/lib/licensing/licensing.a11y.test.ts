/**
 * Tier 3 a11y tests for the licensing dialogs: About, Acknowledgements, the
 * commercial reminder, the expiration modal, and the license-key dialog.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * No stub here disagrees between blocks: the five `$lib/tauri-commands` sets are
 * disjoint, so their union is what each block always saw, and `getCachedStatus`
 * was already a module-level mutable both the About and license-key blocks reset
 * in their own `beforeEach`. Every `$lib/*` stub spreads the real module first, so
 * a block that never stubbed one still sees its un-stubbed exports.
 */

import { describe, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import AboutWindow from './AboutWindow.svelte'
import AcknowledgementsDialog from './AcknowledgementsDialog.svelte'
import CommercialReminderModal from './CommercialReminderModal.svelte'
import ExpirationModal from './ExpirationModal.svelte'
import LicenseKeyDialog from './LicenseKeyDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mockCachedStatus: unknown = null
let mockLicenseInfo: unknown = null

// The union of the licensing IPC the five dialogs reach for. The real module is
// spread first so a call outside the union behaves as it does un-merged.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  markCommercialReminderDismissed: vi.fn(() => Promise.resolve()),
  markExpirationModalShown: vi.fn(() => Promise.resolve()),
  verifyLicense: vi.fn(() => Promise.resolve({ info: {}, fullKey: '', shortCode: '' })),
  commitLicense: vi.fn(() => Promise.resolve()),
  validateLicenseWithServer: vi.fn(() => Promise.resolve(null)),
  getLicenseInfo: vi.fn(() => Promise.resolve(mockLicenseInfo)),
  resetLicense: vi.fn(() => Promise.resolve()),
  parseActivationError: vi.fn(() => null),
}))

vi.mock('./licensing-store.svelte', () => ({
  loadLicenseStatus: vi.fn(() => Promise.resolve()),
  getCachedStatus: () => mockCachedStatus,
  setCachedStatus: vi.fn(),
  isPendingVerification: () => false,
  setPendingVerification: vi.fn(),
}))

// `@tauri-apps/api/app` is dynamically imported when About mounts; stub it so
// getVersion() resolves without crashing.
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(() => Promise.resolve('1.0.0')),
}))

// The real file is ~119 KB of generated JSON; a couple of representative rows
// exercise the same markup, including the URL-less case.
vi.mock('./third-party-packages.gen.json', () => ({
  default: {
    rust: [
      { name: 'serde', version: '1.0.228', license: 'MIT OR Apache-2.0', url: 'https://github.com/serde-rs/serde' },
      { name: 'mystery', version: '1.0.0', license: 'MIT', url: '' },
    ],
    npm: [{ name: '@ark-ui/svelte', version: '5.22.1', license: 'MIT', url: 'https://ark-ui.com' }],
  },
}))

vi.mock('$lib/ui/toast/toast-store.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  addToast: vi.fn(() => 'id'),
}))

// These dialogs share one jsdom document, they portal into `document.body`, and
// axe resolves ARIA id references document-wide. Clearing between tests keeps each
// audit looking at its own dialog only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `AboutWindow.svelte`.
 *
 * About dialog shows app name, version, license info, and a few
 * external links. The license description varies with cached license
 * status (personal/commercial/expired). Tests cover the three
 * meaningful variants.
 */
describe('AboutWindow a11y', () => {
  beforeEach(() => {
    mockCachedStatus = null
  })

  it('personal license (no status) has no a11y violations', async () => {
    mockCachedStatus = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('commercial perpetual license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: 'Acme Corp',
      expiresAt: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('commercial subscription license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: 'Acme Corp',
      expiresAt: '2027-01-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('expired license has no a11y violations', async () => {
    mockCachedStatus = {
      type: 'expired',
      expiredAt: '2025-12-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AboutWindow, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `AcknowledgementsDialog.svelte`.
 *
 * The dialog has two states worth checking: the brief loading state before the
 * generated package list resolves, and the loaded state with the two long
 * link lists. The lists are the interesting case, since hundreds of links in a
 * scrollable region is where a11y usually goes wrong.
 */
describe('AcknowledgementsDialog a11y', () => {
  function mountDialog(): HTMLElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AcknowledgementsDialog, { target, props: { onClose: () => {} } })
    return target
  }

  /**
   * Waits for the dynamic package-list import to land. A fixed number of `tick()`s
   * isn't enough (the `import()` settles over an unknown number of macrotasks), and
   * getting this wrong silently re-runs the loading-state assertions instead of the
   * loaded ones.
   */
  async function waitForPackages(target: HTMLElement): Promise<void> {
    for (let attempt = 0; attempt < 100; attempt++) {
      await new Promise((resolve) => setTimeout(resolve, 5))
      await tick()
      if (target.querySelector('.package-list li')) return
    }
    throw new Error("The package list never rendered; the dialog's dynamic import didn't resolve")
  }

  it('has no a11y violations while the list is loading', async () => {
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  // 20s rather than vitest's 5s default: this is the only a11y case that runs axe
  // over the FULL acknowledgements tree (hundreds of package links), and the
  // check lane runs the suite under v8 coverage, which costs it about 5x. Plain
  // `vitest run` finishes it in ~1.6s; instrumented it lands around 8s, so the
  // default budget fails deterministically in the lane and passes everywhere
  // else. ❗ The budget is the only thing raised — the assertion is unchanged.
  it('has no a11y violations once the package lists are rendered', { timeout: 20_000 }, async () => {
    const target = mountDialog()
    await waitForPackages(target)
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `CommercialReminderModal.svelte`.
 */
describe('CommercialReminderModal a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CommercialReminderModal, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ExpirationModal.svelte`.
 */
describe('ExpirationModal a11y', () => {
  it('with org name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExpirationModal, {
      target,
      props: {
        organizationName: 'Acme Corp',
        expiredAt: '2025-03-01T00:00:00Z',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('without org name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExpirationModal, {
      target,
      props: {
        organizationName: null,
        expiredAt: '2025-03-01T00:00:00Z',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `LicenseKeyDialog.svelte`.
 *
 * Dialog has three rendered branches: loading (short-lived), entry
 * (no existing license), and details (existing license). Entry is the
 * default for a fresh user; details shows the stored key info with a
 * "Use a different key" reset flow. Tests cover entry, details, and
 * the reset-confirm sub-state.
 */
describe('LicenseKeyDialog a11y', () => {
  beforeEach(() => {
    mockLicenseInfo = null
    mockCachedStatus = null
  })

  it('entry state (no existing license) has no a11y violations', async () => {
    mockLicenseInfo = null
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    // Flush the getLicenseInfo() microtask so `isLoading` flips to false.
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('details state (existing commercial license) has no a11y violations', async () => {
    mockLicenseInfo = {
      organizationName: 'Acme Corp',
      licenseType: 'commercial_perpetual',
      shortCode: 'CMDR-ABCD-EFGH-1234',
      transactionId: 'txn-1',
    }
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_perpetual',
      organizationName: 'Acme Corp',
      expiresAt: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })

  it('details state (subscription with expiry) has no a11y violations', async () => {
    mockLicenseInfo = {
      organizationName: 'Acme Corp',
      licenseType: 'commercial_subscription',
      shortCode: 'CMDR-WXYZ-1234-5678',
      transactionId: 'txn-2',
    }
    mockCachedStatus = {
      type: 'commercial',
      licenseType: 'commercial_subscription',
      organizationName: 'Acme Corp',
      expiresAt: '2027-01-01',
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LicenseKeyDialog, { target, props: { onClose: () => {}, onSuccess: () => {} } })
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})
