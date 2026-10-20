/**
 * Tier 3 a11y tests for the file-operations chrome: the conflict dialog, the
 * rollback confirmation, the progress readout, the new-entry name field, the
 * slow-create notice, and the setup dialogs' Background button.
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
import type { OperationSnapshot, WriteConflictEvent } from '#lib/tauri-commands/index.js'
import type { ConflictPrompt } from './operation-conflict.svelte'
import type { SourcesGonePrompt as SourcesGonePromptState } from './sources-gone.svelte'
import type { LockedSourcesPrompt as LockedSourcesPromptState } from './locked-sources.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { seconds } from '#lib/units/index.js'

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

let sourcesGonePrompt: SourcesGonePromptState | null = null

vi.mock('./sources-gone.svelte', () => ({
  getSourcesGonePrompt: () => sourcesGonePrompt,
  isAnsweringSourcesGone: () => false,
  continueSourcesGonePrompt: vi.fn(() => Promise.resolve()),
  cancelSourcesGonePrompt: vi.fn(() => Promise.resolve()),
}))

let lockedSourcesPrompt: LockedSourcesPromptState | null = null

vi.mock('./locked-sources.svelte', () => ({
  getLockedSourcesPrompt: () => lockedSourcesPrompt,
  isAnsweringLockedSources: () => false,
  moveLockedSourcesAnyway: vi.fn(() => Promise.resolve()),
  skipLockedSources: vi.fn(() => Promise.resolve()),
  cancelLockedSourcesPrompt: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  // The name field's clash lookup and its directory-diff re-validation.
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
  // The S3 cost line's one question: an AWS estimate worth a line.
  estimateOperationCost: vi.fn(() => Promise.resolve([{ amount: 0.02, currency: 'USD', providerLabel: 'AWS' }])),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => fileSizeFormat),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

import OperationConflictDialog from './OperationConflictDialog.svelte'
import RollbackConfirmDialog from './RollbackConfirmDialog.svelte'
import SourcesGoneDialog from './SourcesGoneDialog.svelte'
import SourcesGonePrompt from './SourcesGonePrompt.svelte'
import LockedSourcesDialog from './LockedSourcesDialog.svelte'
import LockedSourcesPrompt from './LockedSourcesPrompt.svelte'
import TransferProgressReadout from './TransferProgressReadout.svelte'
import NewEntryNameField from './NewEntryNameField.svelte'
import S3CostLine from './S3CostLine.svelte'
import StillCreatingNotice from './StillCreatingNotice.svelte'
import StartInBackgroundButton from './StartInBackgroundButton.svelte'
import { NewEntryNameCheck } from './new-entry-name-check.svelte'

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
      rollbackSupport: 'supported',
      reverses: null,
      error: null,
      canRetry: false,
      items: null,
    }
    const event: WriteConflictEvent = {
      operationId: 'op-1',
      conflictId: 1,
      sourcePath: '/Users/me/Pictures/2026/sunset.jpg',
      destinationPath: '/Volumes/Naspolya/Backup/2026/sunset.jpg',
      sourceSize: { kind: 'exact', bytes: 2048 },
      destinationSize: { kind: 'exact', bytes: 1024 },
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
    expect(target.querySelector('#rollback-confirmation-body')?.textContent).toContain('won’t come back')
  })
})

/**
 * Tier 3 a11y test for `SourcesGonePrompt.svelte` and the `SourcesGoneDialog`
 * it mounts: a queued operation's question about sources that left while it
 * waited. The count sentence is what the dialog is about, so it has to describe
 * the dialog.
 */
describe('SourcesGonePrompt a11y', () => {
  async function mountDialog(): Promise<HTMLElement> {
    sourcesGonePrompt = {
      operationId: 'op-1',
      event: {
        operationId: 'op-1',
        operationType: 'move',
        totalCount: 22,
        goneCount: 6,
        goneSample: ['/Music/CD1/01.mp3', '/Music/CD1/02.mp3'],
        next: 'awaiting_answer',
      },
      snapshot: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SourcesGonePrompt, { target })
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

  it('has no a11y violations with the asking operation named and more gone than it lists', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SourcesGoneDialog, {
      target,
      props: {
        event: {
          operationId: 'op-1',
          operationType: 'copy',
          totalCount: 5_678,
          goneCount: 1_234,
          goneSample: ['/Music/CD1/01.mp3'],
          next: 'awaiting_answer',
        },
        snapshot: {
          operationId: 'op-1',
          operationType: 'copy',
          status: 'running',
          source: 'CD1',
          destination: 'Backup',
          rollbackSupport: 'supported',
          reverses: null,
          error: null,
          canRetry: false,
          items: null,
        },
        answering: true,
        onContinue: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('describes itself with the count sentence', async () => {
    const target = await mountDialog()
    const dialog = target.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-describedby')).toBe('sources-gone-message')
    expect(target.querySelector('#sources-gone-message')?.textContent).toContain('6 of the 22 items')
  })
})

/**
 * Tier 3 a11y test for `LockedSourcesPrompt.svelte` and the
 * `LockedSourcesDialog` it mounts: a move's question about the Locked items it
 * found. The count sentence is what the dialog is about, so it has to describe
 * the dialog.
 */
describe('LockedSourcesPrompt a11y', () => {
  async function mountDialog(): Promise<HTMLElement> {
    lockedSourcesPrompt = {
      operationId: 'op-1',
      event: {
        operationId: 'op-1',
        operationType: 'move',
        lockedCount: 6,
        lockedSample: ['/Docs/contract.pdf', '/Docs/deed.pdf'],
      },
      snapshot: null,
    }
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LockedSourcesPrompt, { target })
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

  it('has no a11y violations with the asking move named and more locked than it lists', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(LockedSourcesDialog, {
      target,
      props: {
        event: {
          operationId: 'op-1',
          operationType: 'move',
          lockedCount: 1_234,
          lockedSample: ['/Docs/contract.pdf'],
        },
        snapshot: {
          operationId: 'op-1',
          operationType: 'move',
          status: 'running',
          source: 'Docs',
          destination: 'Archive',
          rollbackSupport: 'supported',
          reverses: null,
          error: null,
          canRetry: false,
          items: null,
        },
        answering: true,
        onMoveAnyway: () => {},
        onSkip: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('describes itself with the count sentence', async () => {
    const target = await mountDialog()
    const dialog = target.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-describedby')).toBe('locked-sources-message')
    expect(target.querySelector('#locked-sources-message')?.textContent).toContain('6 items you’re moving are locked')
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

/**
 * Tier 3 a11y tests for `NewEntryNameField.svelte`, the subtitle plus name
 * field the New folder and New file dialogs share. Both dialogs are audited
 * whole in their own directories; this block covers what the field owns: the
 * labelled input and the error line it points `aria-describedby` at.
 */
describe('NewEntryNameField a11y', () => {
  async function mountField(kind: 'folder' | 'file', errorMessage = ''): Promise<HTMLElement> {
    const host = document.createElement('div')
    document.body.appendChild(host)
    const check = new NewEntryNameCheck({
      currentPath: '/Users/test/Projects',
      listingId: 'listing-1',
      showHiddenFiles: false,
      getName: () => 'draft',
    })
    check.errorMessage = errorMessage
    mount(NewEntryNameField, {
      target: host,
      props: { kind, currentPath: '/Users/test/Projects', check, value: 'draft', onSubmit: () => {} },
    })
    await tick()
    return host
  }

  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('the folder variant has no a11y violations', async () => {
    await expectNoA11yViolations(await mountField('folder'))
  })

  it('the file variant has no a11y violations', async () => {
    await expectNoA11yViolations(await mountField('file'))
  })

  it('an error line the field describes itself by has no a11y violations', async () => {
    const host = await mountField('folder', 'There is already a folder by this name in this folder.')
    expect(host.querySelector('input')?.getAttribute('aria-describedby')).toBe('new-folder-error')
    await expectNoA11yViolations(host)
  })
})

/**
 * Tier 3 a11y test for `S3CostLine.svelte`: the estimate line and its InfoTip,
 * which carries no visible text and so has to name itself.
 */
describe('S3CostLine a11y', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('the estimate line and its info glyph have no a11y violations', async () => {
    const host = document.createElement('div')
    document.body.appendChild(host)
    mount(S3CostLine, {
      target: host,
      props: {
        request: { operation: 'copy', previewId: 'preview-1', sourceVolumeId: 'root', destinationVolumeId: 's3-1' },
      },
    })
    await vi.waitFor(() => {
      expect(host.querySelector('.s3-cost')).not.toBeNull()
    })
    await expectNoA11yViolations(host)
  })
})

/**
 * Tier 3 a11y test for `StillCreatingNotice.svelte`: the new-folder / new-file
 * dialogs' "still creating" line, a live status beside a spinner.
 */
describe('StillCreatingNotice a11y', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('the slow-create notice has no a11y violations', async () => {
    const host = document.createElement('div')
    document.body.appendChild(host)
    mount(StillCreatingNotice, { target: host, props: { name: 'photos', kind: 'folder', since: Date.now() } })
    await tick()
    expect(host.querySelector('[role="status"]')).not.toBeNull()
    await expectNoA11yViolations(host)
  })
})

/**
 * Tier 3 a11y test for `StartInBackgroundButton.svelte`: the setup dialogs'
 * Background / Queue button. Its spoken name must carry the visible word, so
 * someone using both sight and a screen reader hears what they see.
 */
describe('StartInBackgroundButton a11y', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('has no a11y violations, enabled or disabled, and its name is the visible word', async () => {
    for (const disabled of [false, true]) {
      document.body.innerHTML = ''
      const host = document.createElement('div')
      document.body.appendChild(host)
      mount(StartInBackgroundButton, { target: host, props: { onclick: () => {}, disabled } })
      await tick()
      const button = host.querySelector('button')
      // The F2 chip is decoration and `aria-keyshortcuts` speaks it. With no `aria-label`, the
      // name is the content minus the hidden chip: the visible word (WCAG 2.5.3, label in name).
      const word = button?.cloneNode(true) as HTMLElement
      word.querySelectorAll('[aria-hidden="true"]').forEach((chip) => {
        chip.remove()
      })
      expect(button?.getAttribute('aria-label')).toBeNull()
      expect(word.textContent.trim()).toBe('Background')
      expect(button?.getAttribute('aria-keyshortcuts')).toBe('F2')
      await expectNoA11yViolations(host)
    }
  })
})
