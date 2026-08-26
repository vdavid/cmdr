/**
 * Tier 3 a11y and interaction coverage for the Ask Cmdr bulk-rename review.
 *
 * The dialog only receives display rows. These tests mock the trigger's user-action
 * callbacks, so no Tauri command or filesystem mutation can run here. The review state is a
 * real `$state` fixture, because the dialog mints thumbnail tokens per proposal and must drop
 * them when the review closes.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { BulkRenameReview, BulkRenameReviewRow } from './ask-cmdr-trigger.svelte'

const { actions, watcher, media, viewer } = vi.hoisted(() => ({
  actions: {
    apply: vi.fn<() => Promise<void>>(),
    allowAll: vi.fn(),
    cancel: vi.fn(),
    denyAll: vi.fn(),
    setAllowed: vi.fn(),
    listingChanged: vi.fn(),
    revise: vi.fn<(payload: { rowId: string; destinationName: string }) => Promise<void>>(),
  },
  watcher: {
    handler: null as ((diff: { changes: unknown[] }) => void) | null,
  },
  media: {
    /** Which paths get a thumbnail token; anything else falls back to the placeholder. */
    tokenFor: new Map<string, string>(),
    mint: vi.fn<(path: string) => Promise<string | null>>(),
    drop: vi.fn<(tokens: string[]) => Promise<void>>(),
  },
  viewer: {
    open: vi.fn<(payload: { filePath: string; volumeId: string }) => Promise<void>>(),
  },
}))

const { reviewState } = await import('./bulk-rename-review-fixture.svelte')

vi.mock('./ask-cmdr-trigger.svelte', async () => {
  const fixture = await import('./bulk-rename-review-fixture.svelte')
  return {
    applyRenameReview: async () => {
      await actions.apply()
    },
    allowAllRenameRows: () => {
      actions.allowAll()
    },
    askCmdrState: fixture.reviewState,
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
    reviseRenameRow: async (rowId: string, destinationName: string) => {
      await actions.revise({ rowId, destinationName })
    },
  }
})

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  onDirectoryDiff: vi.fn((handler: (diff: { changes: unknown[] }) => void) => {
    watcher.handler = handler
    return Promise.resolve(vi.fn())
  }),
  mediaIndexThumbnailToken: (path: string) => media.mint(path),
  mediaIndexDropThumbnailTokens: (tokens: string[]) => media.drop(tokens),
}))

// The viewer's `mediaUrl`; a plain string is all the row needs for render + axe.
vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

vi.mock('$lib/file-viewer/open-viewer', () => ({
  openFileViewer: (path: string, volumeId: string) => viewer.open({ filePath: path, volumeId }),
}))

import BulkRenameReviewDialog from './BulkRenameReviewDialog.svelte'

function row(overrides: Partial<BulkRenameReviewRow> = {}): BulkRenameReviewRow {
  return {
    rowId: 'opaque-row-one',
    sourceName: 'before-one.png',
    destinationName: 'after-one.png',
    sourcePath: '/shots/before-one.png',
    volumeId: 'root',
    evidence: { source: 'imageText', detail: 'Invoice 4021 total 250 SEK' },
    coverage: null,
    allowed: true,
    blockedReason: null,
    warnings: [],
    nameRejected: false,
    ...overrides,
  }
}

function review(overrides: Partial<BulkRenameReview> = {}): BulkRenameReview {
  return {
    proposalId: 'opaque-proposal-id',
    rows: [
      row({ warnings: ['extensionChanged'] }),
      row({
        rowId: 'opaque-row-two',
        sourceName: 'before-two.png',
        destinationName: 'after-two.png',
        sourcePath: '/shots/before-two.png',
        evidence: { source: 'metadata', detail: 'Taken 2026-07-20' },
        warnings: ['cycle'],
      }),
      row({
        rowId: 'opaque-row-blocked',
        sourceName: 'occupied.png',
        destinationName: 'after-three.png',
        sourcePath: '/shots/occupied.png',
        evidence: { source: 'imageTags', detail: 'receipt, document' },
        allowed: false,
        blockedReason: 'targetExists',
      }),
      row({
        rowId: 'opaque-row-missing',
        sourceName: 'imagined.png',
        destinationName: 'after-four.png',
        sourcePath: '/shots/imagined.png',
        evidence: { source: 'userInstruction', detail: 'you asked for YYYY-MM-DD prefixes' },
        allowed: false,
        blockedReason: 'sourceMissing',
      }),
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

/** Type into a field the way a user does, so the dialog's own input handler runs. */
function typeName(input: HTMLInputElement, value: string): void {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
  flushSync()
}

/** One row's editable new-name field, by the row's opaque id. */
function nameInput(target: ParentNode, rowId: string): HTMLInputElement {
  const element = target.querySelector<HTMLInputElement>(`input[data-row-id="${rowId}"]`)
  if (element === null) throw new Error(`Expected a name field for ${rowId}`)
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
  media.tokenFor.clear()
  media.mint.mockReset()
  media.mint.mockImplementation((path: string) => Promise.resolve(media.tokenFor.get(path) ?? null))
  media.drop.mockReset()
  media.drop.mockResolvedValue()
  viewer.open.mockReset()
  viewer.open.mockResolvedValue()
  reviewState.renameReview = review()
  actions.apply.mockReset()
  actions.allowAll.mockReset()
  actions.cancel.mockReset()
  actions.denyAll.mockReset()
  actions.setAllowed.mockReset()
  actions.listingChanged.mockReset()
  actions.revise.mockReset()
  actions.revise.mockResolvedValue()
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

  /**
   * A bare quote made a 7-character hit inside 3,140 characters of recognized text look
   * exactly as strong as a decisive one. The quote now renders inside the line it came from,
   * with a coverage figure under it, and a thin match is marked as thin.
   */
  it('renders a thin match and a decisive one differently', async () => {
    reviewState.renameReview = review({
      rows: [
        row({
          rowId: 'opaque-row-thin',
          evidence: { source: 'imageText', detail: 'Total 1 299 kr' },
          coverage: {
            matchOffset: 2_140,
            matchedChars: 14,
            deliveredChars: 3_140,
            contextBefore: 'Betalning mottagen  ',
            matchedText: 'Total 1 299 kr',
            contextAfter: '  Tack för ditt köp',
            trimmedBefore: true,
            trimmedAfter: true,
          },
        }),
        row({
          rowId: 'opaque-row-decisive',
          sourceName: 'before-two.png',
          sourcePath: '/shots/before-two.png',
          evidence: { source: 'imageText', detail: 'Klarna payment confirmation' },
          coverage: {
            matchOffset: 0,
            matchedChars: 27,
            deliveredChars: 44,
            contextBefore: '',
            matchedText: 'Klarna payment confirmation',
            contextAfter: ' 1,299 SEK',
            trimmedBefore: false,
            trimmedAfter: false,
          },
        }),
      ],
    })
    const target = mountDialog()
    await tick()

    const cells = [...target.querySelectorAll<HTMLElement>('td.why')]
    // The quote sits inside its surrounding line, with the cut ends marked.
    expect(cells[0]?.textContent).toContain('…Betalning mottagen  Total 1 299 kr  Tack för ditt köp…')
    expect(cells[0]?.querySelector('mark')?.textContent).toBe('Total 1 299 kr')
    expect(cells[1]?.textContent).toContain('Klarna payment confirmation 1,299 SEK')
    expect(cells[1]?.textContent).not.toContain('…')

    // Same figure shape, different verdict: only the sliver is flagged.
    expect(cells[0]?.textContent).toContain('Matched 14 of 3,140 characters')
    expect(cells[1]?.textContent).toContain('Matched 27 of 44 characters')
    expect(requiredElement(cells[0] ?? target, '[data-coverage]').dataset.coverage).toBe('thin')
    expect(requiredElement(cells[1] ?? target, '[data-coverage]').dataset.coverage).toBe('solid')
    const warning = requiredElement(cells[0] ?? target, '[data-coverage-warning="thin"]')
    expect(warning.getAttribute('aria-label')).toContain('small part of the text')
    expect(cells[1]?.querySelector('[data-coverage-warning]')).toBeNull()
    await expectNoA11yViolations(target)
  })

  /** Recognized text reaches this column too, so the excerpt is plain text like the quote. */
  it('renders the surrounding line as plain text, never as markup', async () => {
    reviewState.renameReview = review({
      rows: [
        row({
          evidence: { source: 'imageText', detail: 'Invoice 4021 total' },
          coverage: {
            matchOffset: 0,
            matchedChars: 18,
            deliveredChars: 60,
            contextBefore: MARKUP_DETAIL,
            matchedText: 'Invoice 4021 total',
            contextAfter: MARKUP_DETAIL,
            trimmedBefore: false,
            trimmedAfter: false,
          },
        }),
      ],
    })
    const target = mountDialog()
    await tick()

    const cell = requiredElement(target, 'td.why')
    expect(cell.querySelector('img')).toBeNull()
    expect(cell.textContent).toContain(MARKUP_DETAIL)
  })

  /**
   * The whole point of M1: `old name → new name → a quote` is what let 12 fabricated names
   * get approved, because the reviewer could not see the file. Every row shows its own image,
   * and the focused row's file opens in the full viewer.
   */
  it('shows a thumbnail per row and opens the focused row in the viewer', async () => {
    media.tokenFor.set('/shots/before-one.png', 'token-one')
    media.tokenFor.set('/shots/before-two.png', 'token-two')
    const target = mountDialog()
    await vi.waitFor(() => {
      expect(target.querySelectorAll('.preview-open img')).toHaveLength(2)
    })

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.preview-open')]
    expect(buttons).toHaveLength(4)
    expect(buttons[0]?.querySelector('img')?.getAttribute('src')).toBe('cmdr-media://localhost/token-one')
    expect(buttons[0]?.getAttribute('aria-label')).toBe('Open before-one.png')

    buttons[1]?.focus()
    flushSync()
    const focusedRows = [...target.querySelectorAll('tbody tr.focused')]
    expect(focusedRows).toHaveLength(1)
    expect(focusedRows[0]?.textContent).toContain('before-two.png')

    buttons[1]?.click()
    expect(viewer.open).toHaveBeenCalledWith({ filePath: '/shots/before-two.png', volumeId: 'root' })
    await expectNoA11yViolations(target)
  })

  /**
   * Degrade, never break: a file with no thumbnail (not an image, unreadable, or on a drive
   * that isn't mounted here) gets a neutral glyph, and the row stays fully reviewable.
   */
  it('shows a neutral placeholder for a file with no thumbnail', async () => {
    media.tokenFor.set('/shots/before-one.png', 'token-one')
    const target = mountDialog()
    await vi.waitFor(() => {
      expect(target.querySelectorAll('.preview-open img')).toHaveLength(1)
    })

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.preview-open')]
    expect(buttons[1]?.querySelector('img')).toBeNull()
    expect(buttons[1]?.querySelector('[data-preview="none"]')).not.toBeNull()
    expect(checkboxByLabel(target, 'Deny: before-two.png').disabled).toBe(false)
    await expectNoA11yViolations(target)
  })

  /** Keyboard-first: reviewing 50 rows can't require a mouse to move the preview. */
  it('walks the previews with the arrow keys', async () => {
    const target = mountDialog()
    await tick()
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('.preview-open')]

    buttons[0]?.focus()
    buttons[0]?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    expect(document.activeElement).toBe(buttons[1])

    buttons[1]?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    expect(document.activeElement).toBe(buttons[0])

    // At the ends, focus stays put rather than wrapping into another row's controls.
    buttons[0]?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
    expect(document.activeElement).toBe(buttons[0])
  })

  /**
   * A `cmdr-media://` token maps to a path in a backend map with no window-close choke point,
   * so the dialog owns every token it mints: a missed drop leaks path mappings for the session.
   */
  it('drops every thumbnail token when the review closes', async () => {
    media.tokenFor.set('/shots/before-one.png', 'token-one')
    media.tokenFor.set('/shots/before-two.png', 'token-two')
    const target = mountDialog()
    await vi.waitFor(() => {
      expect(target.querySelectorAll('.preview-open img')).toHaveLength(2)
    })
    expect(media.drop).not.toHaveBeenCalled()

    reviewState.renameReview = null
    flushSync()

    expect(media.drop).toHaveBeenCalledTimes(1)
    expect(media.drop.mock.calls[0]?.[0]).toEqual(expect.arrayContaining(['token-one', 'token-two']))
    expect(media.drop.mock.calls[0]?.[0]).toHaveLength(2)
  })

  /** Model-authored text reaches this column, so it must never be interpreted as markup. */
  it('renders evidence detail as plain text, never as markup', async () => {
    reviewState.renameReview = review({
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

  /**
   * The point of M2: a row used to be allow-or-deny, so a plausible wrong name left the user
   * with the model's name or the old one, which is the pressure that produces "approved because
   * it looked plausible". Now the name is a field, and leaving it (or pressing Enter) sends the
   * edit to the server, which owns validation, the row's evidence, and the preflight it clears.
   */
  it('sends a typed name to the server on blur and on Enter', async () => {
    const target = mountDialog()
    await tick()

    const input = nameInput(target, 'opaque-row-one')
    expect(input.value).toBe('after-one.png')
    expect(input.getAttribute('aria-label')).toBe('New name for before-one.png')

    typeName(input, 'Klarna payment 2026-07-24.png')
    input.dispatchEvent(new FocusEvent('blur'))
    expect(actions.revise).toHaveBeenCalledWith({
      rowId: 'opaque-row-one',
      destinationName: 'Klarna payment 2026-07-24.png',
    })

    typeName(input, 'Klarna payment confirmation 2026-07-24.png')
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(actions.revise).toHaveBeenLastCalledWith({
      rowId: 'opaque-row-one',
      destinationName: 'Klarna payment confirmation 2026-07-24.png',
    })
    await expectNoA11yViolations(target)
  })

  /** A blocked row is exactly the one a rename most often needs: an occupied name is fixed by
   *  typing a different one, so the field can't be disabled by the block. */
  it('lets a blocked row be renamed out of its clash', async () => {
    const target = mountDialog()
    await tick()

    const input = nameInput(target, 'opaque-row-blocked')
    expect(input.disabled).toBe(false)
    typeName(input, 'after-three (2).png')
    input.dispatchEvent(new FocusEvent('blur'))

    expect(actions.revise).toHaveBeenCalledWith({
      rowId: 'opaque-row-blocked',
      destinationName: 'after-three (2).png',
    })
  })

  /** Escape abandons one edit; it must not close the whole review over a typo. */
  it('reverts the field on Escape and sends nothing', async () => {
    const target = mountDialog()
    await tick()

    const input = nameInput(target, 'opaque-row-one')
    typeName(input, 'half-typed')
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    flushSync()

    expect(input.value).toBe('after-one.png')
    expect(actions.revise).not.toHaveBeenCalled()
    expect(actions.cancel).not.toHaveBeenCalled()
  })

  /** A name the server won't take leaves the row on the name it had, and says so on the row. */
  it('says when a typed name can’t be used', async () => {
    reviewState.renameReview = review({ rows: [row({ nameRejected: true })] })
    const target = mountDialog()
    await tick()

    const message = requiredElement(target, '.rejected')
    expect(message.textContent).toContain('Cmdr can’t use that name')
    expect(nameInput(target, 'opaque-row-one').getAttribute('aria-invalid')).toBe('true')
    await expectNoA11yViolations(target)
  })

  /**
   * The other half of M2: M4 will tell the model to keep a neutral name when it couldn't read a
   * file, and that instruction is worthless if the user can't see which rows took that path. The
   * state stays on the row, and it keeps saying nothing inside the file was read.
   */
  it('marks the rows where nothing inside the file was read', async () => {
    reviewState.renameReview = review({
      rows: [
        row({
          rowId: 'kept',
          sourceName: 'IMG_4417.jpeg',
          destinationName: 'IMG_4417.jpeg',
          evidence: { source: 'metadata', detail: 'Shot 2026-07-14' },
        }),
        row({ rowId: 'nothing-read', evidence: { source: 'metadata', detail: 'Shot 2026-07-14' } }),
        row({ rowId: 'read', evidence: { source: 'imageText', detail: 'Invoice 4021 total' } }),
        row({ rowId: 'typed', evidence: { source: 'userEdited', detail: '' } }),
      ],
    })
    const target = mountDialog()
    await tick()

    const kept = requiredElement(target, '[data-name-provenance="nameKept"]')
    expect(kept.textContent).toBe('(name kept)')
    expect(kept.getAttribute('aria-label')).toContain('read nothing inside this file')
    const nothingRead = requiredElement(target, '[data-name-provenance="nothingRead"]')
    expect(nothingRead.textContent).toBe('(nothing read)')
    expect(nothingRead.getAttribute('aria-label')).toContain('read nothing inside this file')
    expect(target.querySelectorAll('[data-name-provenance]')).toHaveLength(2)

    // A user-typed name states whose name it is, and claims nothing beyond that.
    const cells = [...target.querySelectorAll<HTMLElement>('td.why')]
    expect(cells[3]?.textContent?.trim()).toBe('You typed this name')
    expect(cells[3]?.querySelector('.evidence-detail')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('disables and labels Apply when no valid row remains allowed', async () => {
    reviewState.renameReview = review({
      rows: review().rows.map((row) => ({ ...row, allowed: false })),
    })
    const target = mountDialog()
    await tick()

    expect(requiredButton(target, 'button[aria-label="Rename 0 files"]').disabled).toBe(true)
  })
})
