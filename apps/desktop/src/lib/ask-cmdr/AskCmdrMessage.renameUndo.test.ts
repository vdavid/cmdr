/**
 * The rename-undo line as rendered: what the user actually reads after a batch lands.
 *
 * The store tests cover which ids get sent; these cover the part a user can be misled
 * by. A partial undo must SHOW that files stayed behind, and the buttons must carry
 * accessible names, since "Undo" on its own tells a screen reader nothing.
 */

import { describe, it, expect, afterEach, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import AskCmdrMessage from './AskCmdrMessage.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RailMessage, RenameUndoState } from './ask-cmdr-messages'

const undoRenameMock = vi.fn<(...args: unknown[]) => Promise<void>>(() => Promise.resolve())

vi.mock('./ask-cmdr-trigger.svelte', () => ({ undoRename: (...args: unknown[]) => undoRenameMock(...args) }))

let host: HTMLElement | null = null
let component: Record<string, unknown> | null = null

function render(message: RailMessage): HTMLElement {
  host = document.createElement('div')
  document.body.appendChild(host)
  component = mount(AskCmdrMessage, { target: host, props: { message } })
  return host
}

function line(undo: RenameUndoState, overrides: Partial<Extract<RailMessage, { kind: 'renameApplied' }>> = {}) {
  return {
    kind: 'renameApplied' as const,
    operationId: 'op-1',
    fileCount: 1234,
    jobOperationIds: [],
    jobFileCount: 0,
    undo,
    ...overrides,
  }
}

function buttons(target: HTMLElement): HTMLButtonElement[] {
  return [...target.querySelectorAll('button')]
}

afterEach(() => {
  if (component) void unmount(component)
  host?.remove()
  host = null
  component = null
  vi.clearAllMocks()
})

describe('a completed batch', () => {
  it('reports the count with thousands separators and offers an Undo', () => {
    const target = render(line({ status: 'undoable' }))

    expect(target.textContent).toContain('Renamed 1,234 files.')
    expect(buttons(target).map((b) => b.textContent.trim())).toEqual(['Undo'])
  })

  it('names what Undo would reverse, so it is not a bare "Undo" to a screen reader', () => {
    const target = render(line({ status: 'undoable' }))

    expect(buttons(target)[0].getAttribute('aria-label')).toBe('Undo renaming 1,234 files')
  })

  it('triggers the undo for its own batch when clicked', () => {
    const message = line({ status: 'undoable' })
    const target = render(message)

    buttons(target)[0].click()

    expect(undoRenameMock).toHaveBeenCalledWith(message)
  })

  it('adds a job-wide undo only once a run has more than one batch', async () => {
    const target = render(
      line({ status: 'undoable' }, { jobOperationIds: ['op-1', 'op-2', 'op-3'], jobFileCount: 2500 }),
    )
    await tick()

    const labels = buttons(target).map((b) => b.textContent.trim())
    expect(labels).toEqual(['Undo', 'Undo all 3 batches'])
    expect(buttons(target)[1].getAttribute('aria-label')).toBe(
      'Undo every rename in this run: 2,500 files across 3 batches',
    )
  })
})

describe('while it runs', () => {
  it('replaces the button with a progress line, so a slow drive is not silence', () => {
    const target = render(line({ status: 'undoing' }))

    expect(target.textContent).toContain('Putting the old names back…')
    expect(buttons(target)).toHaveLength(0)
  })
})

describe('the result', () => {
  it('reports a clean undo', () => {
    const target = render(line({ status: 'undone', restored: 1234 }))

    expect(target.textContent).toContain('Put the old names back on 1,234 files.')
    expect(buttons(target)).toHaveLength(0)
  })

  it('names the file and its own reason when one file stayed behind', () => {
    const target = render(
      line({
        status: 'partial',
        restored: 19,
        skipped: 1,
        refusedBatches: 0,
        skips: [{ reason: 'drift', count: 1, exampleName: 'invoice-2026.pdf' }],
      }),
    )

    expect(target.textContent).toContain('Put the old names back on 19 files.')
    // The whole point of the per-item reason: this file, this reason.
    expect(target.textContent).toContain('Left invoice-2026.pdf alone: it changed since the rename.')
    // And NOT the vague either/or class line it replaces.
    expect(target.textContent).not.toContain('or the old name is taken again')
  })

  it('gives each reason its own line, so two different reasons are not one blurred class', () => {
    const target = render(
      line({
        status: 'partial',
        restored: 19,
        skipped: 4,
        refusedBatches: 0,
        skips: [
          { reason: 'drift', count: 3, exampleName: 'invoice-2026.pdf' },
          { reason: 'restoreTargetOccupied', count: 1, exampleName: 'receipt-2026.pdf' },
        ],
      }),
    )

    expect(target.textContent).toContain('Left 3 files alone: they changed since the rename.')
    expect(target.textContent).toContain('Left receipt-2026.pdf alone: its old name is taken again.')
  })

  it('falls back to naming the reason class when no reason was recorded', () => {
    // A batch undone before the reason column existed: the count is still reported, so a
    // missing reason can never hide that files stayed behind.
    const target = render(line({ status: 'partial', restored: 19, skipped: 4, refusedBatches: 0, skips: [] }))

    expect(target.textContent).toContain('Left 4 files alone')
    expect(target.textContent).toContain('changed since the rename, or the old name is taken again')
  })

  it('names refused batches separately, since they carry no per-file numbers', () => {
    const target = render(line({ status: 'partial', restored: 12, skipped: 0, refusedBatches: 2, skips: [] }))

    expect(target.textContent).toContain('Put the old names back on 12 files.')
    expect(target.textContent).toContain('Cmdr couldn’t undo 2 batches.')
    // Nothing was skipped per file, so that line stays away.
    expect(target.textContent).not.toContain('alone')
  })

  it('says nothing happened rather than reporting an undo of zero files', () => {
    const target = render(line({ status: 'unavailable' }))

    expect(target.textContent).toContain('Nothing to put back.')
    expect(target.textContent).not.toContain('Put the old name')
  })
})

describe('a11y', () => {
  it('has no violations with both undo buttons present', async () => {
    const target = render(line({ status: 'undoable' }, { jobOperationIds: ['op-1', 'op-2'], jobFileCount: 1500 }))
    await tick()

    await expectNoA11yViolations(target)
  })

  it('has no violations reporting a partial result', async () => {
    const target = render(
      line({
        status: 'partial',
        restored: 19,
        skipped: 4,
        refusedBatches: 1,
        skips: [{ reason: 'drift', count: 4, exampleName: 'invoice-2026.pdf' }],
      }),
    )
    await tick()

    await expectNoA11yViolations(target)
  })
})
