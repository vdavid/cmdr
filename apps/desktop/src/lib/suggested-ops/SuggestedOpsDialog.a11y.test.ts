/**
 * Tier 3 a11y and disclosure coverage for the AI suggestions dialog.
 *
 * The store is mocked, so nothing here reads a database or queues an operation. What these
 * tests actually pin is the disclosure: the button names the action, the AI's reason reaches
 * the screen LABELLED as the AI's words beside the facts Cmdr holds by itself, the undo pill
 * says why, an irreversible group says so, a folder that will be created says so, and a file's
 * size and date say what the disk says now, or say plainly that they're from suggestion time.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

const { actions, store } = vi.hoisted(() => ({
  actions: {
    close: vi.fn(),
    approve: vi.fn<(id: number) => Promise<void>>(),
    reject: vi.fn<(id: number) => Promise<void>>(),
    reload: vi.fn<(id: number) => Promise<void>>(),
    ensure: vi.fn<(payload: { groupId: number; startIndex: number }) => Promise<void>>(),
    range: vi.fn<(payload: { groupId: number; start: number; end: number }) => Promise<void>>(),
    toggle: vi.fn<(payload: { groupId: number; opId: number }) => void>(),
    showMore: vi.fn<(id: number) => void>(),
    showAll: vi.fn<(id: number) => void>(),
    toggleAll: vi.fn<(id: number) => void>(),
  },
  // A plain object rather than `$state`: each test mounts a fresh dialog, so the values only
  // have to be right at mount time.
  store: {
    state: {
      open: true,
      loading: false,
      loadError: false,
      sweeps: [] as Array<{ groups: Array<{ groupId: number; liveOpCount: number }> }>,
      reviews: new Map<number, unknown>(),
      busyGroupId: null as number | null,
      decisionNotice: null as string | null,
      refusals: new Map<number, unknown>(),
    },
    ops: [] as Array<{ opId: number }>,
    review: {
      window: null as { offset: number; ops: unknown[]; total: number } | null,
      windowLoading: false,
      windowError: false,
      shown: 10,
      deselected: new Set<number>(),
      changedUnderReview: false,
      liveFacts: new Map<number, unknown>(),
      takenNames: new Set<number>(),
      sourceUnreachable: null as string | null,
      range: { start: 0, end: 10 },
    },
  },
}))

// `ModalDialog` tells the MCP dialog registry it opened, which is a real Tauri call.
vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))
const settings = vi.hoisted(() => ({ suppressed: false, set: vi.fn() }))
vi.mock('#lib/settings/index.js', async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSetting: () => settings.suppressed,
  setSetting: settings.set,
}))
vi.mock('#lib/icon-cache.js', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, iconCacheVersion: writable(0), prefetchIcons: () => Promise.resolve() }
})

vi.mock('./suggested-ops-trigger.svelte', () => ({
  INITIAL_ROWS: 10,
  SHOW_MORE_STEP: 20,
  suggestedOpsState: store.state,
  approvableCount: (id: number) => store.state.sweeps[0]?.groups.find((g) => g.groupId === id)?.liveOpCount ?? 0,
  approveGroup: actions.approve,
  canApprove: (id: number) => {
    const found = store.state.sweeps[0]?.groups.find((g) => g.groupId === id) as
      | { liveOpCount: number; server?: unknown }
      | undefined
    return !store.review.changedUnderReview && (Boolean(found?.server) || (found?.liveOpCount ?? 0) > 0)
  },
  closeSuggestedOps: actions.close,
  ensureOpWindow: (groupId: number, startIndex: number) => actions.ensure({ groupId, startIndex }),
  isOpSelected: (_groupId: number, op: { opId: number }) =>
    !store.review.deselected.has(op.opId) &&
    (store.review.liveFacts.get(op.opId) as { kind?: string } | undefined)?.kind !== 'missing',
  isOpMissing: (_groupId: number, op: { opId: number }) =>
    (store.review.liveFacts.get(op.opId) as { kind?: string } | undefined)?.kind === 'missing',
  isNewNameTaken: (_groupId: number, op: { opId: number }) => store.review.takenNames.has(op.opId),
  pendingGroups: () => store.state.sweeps.flatMap((sweep) => sweep.groups),
  selectionState: () => (store.review.deselected.size === 0 ? 'all' : 'some'),
  toggleAllOps: actions.toggleAll,
  opAt: (_groupId: number, index: number) => store.ops[index] ?? null,
  rejectGroup: actions.reject,
  reloadGroup: actions.reload,
  reviewOf: () => store.review,
  showAllRows: actions.showAll,
  showMoreRows: actions.showMore,
  showRange: (groupId: number, start: number, end: number) => actions.range({ groupId, start, end }),
  shownRows: (group: { liveOpCount: number }) => Math.min(store.review.shown, group.liveOpCount),
  toggleOp: (groupId: number, opId: number) => {
    actions.toggle({ groupId, opId })
  },
  totalRows: (group: { liveOpCount: number }) => group.liveOpCount,
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
    displayName: 'Installers to the NAS',
    rationale: 'They all look like installers to me.',
    sourceVolumeId: 'root',
    sourceFolder: '/Users/someone/Downloads',
    destination: '/backups/Installers',
    destinationVolumeName: 'Naspolya',
    reversible: 'restoreMove',
    runsAs: null,
    destinationState: 'exists',
    liveOpCount: 3,
    totalOpCount: 3,
    fromSelector: false,
    server: null,
    ...overrides,
  }
}

function seed(groupOverrides: Record<string, unknown> = {}) {
  store.state.sweeps = [
    {
      sweepId: 1,
      createdAt: 1_780_000_000,
      rationale: 'Ten new files in Downloads.',
      groups: [group(groupOverrides)],
    } as unknown as { groups: Array<{ groupId: number; liveOpCount: number }> },
  ]
}

beforeEach(() => {
  vi.clearAllMocks()
  settings.suppressed = false
  document.body.innerHTML = ''
  store.state.loading = false
  store.state.loadError = false
  store.state.busyGroupId = null
  store.state.decisionNotice = null
  store.state.refusals = new Map<number, unknown>()
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
      sourcePath: '/Users/someone/Downloads/unrecorded.pdf',
      newName: null,
      status: 'pending',
      snapshotSize: null,
      snapshotModified: null,
    },
    {
      opId: 3,
      sourcePath: '/Users/someone/Downloads/gone.dmg',
      newName: null,
      status: 'pending',
      snapshotSize: 1_000,
      snapshotModified: 1_780_000_000,
    },
  ] as unknown as Array<{ opId: number }>
  store.review = {
    window: { offset: 0, ops: store.ops, total: store.ops.length },
    windowLoading: false,
    windowError: false,
    shown: 10,
    deselected: new Set<number>(),
    changedUnderReview: false,
    liveFacts: new Map<number, unknown>(),
    takenNames: new Set<number>(),
    sourceUnreachable: null,
    range: { start: 0, end: 10 },
  }
  seed()
})

describe('accessibility', () => {
  it('has no violations with a group and its file list', async () => {
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('has no violations with every row state at once', async () => {
    store.review.liveFacts = new Map<number, unknown>([
      [1, { kind: 'present', size: 99_999, modified: 1_790_000_000 }],
      [3, { kind: 'missing' }],
    ])
    const host = mountDialog()
    await expectNoA11yViolations(host)
  })

  it('names every per-file checkbox after its file', () => {
    const host = mountDialog()

    const names = [...host.querySelectorAll('input[type="checkbox"]')].map((box) => box.getAttribute('aria-label'))
    expect(names).toContain('Include invoice-jan.pdf')
  })
})

describe('the action', () => {
  it('names the verb and the count on the button', () => {
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Move 3 files')
    expect(buttons).toContain('Reject')
  })

  it('names the Trash on a trash group', () => {
    seed({ verb: 'trash', destination: null, destinationVolumeName: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Move 3 files to Trash')
    expect(host.querySelector('.facts')?.textContent).toContain('The Trash')
  })

  it('shows where the files are and the full destination, volume included', () => {
    const host = mountDialog()

    const facts = host.querySelector('.facts')?.textContent ?? ''
    expect(facts).toContain('/Users/someone/Downloads')
    expect(facts).toContain('Naspolya')
    expect(facts).toContain('/backups/Installers')
  })

  it('shows the files right away, with no button to reveal them', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('invoice-jan.pdf')
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).not.toContain('Review the files')
  })

  it('offers more rows, then all of them, under a long list', () => {
    seed({ liveOpCount: 60_000, totalOpCount: 60_000 })
    const host = mountDialog()

    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Show 20 more')
    expect(buttons).toContain('Show all 60,000')
  })
})

/**
 * The AI's reason is LABELLED as its words, beside facts Cmdr holds by itself. Without the label,
 * a rationale reads as something Cmdr verified, and a hallucinated claim arrives looking like a
 * finding.
 */
describe('disclosure', () => {
  it('labels the AI’s words as the AI’s, never as something Cmdr checked', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('They all look like installers to me.')
    expect(host.textContent).toContain('Why, according to the AI:')
  })

  it('says why a move can be undone', () => {
    const host = mountDialog()

    expect(host.textContent).toContain('You can undo this: Cmdr can move them back')
  })

  it('says a permanent delete cannot be undone, and still offers it', () => {
    seed({ verb: 'delete', reversible: 'irreversible', destination: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    expect(host.textContent).toContain('This can’t be undone')
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Delete 3 files')
  })

  it('offers a trash on a drive with no Trash as the permanent delete it would be', () => {
    seed({ verb: 'trash', runsAs: 'permanentDelete', destination: null, destinationState: 'notApplicable' })
    const host = mountDialog()

    expect(host.textContent).toContain('This drive has no Trash')
    expect(host.textContent).toContain('This can’t be undone')
    expect(host.textContent).not.toContain('You can undo this')
    expect(host.textContent).not.toContain('The Trash')
    const button = [...host.querySelectorAll('button')].find((b) => b.textContent.includes('Delete 3 files'))
    expect(button?.className).toContain('danger')
    button?.click()
    expect(actions.approve).toHaveBeenCalled()
  })

  it('says when approving would create the target folder', () => {
    seed({ destinationState: 'willBeCreated' })
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr will create this folder')
  })

  it('admits when it could not check the target folder, rather than guessing', () => {
    seed({ destinationState: 'unknown' })
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t check the target folder')
  })

  it('marks a group a pattern produced', () => {
    seed({ fromSelector: true })
    const host = mountDialog()

    expect(host.textContent).toContain('Matched by a pattern')
  })
})

describe('the approval note and the sweep heading', () => {
  it('closes the note for good from its ×', () => {
    settings.suppressed = false
    const host = mountDialog()
    expect(host.textContent).toContain('Apply them only if you agree.')

    host.querySelector<HTMLButtonElement>('[aria-label="Don’t show this again"]')?.click()
    flushSync()

    expect(settings.set).toHaveBeenCalledWith('suggestedOps.suppressApprovalNotice', true)
    expect(host.textContent).not.toContain('Apply them only if you agree.')
  })

  it('keeps the note hidden once the user closed it', () => {
    settings.suppressed = true
    const host = mountDialog()
    settings.suppressed = false

    expect(host.textContent).not.toContain('Apply them only if you agree.')
  })

  it('heads an older sweep with when it was suggested', () => {
    const host = mountDialog()

    expect(host.querySelector('.sweep-heading')?.textContent).toMatch(/^Suggested /)
  })

  it('leaves out the heading for a lone sweep from the last ten minutes', () => {
    seed()
    ;(store.state.sweeps[0] as unknown as { createdAt: number }).createdAt = Math.floor(Date.now() / 1000) - 60
    const host = mountDialog()

    expect(host.querySelector('.sweep-heading')).toBeNull()
    expect(host.textContent).toContain('Ten new files in Downloads.')
  })
})

describe('selection from the header', () => {
  it('ticks or unticks every file from the header box', () => {
    const host = mountDialog()

    const header = host.querySelector<HTMLElement>('[aria-label="Include all files"]')
    expect(header).not.toBeNull()
    header?.click()

    expect(actions.toggleAll).toHaveBeenCalledWith(7)
  })
})

describe('a new name', () => {
  it('shows after an arrow for any verb, not only a rename', () => {
    ;(store.ops[0] as unknown as { newName: string }).newName = 'Invoice 2026-01.pdf'
    const host = mountDialog()

    expect(host.textContent).toContain('Invoice 2026-01.pdf')
    expect(host.textContent).toContain('New name')
  })

  // A move that files its sources under new names (the seeded group is a move): only the
  // renamed row draws the arrow, and a screen reader hears "New name" before the name.
  it('draws the arrow on a renamed move row only, and reads it as the new name', async () => {
    ;(store.ops[0] as unknown as { newName: string }).newName = '2026-09-27 OpenAI invoice Rymd.pdf'
    const host = mountDialog()

    const cells = [...host.querySelectorAll('.name-cell')]
    const renamed = cells.find((cell) => cell.textContent.includes('2026-09-27 OpenAI invoice Rymd.pdf'))
    expect(renamed?.textContent).toContain('invoice-jan.pdf')
    expect(renamed?.querySelector('.arrow')).not.toBeNull()
    expect(renamed?.querySelector('.sr-only')?.textContent).toBe('New name')
    expect(cells.filter((cell) => cell.querySelector('.arrow') !== null)).toHaveLength(1)
    await expectNoA11yViolations(host)
  })
})

describe('honest file facts', () => {
  it('labels a value Cmdr couldn’t check now as the suggestion-time one, and never shows a zero', () => {
    const host = mountDialog()

    expect(host.querySelector('[aria-label="From when the AI suggested this"]')).not.toBeNull()
    expect(host.textContent).toContain('Unknown')
    expect(host.textContent).not.toContain('0 B')
    expect(host.textContent).not.toContain('Not in the index')
  })

  it('flags a live value that moved since the suggestion, with both values', () => {
    store.review.liveFacts = new Map<number, unknown>([[1, { kind: 'present', size: 99_999, modified: 1_780_000_000 }]])
    const host = mountDialog()

    const glyph = host.querySelector('[aria-label="Changed since the suggestion"]')
    expect(glyph).not.toBeNull()
  })

  it('says a file is gone', () => {
    store.review.liveFacts = new Map<number, unknown>([[3, { kind: 'missing' }]])
    const host = mountDialog()

    expect(host.textContent).toContain('Moved or deleted')
  })

  it('says once, above the list, when the files’ drive isn’t connected', () => {
    store.review.sourceUnreachable = 'notConnected'
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr isn’t connected to the drive these files are on')
  })

  it('shows each file’s folder only when the files don’t share one', () => {
    const shared = mountDialog()
    expect(shared.querySelector('[data-column="folder"]')).toBeNull()

    document.body.innerHTML = ''
    seed({ sourceFolder: null })
    const mixed = mountDialog()
    expect(mixed.querySelector('[data-column="folder"]')).not.toBeNull()
    expect(mixed.textContent).toContain('Several folders')
  })
})

describe('honest absence', () => {
  it('announces a group the agent changed instead of swapping the rows', () => {
    store.review.changedUnderReview = true
    const host = mountDialog()

    expect(host.textContent).toContain('The AI changed this suggestion while you were reading it.')
    expect(host.textContent).toContain('invoice-jan.pdf')
    const reload = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Show the new version')
    reload?.click()
    expect(actions.reload).toHaveBeenCalledWith(7)
    // The version on screen isn't the one that would run, so Approve waits for the reload.
    const approve = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Move 3 files')
    expect(approve?.disabled).toBe(true)
  })

  it('says nothing is waiting when nothing is', () => {
    store.state.sweeps = []
    const host = mountDialog()

    expect(host.textContent).toContain('Nothing is waiting for you right now.')
  })

  it('says the files could not load, and offers to try again', async () => {
    store.review.window = null
    store.review.windowError = true
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t load these files.')
    const retry = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Try again')
    expect(retry).toBeDefined()
    retry?.click()
    expect(actions.ensure).toHaveBeenCalledWith({ groupId: 7, startIndex: 0 })
    await expectNoA11yViolations(host)
  })

  it('says when an approval might not have gone through', () => {
    store.state.decisionNotice = 'suggestedOps.approvalUnsure'
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr isn’t sure the approval went through.')
  })

  it('distinguishes a read that failed from an empty list', () => {
    store.state.sweeps = []
    store.state.loadError = true
    const host = mountDialog()

    expect(host.textContent).toContain('Cmdr couldn’t read the suggestions.')
    expect(host.textContent).not.toContain('Nothing is waiting for you right now.')
  })

  it('shows why an approval didn’t start under the group it gave back', async () => {
    store.state.refusals = new Map([
      [7, { kind: 'refused', error: { type: 'source_not_connected', path: '/DCIM/one.jpg' } }],
    ])
    const host = mountDialog()

    const refusal = host.querySelector('.group .refusal')
    expect(refusal?.getAttribute('role')).toBe('alert')
    expect(refusal?.textContent).toContain('Why this didn’t start')
    expect(refusal?.textContent).toContain('Cmdr hasn’t connected to the phone or server holding these files yet')
    await expectNoA11yViolations(host)
  })

  it('says nothing under a group whose approval nobody refused', () => {
    const host = mountDialog()

    expect(host.querySelector('.refusal')).toBeNull()
  })
})

/**
 * A server card: what the click saves, laid out like every other suggestion, with no files, no
 * selection, and nothing to type into. The fields are Cmdr's facts; only the reason is the AI's.
 */
describe('a server card', () => {
  const addCard = {
    verb: 'addServer',
    displayName: 'Build box',
    rationale: 'You SSH into this box every day.',
    reversible: 'serverSettings',
    destination: null,
    destinationVolumeName: null,
    destinationState: 'notApplicable',
    sourceFolder: null,
    liveOpCount: 0,
    totalOpCount: 0,
    server: {
      protocol: 'sftp',
      fields: [
        { field: 'name', value: 'Build box', changed: false, previous: null },
        { field: 'address', value: 'build.local', changed: false, previous: null },
        { field: 'port', value: '22', changed: false, previous: null },
        { field: 'account', value: 'ada', changed: false, previous: null },
        { field: 'folder', value: '/srv/builds', changed: false, previous: null },
      ],
      addressIsNew: true,
      sharesAffected: null,
    },
  }
  const editCard = {
    ...addCard,
    verb: 'editServer',
    displayName: 'Naspolya',
    server: {
      protocol: 'smb',
      fields: [
        { field: 'name', value: 'Naspolya', changed: true, previous: 'naspi' },
        { field: 'address', value: 'naspi.local', changed: false, previous: null },
        { field: 'account', value: 'david', changed: true, previous: null },
      ],
      addressIsNew: false,
      sharesAffected: 3,
    },
  }

  it('shows the protocol and every field it saves, and the new address it points at', async () => {
    seed(addCard)
    const host = mountDialog()

    const facts = host.querySelector('.facts')?.textContent ?? ''
    for (const text of ['Type', 'SFTP', 'Address', 'build.local', 'Port', '22', 'Account', 'ada', '/srv/builds']) {
      expect(facts).toContain(text)
    }
    expect(facts).toContain('A new address')
    expect(host.querySelector('.group-head')?.textContent).toContain('Add a server')
    expect(host.querySelector('.group-title .title-text')?.textContent).toBe('Build box')
    expect(host.querySelector('.group-title .action')?.textContent).toBe('Add a server')
    expect(host.textContent).toContain('You can undo this: forget the server in Servers')
    await expectNoA11yViolations(host)
  })

  it('takes no input of any kind, and lists no files', () => {
    seed(addCard)
    const host = mountDialog()

    const group = host.querySelector('.group')
    expect(group?.querySelectorAll('input, textarea, select, [contenteditable]')).toHaveLength(0)
    expect(group?.querySelector('.op-row')).toBeNull()
    expect(group?.textContent).not.toContain('Password')
  })

  it('labels the AI’s reason as the AI’s, and nothing else', () => {
    seed(addCard)
    const host = mountDialog()

    expect(host.querySelector('.group .agent-reason')?.textContent).toContain('You SSH into this box every day.')
    expect(host.querySelector('.facts')?.textContent).not.toContain('You SSH into this box')
  })

  it('approves through the group with the add button', () => {
    seed(addCard)
    const host = mountDialog()

    const button = [...host.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Save server')
    expect(button?.disabled).toBe(false)
    button?.click()
    expect(actions.approve).toHaveBeenCalledWith(7)
  })

  it('shows old → new on the rows an edit changes, plain on the rest, and the shares it reaches', async () => {
    seed(editCard)
    const host = mountDialog()

    const rows = [...host.querySelectorAll('.facts dd')].map((dd) => ({
      text: dd.textContent.replace(/\s+/g, ' ').trim(),
      arrow: dd.querySelector('.arrow') !== null,
    }))
    expect(rows).toContainEqual({ text: 'naspi changes to Naspolya', arrow: true })
    expect(rows).toContainEqual({ text: 'Not set changes to david', arrow: true })
    expect(rows).toContainEqual({ text: 'naspi.local', arrow: false })
    expect(host.textContent).not.toContain('A new address')
    expect(host.querySelector('.shares')?.textContent).toContain('This changes 3 saved shares on this server.')
    expect(host.querySelector('.group-head')?.textContent).toContain('Change server settings')
    const buttons = [...host.querySelectorAll('button')].map((b) => b.textContent.trim())
    expect(buttons).toContain('Save changes')
    await expectNoA11yViolations(host)
  })

  it('shows why a save gave the card back, under the card', async () => {
    seed(addCard)
    store.state.refusals = new Map([[7, { kind: 'server', answer: { kind: 'nameTaken', name: 'Naspolya' } }]])
    const host = mountDialog()

    const refusal = host.querySelector('.group .refusal')
    expect(refusal?.getAttribute('role')).toBe('alert')
    expect(refusal?.textContent).toContain('Why this wasn’t saved')
    expect(refusal?.textContent).toContain('Another saved server is already called Naspolya')
    await expectNoA11yViolations(host)
  })

  it('words a refused save in the servers sheet’s own sentence, with no second line', () => {
    seed(addCard)
    store.state.refusals = new Map([
      [7, { kind: 'server', answer: { kind: 'refused', outcome: { outcome: 'start_folder_outside_root' } } }],
    ])
    const host = mountDialog()

    expect(host.querySelector('.refusal-message')?.textContent).toContain('The start folder has to be the root folder')
    expect(host.querySelector('.refusal-suggestion')).toBeNull()
  })
})

/** The dialog's parts, each audited on its own as well, so a part reused elsewhere stays clean. */
describe('the parts on their own', () => {
  function freshHost(): HTMLElement {
    const host = document.createElement('div')
    document.body.appendChild(host)
    return host
  }

  it('AgentReason labels the AI’s words', async () => {
    const AgentReason = (await import('./AgentReason.svelte')).default
    const host = freshHost()
    mount(AgentReason, { target: host, props: { text: 'They all look like invoices.' } })
    flushSync()
    await expectNoA11yViolations(host)
    expect(host.textContent).toContain('Why, according to the AI:')
  })

  it('SuggestedOpsGroup renders a group with no violations', async () => {
    const SuggestedOpsGroup = (await import('./SuggestedOpsGroup.svelte')).default
    const host = freshHost()
    mount(SuggestedOpsGroup, { target: host, props: { group: group() as never, actionLabels: [] } })
    flushSync()
    await expectNoA11yViolations(host)
    expect(host.textContent).toContain('Move 3 files')
  })

  it('VolumePill names a volume the list doesn’t know, by the name it was given', async () => {
    const VolumePill = (await import('./VolumePill.svelte')).default
    const host = freshHost()
    mount(VolumePill, { target: host, props: { volumeId: 'not-listed', name: 'Naspolya' } })
    flushSync()
    await expectNoA11yViolations(host)
    expect(host.textContent).toContain('Naspolya')
  })

  it('SuggestedOpFiles renders a file list with no violations', async () => {
    const SuggestedOpFiles = (await import('./SuggestedOpFiles.svelte')).default
    const { createRawSnippet } = await import('svelte')
    const actions = createRawSnippet(() => ({ render: () => '<span></span>' }))
    const host = freshHost()
    mount(SuggestedOpFiles, { target: host, props: { group: group() as never, undoNote: actions, actions } })
    flushSync()
    await expectNoA11yViolations(host)
    expect(host.textContent).toContain('invoice-jan.pdf')
  })
})
