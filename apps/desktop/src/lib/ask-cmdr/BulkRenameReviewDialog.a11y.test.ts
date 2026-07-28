/**
 * Tier 3 a11y and interaction coverage for the Ask Cmdr bulk-rename review.
 *
 * The dialog only receives display rows. These tests mock the trigger's user-action
 * callbacks, so no Tauri command or filesystem mutation can run here.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { state, actions, watcher } = vi.hoisted(() => ({
  state: {
    renameReview: null as {
      proposalId: string
      rows: Array<{
        rowId: string
        sourceName: string
        destinationName: string
        evidence: { source: 'imageText' | 'imageTags' | 'filename' | 'metadata' | 'userInstruction'; detail: string }
        allowed: boolean
        blockedReason: string | null
        warnings: Array<'extensionChanged' | 'cycle'>
      }>
      preflighting: boolean
      expired: boolean
      requestVersion: number
    } | null,
  },
  actions: {
    apply: vi.fn<() => Promise<void>>(),
    allowAll: vi.fn(),
    cancel: vi.fn(),
    denyAll: vi.fn(),
    setAllowed: vi.fn(),
    listingChanged: vi.fn(),
  },
  watcher: {
    handler: null as ((diff: { changes: unknown[] }) => void) | null,
  },
}))

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  applyRenameReview: async () => {
    await actions.apply()
  },
  allowAllRenameRows: () => {
    actions.allowAll()
  },
  askCmdrState: state,
  cancelRenameReview: () => {
    actions.cancel()
  },
  denyAllRenameRows: () => {
    actions.denyAll()
  },
  setRenameRowAllowed: (rowId: string, allowed: boolean) => {
    actions.setAllowed(rowId, allowed)
  },
  renameReviewListingChanged: (changes: unknown[]) => {
    actions.listingChanged(changes)
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn((handler: (diff: { changes: unknown[] }) => void) => {
    watcher.handler = handler
    return Promise.resolve(vi.fn())
  }),
}))

import BulkRenameReviewDialog from './BulkRenameReviewDialog.svelte'

function review(
  overrides: Partial<NonNullable<typeof state.renameReview>> = {},
): NonNullable<typeof state.renameReview> {
  return {
    proposalId: 'opaque-proposal-id',
    rows: [
      {
        rowId: 'opaque-row-one',
        sourceName: 'before-one.png',
        destinationName: 'after-one.png',
        evidence: { source: 'imageText', detail: 'Invoice 4021 total 250 SEK' },
        allowed: true,
        blockedReason: null,
        warnings: ['extensionChanged'],
      },
      {
        rowId: 'opaque-row-two',
        sourceName: 'before-two.png',
        destinationName: 'after-two.png',
        evidence: { source: 'metadata', detail: 'Taken 2026-07-20' },
        allowed: true,
        blockedReason: null,
        warnings: ['cycle'],
      },
      {
        rowId: 'opaque-row-blocked',
        sourceName: 'occupied.png',
        destinationName: 'after-three.png',
        evidence: { source: 'imageTags', detail: 'receipt, document' },
        allowed: false,
        blockedReason: 'targetExists',
        warnings: [],
      },
      {
        rowId: 'opaque-row-missing',
        sourceName: 'imagined.png',
        destinationName: 'after-four.png',
        evidence: { source: 'userInstruction', detail: 'you asked for YYYY-MM-DD prefixes' },
        allowed: false,
        blockedReason: 'sourceMissing',
        warnings: [],
      },
    ],
    preflighting: false,
    expired: false,
    requestVersion: 0,
    ...overrides,
  }
}

/** Markup a model could smuggle into `detail`; the column must show it, not run it. */
const MARKUP_DETAIL = '<img src=x onerror="boom">'

function mountDialog(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(BulkRenameReviewDialog, { target, props: {} })
  return target
}

function requiredElement(target: ParentNode, selector: string): HTMLElement {
  const element = target.querySelector<HTMLElement>(selector)
  if (element === null) throw new Error(`Expected ${selector}`)
  return element
}

function requiredButton(target: ParentNode, selector: string): HTMLButtonElement {
  const element = target.querySelector<HTMLButtonElement>(selector)
  if (element === null) throw new Error(`Expected ${selector}`)
  return element
}

/**
 * The row checkboxes render through the `Checkbox` primitive (Ark UI). The accessible
 * name sits on the hidden `<input>` itself — putting it on the wrapping label root
 * instead leaves the control anonymous to assistive tech, see `lib/ui/DETAILS.md`.
 */
function checkboxByLabel(target: ParentNode, label: string): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>(`input[type="checkbox"][aria-label="${label}"]`)
  if (input === null) throw new Error(`Expected checkbox labeled "${label}"`)
  return input
}

beforeEach(() => {
  state.renameReview = review()
  actions.apply.mockReset()
  actions.allowAll.mockReset()
  actions.cancel.mockReset()
  actions.denyAll.mockReset()
  actions.setAllowed.mockReset()
  actions.listingChanged.mockReset()
  watcher.handler = null
  document.body.replaceChildren()
})

describe('BulkRenameReviewDialog', () => {
  it('announces reviewable and blocked rows without accessibility violations', async () => {
    const target = mountDialog()
    await tick()

    expect(requiredElement(target, '[role="status"]').textContent).toContain('2 renames allowed; 2 blocked')
    expect(requiredButton(target, 'button[aria-label="Rename 2 files"]').disabled).toBe(false)
    expect(checkboxByLabel(target, 'Deny: before-one.png').checked).toBe(true)
    expect(checkboxByLabel(target, 'Allow: occupied.png').disabled).toBe(true)
    const overwriteBadge = requiredElement(target, '[data-warning="overwrite"]')
    expect(overwriteBadge.textContent).toContain('(overwrite!)')
    expect(overwriteBadge.getAttribute('aria-label')).toContain("isn't part of this rename plan")
    const missingBadge = requiredElement(target, '[data-warning="source-missing"]')
    expect(missingBadge.textContent).toContain("(doesn't exist)")
    expect(missingBadge.getAttribute('aria-label')).toContain('no longer exists')
    expect(checkboxByLabel(target, 'Allow: imagined.png').disabled).toBe(true)
    const extensionBadge = requiredElement(target, '[data-rename-warning="extensionChanged"]')
    expect(extensionBadge.textContent).toBe('(extension)')
    expect(extensionBadge.getAttribute('aria-label')).toBe(
      'Extension changed. The file contents will not be converted.',
    )
    const cycleBadge = requiredElement(target, '[data-rename-warning="cycle"]')
    expect(cycleBadge.textContent).toBe('(cycle)')
    expect(cycleBadge.getAttribute('aria-label')).toContain('one temporary name')
    await expectNoA11yViolations(target)
  })

  /**
   * The reviewer has to be able to SEE what each name is based on: old name → new name
   * alone is what let 12 fabricated names get approved. A source that read nothing inside
   * the file must say so, so a metadata-only name can't pass as content-derived.
   */
  it('shows why each name was chosen, naming the source honestly', async () => {
    const target = mountDialog()
    await tick()

    const headers = [...target.querySelectorAll('th')].map((th) => th.textContent)
    expect(headers).toContain('Why this name')

    const cells = [...target.querySelectorAll<HTMLElement>('td.why')]
    expect(cells).toHaveLength(4)
    expect(cells[0]?.textContent).toContain('Text in the image')
    expect(cells[0]?.textContent).toContain('Invoice 4021 total 250 SEK')
    expect(cells[1]?.textContent).toContain('File details, not contents')
    expect(cells[1]?.textContent).toContain('Taken 2026-07-20')
    expect(cells[2]?.textContent).toContain('What Cmdr sees in the image')
    expect(cells[3]?.textContent).toContain('What you asked for')
    expect(cells.map((cell) => cell.dataset.evidenceSource)).toEqual([
      'imageText',
      'metadata',
      'imageTags',
      'userInstruction',
    ])
    await expectNoA11yViolations(target)
  })

  /** Model-authored text reaches this column, so it must never be interpreted as markup. */
  it('renders evidence detail as plain text, never as markup', async () => {
    state.renameReview = review({
      rows: review()
        .rows.slice(0, 1)
        .map((row) => ({ ...row, evidence: { source: 'imageText' as const, detail: MARKUP_DETAIL } })),
    })
    const target = mountDialog()
    await tick()

    const cell = requiredElement(target, 'td.why')
    expect(cell.querySelector('img')).toBeNull()
    expect(cell.textContent).toContain(MARKUP_DETAIL)
  })

  it('sends only user decisions to the trigger callbacks', async () => {
    const target = mountDialog()
    await tick()

    checkboxByLabel(target, 'Deny: before-one.png').click()
    const bulkButtons = target.querySelectorAll<HTMLButtonElement>('.bulk-actions button')
    if (bulkButtons.length < 2) throw new Error('Expected bulk rename action buttons')
    const allowAll = bulkButtons.item(0)
    const denyAll = bulkButtons.item(1)
    allowAll.click()
    denyAll.click()
    requiredButton(target, 'button[aria-label="Rename 2 files"]').click()
    requiredButton(target, '.modal-footer button:not([aria-label])').click()

    expect(actions.setAllowed).toHaveBeenCalledWith('opaque-row-one', false)
    expect(actions.allowAll).toHaveBeenCalledOnce()
    expect(actions.denyAll).toHaveBeenCalledOnce()
    expect(actions.apply).toHaveBeenCalledOnce()
    expect(actions.cancel).toHaveBeenCalledOnce()
  })

  it('forwards pane file-watcher changes for live preflight', async () => {
    mountDialog()
    await tick()
    const changes = [{ type: 'add', entry: { name: 'after-three.png' } }]

    watcher.handler?.({ changes })

    expect(actions.listingChanged).toHaveBeenCalledWith(changes)
  })

  it('disables and labels Apply when no valid row remains allowed', async () => {
    state.renameReview = review({
      rows: review().rows.map((row) => ({ ...row, allowed: false })),
    })
    const target = mountDialog()
    await tick()

    expect(requiredButton(target, 'button[aria-label="Rename 0 files"]').disabled).toBe(true)
  })
})
