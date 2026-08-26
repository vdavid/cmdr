/**
 * Tier 3 a11y and disclosure coverage for the Suggested ops dialog.
 *
 * The store is mocked, so nothing here reads a database or queues an operation. What these
 * tests actually pin is the disclosure: the agent's reason reaches the screen LABELLED as the
 * agent's words, the facts Cmdr holds by itself are labelled as those, an irreversible group
 * says so, a folder that will be created says so, and a file the index knew nothing about
 * shows that in words rather than as a zero.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { actions, store } = vi.hoisted(() => ({
  actions: {
    close: vi.fn(),
    collapse: vi.fn(),
    expand: vi.fn<(id: number) => Promise<void>>(),
    ensure: vi.fn<(payload: { groupId: number; startIndex: number }) => Promise<void>>(),
    reject: vi.fn<(id: number) => Promise<void>>(),
    refresh: vi.fn<() => Promise<void>>(),
    toggle: vi.fn<(id: number) => void>(),
  },
  // A plain object rather than `$state`: each test mounts a fresh dialog, so the values only
  // have to be right at mount time.
  store: {
    state: {
      open: true,
      loading: false,
      loadError: false,
      sweeps: [] as unknown[],
      openGroupId: null as number | null,
      window: null as { groupId: number; offset: number; ops: unknown[]; total: number } | null,
      windowLoading: false,
      deselected: new Set<number>(),
      changedUnderReview: false,
      busyGroupId: null as number | null,
    },
    ops: [] as unknown[],
  },
}))

// `ModalDialog` tells the MCP dialog registry it opened, which is a real Tauri call.
vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('./suggested-ops-trigger.svelte', () => ({
  suggestedOpsState: store.state,
  approvableCount: () => 3,
  closeSuggestedOps: actions.close,
  collapseGroup: actions.collapse,
  ensureOpWindow: (groupId: number, startIndex: number) => actions.ensure({ groupId, startIndex }),
  expandGroup: actions.expand,
  openGroup: () => (store.state as { sweeps: { groups: unknown[] }[] }).sweeps[0]?.groups[0] ?? null,
  opAt: (i: number) => store.ops[i] ?? null,
  refreshSuggestions: actions.refresh,
  rejectGroup: actions.reject,
  toggleOp: actions.toggle,
}))

const SuggestedOpsDialog = (await import('./SuggestedOpsDialog.svelte')).default

function mountDialog(): HTMLElement {
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(SuggestedOpsDialog, { target: host })
  flushSync()
  return host
}

function group(overrides: Record<string, unknown> = {}) {
  return {
    groupId: 7,
    sweepId: 1,
    verb: 'move',
    status: 'pending',
    displayName: 'five invoices',
    rationale: 'They all look like invoices to me.',
    sourceVolumeId: 'root',
    destination: '/Users/someone/Documents/Invoices',
    reversible: 'restoreMove',
    destinationState: 'exists',
    liveOpCount: 3,
    totalOpCount: 3,
    fromSelector: false,
    ...overrides,
  }
}

function seed(groupOverrides: Record<string, unknown> = {}, expanded = false) {
  store.state.sweeps = [
    { sweepId: 1, createdAt: 1_780_000_000, rationale: 'Ten new files in Downloads.', groups: [group(groupOverrides)] },
  ]
  store.state.openGroupId = expanded ? 7 : null
  store.state.window = expanded ? { groupId: 7, offset: 0, ops: store.ops, total: store.ops.length } : null
}

beforeEach(() => {
  vi.clearAllMocks()
  document.body.innerHTML = ''
  store.state.loading = false
  store.state.loadError = false
  store.state.changedUnderReview = false
  store.state.busyGroupId = null
  store.state.deselected = new Set<number>()
  store.ops = [
    {
      opId: 1,
      sourcePath: '/Users/someone/Downloads/invoice-jan.pdf',
      newName: null,
      status: 'pending',
      snapshotSize: 20_480,
      snapshotModified: 1_780_000_000,
    },
    {
      opId: 2,
      sourcePath: '/Users/someone/Downloads/unindexed.pdf',
      newName: null,
      status: 'pending',
      snapshotSize: null,
      snapshotModified: null,
    },
  ]
  seed()
})

describe('accessibility', () => {
  it('has no violations with a group listed', async () => {
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('has no violations with a group expanded over its file list', async () => {
    seed({}, true)
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('gives every per-file checkbox an accessible name', () => {
    seed({}, true)
    const host = mountDialog()

    const boxes = host.querySelectorAll('input[type="checkbox"]')
    expect(boxes.length).toBeGreaterThan(0)
    for (const box of boxes) {
      expect((box.getAttribute('aria-label') ?? '').length).toBeGreaterThan(0)
    }
  })
})

/**
 * Both labels, or neither works.
 *
 * The disclosure is a JUXTAPOSITION: the agent's claim on one side, facts Cmdr holds by itself
 * on the other, so the user can check one against the other. Drop either label and it inverts.
 * Without "Ask Cmdr's reason", a rationale reads as something Cmdr verified. Without "What Cmdr
 * knows", a column of sizes and dates reads as MORE of the agent's claims rather than as the
 * independent check on them, which is worse than showing no facts at all: it lends the agent
 * Cmdr's credibility.
 *
 * Both were nearly lost once already. "What Cmdr knows" was written into the message catalog
 * and never rendered; only `message-keys-unused` noticed, because the dialog looked complete
 * without it.
 */
describe('disclosure', () => {
  it("labels the agent's words as the agent's, never as something Cmdr checked", () => {
    const host = mountDialog()

    expect(host.textContent).toContain('They all look like invoices to me.')
    expect(host.textContent).toContain("Ask Cmdr's reason")
  })

  it("labels Cmdr's own facts as Cmdr's, so the two sit side by side", () => {
    seed({}, true)
    const host = mountDialog()

    expect(host.textContent).toContain('What Cmdr knows')
    expect(host.textContent).toContain('Size when suggested')
  })

  it('says a permanent delete cannot be undone, and still offers it', () => {
    seed({ verb: 'delete', reversible: 'irreversible', destination: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    expect(host.textContent).toContain("This can't be undone")
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Reject')
  })

  it('says when approving would create the target folder', () => {
    seed({ destinationState: 'willBeCreated' })
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr will create this folder')
  })

  it('admits when it could not check the target folder, rather than guessing', () => {
    seed({ destinationState: 'unknown' })
    const host = mountDialog()

    expect(host.textContent).toContain("Cmdr couldn't check the target folder")
  })

  it('marks a group a pattern produced', () => {
    seed({ fromSelector: true })
    const host = mountDialog()

    expect(host.textContent).toContain('Matched by a pattern')
  })
})

describe('honest absence', () => {
  it('says the index knew nothing about a file rather than showing a zero', () => {
    seed({}, true)
    const host = mountDialog()

    expect(host.textContent).toContain('Not in the index')
    expect(host.textContent).not.toContain('0 B')
  })

  it('announces a group the agent changed instead of swapping the rows', () => {
    seed({}, true)
    store.state.changedUnderReview = true
    const host = mountDialog()

    expect(host.textContent).toContain('Ask Cmdr changed this suggestion while you were reading it.')
    expect(host.textContent).toContain('/Users/someone/Downloads/invoice-jan.pdf')
  })

  it('says nothing is waiting when nothing is', () => {
    store.state.sweeps = []
    store.state.openGroupId = null
    const host = mountDialog()

    expect(host.textContent).toContain('Nothing is waiting for you right now.')
  })

  it('distinguishes a read that failed from an empty list', () => {
    store.state.sweeps = []
    store.state.openGroupId = null
    store.state.loadError = true
    const host = mountDialog()

    expect(host.textContent).toContain("Cmdr couldn't read the suggestions.")
    expect(host.textContent).not.toContain('Nothing is waiting for you right now.')
  })
})
