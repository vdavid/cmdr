/**
 * Tier 3 a11y and interaction coverage for the Ask Cmdr bulk-rename review.
 *
 * The dialog only receives display rows. These tests mock the trigger's user-action
 * callbacks, so no Tauri command or filesystem mutation can run here.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { state, actions } = vi.hoisted(() => ({
  state: {
    renameReview: null as {
      proposalId: string
      rows: Array<{
        rowId: string
        sourceName: string
        destinationName: string
        allowed: boolean
        blockedReason: string | null
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
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

import BulkRenameReviewDialog from './BulkRenameReviewDialog.svelte'

function review(overrides: Partial<NonNullable<typeof state.renameReview>> = {}) {
  return {
    proposalId: 'opaque-proposal-id',
    rows: [
      {
        rowId: 'opaque-row-one',
        sourceName: 'before-one.png',
        destinationName: 'after-one.png',
        allowed: true,
        blockedReason: null,
      },
      {
        rowId: 'opaque-row-two',
        sourceName: 'before-two.png',
        destinationName: 'after-two.png',
        allowed: true,
        blockedReason: null,
      },
      {
        rowId: 'opaque-row-blocked',
        sourceName: 'occupied.png',
        destinationName: 'after-three.png',
        allowed: false,
        blockedReason: 'targetExists',
      },
    ],
    preflighting: false,
    expired: false,
    requestVersion: 0,
    ...overrides,
  }
}

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

function requiredInput(target: ParentNode, selector: string): HTMLInputElement {
  const element = target.querySelector<HTMLInputElement>(selector)
  if (element === null) throw new Error(`Expected ${selector}`)
  return element
}

beforeEach(() => {
  state.renameReview = review()
  actions.apply.mockReset()
  actions.allowAll.mockReset()
  actions.cancel.mockReset()
  actions.denyAll.mockReset()
  actions.setAllowed.mockReset()
  document.body.replaceChildren()
})

describe('BulkRenameReviewDialog', () => {
  it('announces reviewable and blocked rows without accessibility violations', async () => {
    const target = mountDialog()
    await tick()

    expect(requiredElement(target, '[role="status"]').textContent).toContain('2 renames allowed; 1 blocked')
    expect(requiredButton(target, 'button[aria-label="Rename 2 files"]').disabled).toBe(false)
    expect(requiredInput(target, 'input[aria-label="Deny: before-one.png"]').checked).toBe(true)
    expect(requiredInput(target, 'input[aria-label="Allow: occupied.png"]').disabled).toBe(true)
    await expectNoA11yViolations(target)
  })

  it('sends only user decisions to the trigger callbacks', async () => {
    const target = mountDialog()
    await tick()

    requiredInput(target, 'input[aria-label="Deny: before-one.png"]').click()
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

  it('disables and labels Apply when no valid row remains allowed', async () => {
    state.renameReview = review({
      rows: review().rows.map((row) => ({ ...row, allowed: false })),
    })
    const target = mountDialog()
    await tick()

    expect(requiredButton(target, 'button[aria-label="Rename 0 files"]').disabled).toBe(true)
  })
})
