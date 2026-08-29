/**
 * Tier 3 a11y tests for the file-operations chrome: the conflict dialog, the
 * rollback confirmation, and the progress readout.
 *
 * One file per component would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * `getFileSizeFormat` is the one genuine disagreement: the conflict dialog wants
 * `binary`, the readout `decimal`. It's a mutable each of those blocks installs in
 * its own `beforeEach`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, tick, flushSync, type ComponentProps } from 'svelte'
import type { OperationSnapshot, WriteConflictEvent } from '$lib/tauri-commands'
import type { ConflictPrompt } from './operation-conflict.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { seconds } from '$lib/units'

let prompt: ConflictPrompt | null = null

// What `getFileSizeFormat` answers. Each block that cares installs its own in
// `beforeEach`, so neither inherits the other's.
let fileSizeFormat: 'binary' | 'decimal' = 'binary'

vi.mock('./operation-conflict.svelte', () => ({
  getConflictPrompt: () => prompt,
  isResolvingConflictPrompt: () => false,
  isCancellingConflictPrompt: () => false,
  resolveConflictPrompt: vi.fn(() => Promise.resolve()),
  cancelConflictPrompt: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => fileSizeFormat),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

import OperationConflictDialog from './OperationConflictDialog.svelte'
import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'
import TransferProgressReadout from './TransferProgressReadout.svelte'

// These components share one jsdom document, the dialogs portal into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `OperationConflictDialog.svelte`.
 *
 * The chrome around a shared body, so what's audited here is what this dialog
 * adds: the context line it describes itself by, and the on-hold note. The body
 * itself has its own audit in `transfer/TransferConflictDialog.a11y.test.ts`.
 */
describe('OperationConflictDialog a11y', () => {
  let component: Record<string, unknown> | null = null
  let target: HTMLElement | null = null

  function makePrompt(over: Partial<ConflictPrompt> = {}): ConflictPrompt {
    const snapshot: OperationSnapshot = {
      operationId: 'op-1',
      operationType: 'copy',
      status: 'paused',
      source: '/Users/me/Pictures/2026',
      destination: '/Volumes/Naspolya/Backup',
      supportsRollback: true,
      reverses: null,
      error: null,
    }
    const event: WriteConflictEvent = {
      operationId: 'op-1',
      conflictId: 1,
      sourcePath: '/Users/me/Pictures/2026/sunset.jpg',
      destinationPath: '/Volumes/Naspolya/Backup/2026/sunset.jpg',
      sourceSize: 2048,
      destinationSize: 1024,
      sourceModified: 1_700_000_000,
      destinationModified: 1_699_000_000,
      destinationIsNewer: false,
      sizeDifference: -1024,
    }
    return { operationId: 'op-1', event, snapshot, pausedOthers: false, ...over }
  }

  function render(): HTMLElement {
    target = document.createElement('div')
    document.body.appendChild(target)
    component = mount(OperationConflictDialog, { target })
    flushSync()
    return target
  }

  beforeEach(() => {
    prompt = null
    fileSizeFormat = 'binary'
  })

  afterEach(() => {
    if (component) void unmount(component)
    component = null
    target?.remove()
    target = null
  })

  it('a file clash on a backgrounded copy has no a11y violations', async () => {
    prompt = makePrompt()
    const host = render()
    await expectNoA11yViolations(host)
  })

  it('the on-hold note has no a11y violations', async () => {
    prompt = makePrompt({ pausedOthers: true })
    const host = render()
    await expectNoA11yViolations(host)
  })

  it('a prompt whose snapshot has not landed has no a11y violations', async () => {
    prompt = makePrompt({ snapshot: null })
    const host = render()
    await expectNoA11yViolations(host)
  })
})

/**
 * Tier 3 a11y test for `RollbackConfirmDialog.svelte`.
 *
 * The question in front of the one control that can destroy a file the user had
 * before the operation started, so its title, body, and two answers have to
 * reach a screen reader as one described dialog.
 */
describe('RollbackConfirmDialog a11y', () => {
  async function mountDialog(): Promise<HTMLElement> {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(RollbackConfirmDialog, {
      target,
      props: { variant: 'stopAndDelete', onConfirm: () => {}, onCancel: () => {} },
    })
    await tick()
    return target
  }

  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('has no a11y violations', async () => {
    const target = await mountDialog()
    await expectNoA11yViolations(target)
  })

  it('describes itself with the sentence that says what will be deleted', async () => {
    const target = await mountDialog()
    const dialog = target.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-describedby')).toBe('rollback-confirmation-body')
    expect(target.querySelector('#rollback-confirmation-body')?.textContent).toContain("won't come back")
  })
})

describe('TransferProgressReadout a11y', () => {
  const running = {
    bytesDone: 50,
    bytesTotal: 200,
    filesDone: 1,
    filesTotal: 4,
    bytesPerSecond: 1_500_000,
    filesPerSecond: 27,
    etaSeconds: seconds(154),
  }

  async function mountReadout(props: ComponentProps<typeof TransferProgressReadout>): Promise<HTMLElement> {
    const host = document.createElement('div')
    document.body.appendChild(host)
    mount(TransferProgressReadout, { target: host, props })
    await tick()
    return host
  }

  beforeEach(() => {
    document.body.innerHTML = ''
    fileSizeFormat = 'decimal'
  })

  it('the dialog density has no a11y violations', async () => {
    await expectNoA11yViolations(await mountReadout(running))
  })

  it('the compact list-row density has no a11y violations', async () => {
    await expectNoA11yViolations(await mountReadout({ ...running, density: 'compact' }))
  })

  it('a stalled readout with no size total has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountReadout({
        ...running,
        bytesTotal: 0,
        countKind: 'items',
        stall: { stillForSeconds: 45, reason: 'destination', inFlight: 2 },
      }),
    )
  })
})
