/**
 * Tier 3 a11y tests for the presentational pieces the transfer dialogs and toasts
 * compose: the compression controls, the direction arrow, the scan-phase body,
 * the variant-derived error copy, the cancelled-rollback summary, the
 * progress dialog's Rollback button, its stalled-transfer notice, its done
 * state's summary, and the setup dialog's "files already exist" block.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The dialogs themselves live in `transfer-dialogs.a11y.test.ts`; one merged
 * file for the whole directory would clear the 800-line `file-length` mark.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

// `null` means "use the real export": `DirectionIndicator` and `ScanPhaseBody`
// never stubbed the settings barrel, and handing them one would change what
// they render.
const stubs = vi.hoisted(() => ({
  getSetting: null as ((id: string) => unknown) | null,
  settingDefinition: null as ((id: string) => unknown) | null,
  defaultValue: null as ((id: string) => unknown) | null,
}))

vi.mock('#lib/settings/index.js', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realGetSetting = actual.getSetting as (id: string) => unknown
  const realDefinition = actual.getSettingDefinition as (id: string) => unknown
  const realDefault = actual.getDefaultValue as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string) => (stubs.getSetting ? stubs.getSetting(id) : realGetSetting(id)),
    setSetting: vi.fn(),
    resetSetting: vi.fn(),
    isModified: vi.fn(() => false),
    onSpecificSettingChange: vi.fn(() => () => {}),
    onSettingChange: vi.fn(() => () => {}),
    getSettingDefinition: (id: string) => (stubs.settingDefinition ? stubs.settingDefinition(id) : realDefinition(id)),
    getDefaultValue: (id: string) => (stubs.defaultValue ? stubs.defaultValue(id) : realDefault(id)),
  }
})

// `FallbackErrorContent` stubbed this module to an empty object purely to keep
// IPC out of the mount; spreading the real one is the same for it and leaves
// the other blocks with the module they already had.
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
}))

// The stall notice's report link opens the report flow and leaves a
// breadcrumb on click; neither has a place in a mount-only axe pass.
vi.mock('#lib/error-reporter/error-report-flow.svelte.js', () => ({ openErrorReportFor: vi.fn() }))
vi.mock('#lib/error-reporter/breadcrumbs.js', () => ({ recordBreadcrumb: vi.fn() }))

import CompressEstimateLine from './CompressEstimateLine.svelte'
import CompressLevelControl from './CompressLevelControl.svelte'
import DirectionIndicator from './DirectionIndicator.svelte'
import FallbackErrorContent from './FallbackErrorContent.svelte'
import ScanPhaseBody from './ScanPhaseBody.svelte'
import CancelRollbackToastContent from './CancelRollbackToastContent.svelte'
import TransferRollbackControls from './TransferRollbackControls.svelte'
import TransferStallNotice from './TransferStallNotice.svelte'
import TransferConflictsSection from './TransferConflictsSection.svelte'
import TransferDoneSummary from './TransferDoneSummary.svelte'
import type { TransferRollback } from './transfer-rollback.svelte'

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  stubs.getSetting = null
  stubs.settingDefinition = null
  stubs.defaultValue = null
})

/**
 * Tier 3 a11y tests for `CompressEstimateLine.svelte`.
 *
 * Covers the three visible states: a present estimate, the loading affordance
 * while a local scan runs, and the absent state (remote source), which renders
 * nothing and must stay violation-free as an empty mount.
 */
describe('CompressEstimateLine a11y', () => {
  const estimate = { compressibleBytes: 1_000_000, mediumBytes: 500_000, incompressibleBytes: 250_000 }

  beforeEach(() => {
    stubs.getSetting = () => 6
  })

  it('has no a11y violations with a present estimate', async () => {
    const target = container()
    mount(CompressEstimateLine, { target, props: { estimate, isScanning: false, sourceIsLocal: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while loading', async () => {
    const target = container()
    mount(CompressEstimateLine, { target, props: { estimate: null, isScanning: true, sourceIsLocal: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations when absent (remote source)', async () => {
    const target = container()
    mount(CompressEstimateLine, { target, props: { estimate: null, isScanning: true, sourceIsLocal: false } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `CompressLevelControl.svelte`.
 *
 * A thin frame around the shared `SettingSlider`. The settings barrel is mocked
 * so the slider renders without a store.
 */
describe('CompressLevelControl a11y', () => {
  beforeEach(() => {
    stubs.getSetting = () => 6
    stubs.defaultValue = () => 6
    stubs.settingDefinition = () => ({
      label: 'Compression level',
      description: '',
      constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
    })
  })

  it('has no a11y violations', async () => {
    const target = container()
    mount(CompressLevelControl, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `DirectionIndicator.svelte`.
 *
 * Arrow graphic that shows "source folder -> destination folder" or the
 * reverse. No Tauri deps, just pure presentational component.
 */
describe('DirectionIndicator a11y', () => {
  it('right direction (source -> destination) has no a11y violations', async () => {
    const target = container()
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/documents',
        destinationPath: '/Users/test/backup',
        direction: 'right',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('left direction (destination <- source) has no a11y violations', async () => {
    const target = container()
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/source-folder',
        destinationPath: '/Users/test/target-folder',
        direction: 'left',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long paths (truncated) has no a11y violations', async () => {
    const target = container()
    mount(DirectionIndicator, {
      target,
      props: {
        sourcePath: '/Users/test/nested/deeply/inside/a-very-long-folder-name-that-overflows',
        destinationPath: '/Volumes/External/backup/2026/january/archive/another-very-long-folder-name',
        direction: 'right',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `FallbackErrorContent.svelte`.
 *
 * Renders variant-derived copy (title + suggestion) for `WriteOperationError`
 * variants when the backend didn't attach a `FriendlyError`. Pinned across
 * the variants the parent dialog's a11y suite covers (permission_denied,
 * insufficient_space, read_only_device, device_disconnected).
 */
describe('FallbackErrorContent a11y', () => {
  function mountFallback(error: WriteOperationError, operationType: 'copy' | 'move' | 'delete' | 'trash' = 'copy') {
    const target = container()
    mount(FallbackErrorContent, { target, props: { error, operationType } })
    return target
  }

  it('permission_denied (copy) has no a11y violations', async () => {
    const target = mountFallback(
      {
        type: 'permission_denied',
        path: '/Users/test/protected.txt',
        message: 'EACCES',
        errno: null,
        refusal: 'unclassified',
        refusedFolder: null,
        side: null,
      },
      'copy',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('insufficient_space (move) has no a11y violations', async () => {
    const target = mountFallback(
      {
        type: 'insufficient_space',
        required: 1024 * 1024 * 500,
        available: 1024 * 1024 * 42,
        volumeName: 'External',
      },
      'move',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read_only_device (delete) has no a11y violations', async () => {
    const target = mountFallback(
      { type: 'read_only_device', path: '/Volumes/ReadOnly/file.txt', deviceName: 'ReadOnly', side: 'destination' },
      'delete',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('device_disconnected (trash) has no a11y violations', async () => {
    const target = mountFallback(
      { type: 'device_disconnected', path: '/Volumes/External/file.txt', side: null },
      'trash',
    )
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ScanPhaseBody.svelte`.
 *
 * Pure presentational component rendered inside `TransferProgressDialog`
 * during the scan phase. Shows source path, running tallies (bytes / files /
 * dirs), throughput, and current dir / current file. No Tauri deps; data
 * flows in via props.
 */
describe('ScanPhaseBody a11y', () => {
  it('early scan state (no throughput, no current file) has no a11y violations', async () => {
    const target = container()
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Users/test/documents',
        scanFilesFound: 0,
        scanDirsFound: 0,
        scanBytesFound: 0,
        scanFilesPerSec: null,
        scanBytesPerSec: null,
        scanCurrentDir: null,
        currentFile: null,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('mid-scan state (throughput + current dir + file) has no a11y violations', async () => {
    const target = container()
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Users/test/documents',
        scanFilesFound: 1234,
        scanDirsFound: 56,
        scanBytesFound: 5_678_901_234,
        scanFilesPerSec: 850,
        scanBytesPerSec: 12_345_678,
        scanCurrentDir: '/Users/test/documents/projects/cmdr/apps/desktop/src',
        currentFile: 'large-build-artifact.tar.gz',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('long paths (shorten-middle action) have no a11y violations', async () => {
    const target = container()
    mount(ScanPhaseBody, {
      target,
      props: {
        sourceFolderPath: '/Volumes/External/very/deeply/nested/folder/structure/with/many/levels/of/depth',
        scanFilesFound: 1,
        scanDirsFound: 1,
        scanBytesFound: 1024,
        scanFilesPerSec: null,
        scanBytesPerSec: null,
        scanCurrentDir: '/Volumes/External/very/deeply/nested/folder/structure/with/many/levels/of/depth/subdir',
        currentFile: 'a-file-with-a-rather-long-name-that-exceeds-the-container-width.txt',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `CancelRollbackToastContent.svelte`.
 *
 * Two shapes, because they differ structurally rather than only in wording: the
 * one-line summary a clean reversal gets, and the headline + explanation + list
 * a reversal that left things behind gets. The list is the half worth checking:
 * a screen reader has to reach every leftover, not just the first.
 */
describe('CancelRollbackToastContent a11y', () => {
  it('has no a11y violations on a clean reversal', async () => {
    const target = container()
    mount(CancelRollbackToastContent, {
      target,
      props: {
        readout: {
          headline: 'Removed the 3 items Cmdr had written.',
          leftBehind: null,
          reasons: [],
          recovered: [],
          staged: [],
          level: 'success',
        },
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations when it lists what stayed behind', async () => {
    const target = container()
    mount(CancelRollbackToastContent, {
      target,
      props: {
        readout: {
          headline: 'Removed 9 items.',
          leftBehind: "Cmdr skips anything it isn't sure about, so these stayed where they are:",
          reasons: [
            'Left notes.md alone: it changed after Cmdr put it there.',
            'Left 3 folders alone: they have something in them now.',
          ],
          recovered: [],
          staged: [],
          level: 'info',
        },
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('TransferRollbackControls a11y', () => {
  /** A controller in one fixed state; the button only reads it. */
  function rollbackIn(blockedTooltip: string | null): TransferRollback {
    return {
      variant: 'stopAndDelete',
      liveTooltip: 'Stops the copy and deletes what it wrote',
      blockedTooltip,
      confirming: false,
      request: vi.fn(),
      confirm: vi.fn(),
      dismiss: vi.fn(),
    }
  }

  it.each([
    ['live', null],
    ['blocked with a reason', 'This move can’t be rolled back'],
  ] as const)('has no a11y violations when %s', async (_label, blockedTooltip) => {
    const target = container()
    mount(TransferRollbackControls, {
      target,
      props: { rollback: rollbackIn(blockedTooltip), isRollingBack: false, disabled: false },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while rolling back', async () => {
    const target = container()
    mount(TransferRollbackControls, {
      target,
      props: { rollback: rollbackIn(null), isRollingBack: true, disabled: true },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TransferStallNotice.svelte`.
 *
 * The warning card at the foot of the progress dialog, with and without the
 * in-flight line, so both shapes of its text column get an axe pass.
 * `TransferProgressDialog.stall.test.ts` covers it in place in the dialog.
 */
describe('TransferStallNotice a11y', () => {
  it.each([
    ['the destination stopped, files in flight', 'destination', 3],
    ['nothing explains it, nothing in flight', 'unknown', 0],
  ] as const)('has no a11y violations when %s', async (_label, reason, inFlight) => {
    const target = container()
    mount(TransferStallNotice, {
      target,
      props: { stall: { stillForSeconds: 42, reason, inFlight }, operationType: 'copy' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * `TransferConflictsSection.svelte`: the setup dialog's "files already exist"
 * block. Audited with everything in the card at once (a clash, a merge, the
 * "Remember this choice" box with its info tip, the remembered-overwrite note,
 * and the mixed-type guardrail), plus the "couldn't find out" notice.
 */
describe('TransferConflictsSection a11y', () => {
  const base = {
    conflictCheckUnknown: false,
    choiceVisible: true,
    totalConflictCount: 3,
    mergeFolderCount: 2,
    namedFolderHint: null,
    hasTypeMismatchConflict: true,
    policy: 'overwrite' as const,
    onPick: () => {},
    remember: true,
    onRememberChange: () => {},
    rememberedOverwriteReach: 'all' as const,
  }

  it('the full card has no a11y violations', async () => {
    const target = container()
    mount(TransferConflictsSection, { target, props: base })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the “couldn’t find out” notice has no a11y violations', async () => {
    const target = container()
    mount(TransferConflictsSection, { target, props: { ...base, conflictCheckUnknown: true, choiceVisible: false } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * `TransferDoneSummary`: the lines under a finished operation's bars in the
 * progress dialog's done state (size, time, and speed; the skips; what the OS
 * refused to trash), as one live region.
 */
describe('TransferDoneSummary a11y', () => {
  const event = {
    operationId: 'op-1',
    operationType: 'trash' as const,
    filesProcessed: 12,
    filesSkipped: 2,
    bytesProcessed: 50_000_000,
    refused: { itemCount: 1, reason: 'notPermitted' as const },
  }

  it('the full summary has no a11y violations', async () => {
    const target = container()
    mount(TransferDoneSummary, { target, props: { event, elapsedMs: 12_000 } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
