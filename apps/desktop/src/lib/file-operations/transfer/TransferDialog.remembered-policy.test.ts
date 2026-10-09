/**
 * `TransferDialog`'s remembered "files already exist" choice, end to end through
 * the component: it opens on `fileOperations.defaultConflictPolicy`, a person's
 * confirmed pick becomes the new value, MCP paths neither read nor write it, and
 * a remembered overwriting policy is flagged in words. The rules themselves are
 * unit-tested in `remembered-conflict-policy.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import type { VolumeConflictInfo } from '$lib/tauri-commands'
import type { ConflictResolution } from '$lib/file-explorer/types'
import type { TransferConfirmPayload, TransferConfirmer } from '$lib/file-explorer/pane/dialog-props'

const POLICY_SETTING = 'fileOperations.defaultConflictPolicy'

const settingsState: { policy: unknown } = { policy: 'stop' }
const setSettingMock = vi.fn<(id: string, value: unknown) => void>()
const getSettingMock = vi.fn<(id: string) => unknown>()

const scanVolumeForConflictsMock = vi.fn<() => Promise<VolumeConflictInfo[]>>(() => Promise.resolve([]))

vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

vi.mock('$lib/tauri-commands', () => ({
  estimateOperationCost: vi.fn(() => Promise.resolve([])),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getVolumeSpace: vi.fn(() =>
    Promise.resolve({ data: { totalBytes: 1024 * 1024 * 1024, availableBytes: 1024 * 1024 * 500 } }),
  ),
  startScanPreview: vi.fn(() => Promise.resolve({ previewId: 'preview-1' })),
  cancelScanPreview: vi.fn(() => Promise.resolve()),
  checkScanPreviewStatus: vi.fn(() => Promise.resolve(null)),
  onScanPreviewProgress: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewComplete: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewError: vi.fn(() => Promise.resolve(() => {})),
  onScanPreviewCancelled: vi.fn(() => Promise.resolve(() => {})),
  scanVolumeForConflicts: () => scanVolumeForConflictsMock(),
  destinationExists: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  destinationWriteAccess: vi.fn(() => Promise.resolve({ kind: 'unknown' })),
  destinationRootEcho: vi.fn(() => Promise.resolve(null)),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => getSettingMock(id),
  setSetting: (id: string, value: unknown) => {
    setSettingMock(id, value)
  },
  getDefaultValue: vi.fn(() => 6),
  onSpecificSettingChange: vi.fn(() => () => {}),
  getSettingDefinition: vi.fn(() => ({
    label: 'Compression level',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  })),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

const FILE_CLASH: VolumeConflictInfo = {
  sourcePath: 'notes.txt',
  destPath: 'notes.txt',
  sourceSize: 10,
  destSize: 20,
  sourceModified: null,
  destModified: null,
  sourceIsDirectory: false,
  destIsDirectory: false,
}

interface MountOpts {
  autoConfirm?: boolean
  autoConfirmOnConflict?: string
  onConfirm?: (payload: TransferConfirmPayload) => void
  onCancel?: () => void
  registerConfirmer?: (confirm: TransferConfirmer) => () => void
}

function mountDialog(opts: MountOpts = {}): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/notes.txt'],
      destinationPath: '/Users/test/dest',
      currentVolumeId: 'root',
      fileCount: 1,
      folderCount: 0,
      sourceFolderPath: '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      autoConfirm: opts.autoConfirm ?? false,
      autoConfirmOnConflict: opts.autoConfirmOnConflict,
      registerConfirmer: opts.registerConfirmer,
      onConfirm: opts.onConfirm ?? (() => {}),
      onCancel: opts.onCancel ?? (() => {}),
    },
  })
  return target
}

async function flushMicrotasks(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0)
    })
    await tick()
  }
}

function radio(target: HTMLElement, policy: ConflictResolution): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>(`.conflict-policy input[type="radio"][value="${policy}"]`)
  if (!input) throw new Error(`radio ${policy} not rendered`)
  return input
}

function checkedPolicy(target: HTMLElement): string | null {
  return target.querySelector<HTMLInputElement>('.conflict-policy input[type="radio"]:checked')?.value ?? null
}

function note(target: HTMLElement): HTMLElement | null {
  return target.querySelector('.remembered-overwrite-note')
}

function confirmButton(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.btn-primary')
  if (!btn) throw new Error('confirm button not rendered')
  return btn
}

function cancelButton(target: HTMLElement): HTMLButtonElement {
  const btn = Array.from(target.querySelectorAll<HTMLButtonElement>('button')).find(
    (b) => b.textContent.trim() === 'Cancel',
  )
  if (!btn) throw new Error('cancel button not rendered')
  return btn
}

/** Mounts with one file clash and waits for the radios, capturing what a confirm sends. */
async function mountWithClash(opts: MountOpts = {}): Promise<{
  target: HTMLDivElement
  sent: () => ConflictResolution | null
}> {
  scanVolumeForConflictsMock.mockResolvedValue([FILE_CLASH])
  const captured: { resolution: ConflictResolution | null } = { resolution: null }
  const target = mountDialog({
    ...opts,
    onConfirm: (payload) => {
      captured.resolution = payload.conflictResolution
      opts.onConfirm?.(payload)
    },
  })
  await flushMicrotasks()
  return { target, sent: () => captured.resolution }
}

beforeEach(() => {
  settingsState.policy = 'stop'
  getSettingMock.mockReset()
  getSettingMock.mockImplementation((id) => (id === POLICY_SETTING ? settingsState.policy : 500))
  setSettingMock.mockReset()
  scanVolumeForConflictsMock.mockReset()
  scanVolumeForConflictsMock.mockResolvedValue([])
  document.body.innerHTML = ''
})

describe('TransferDialog opens on the remembered policy', () => {
  it.each(['stop', 'skip', 'overwrite', 'overwrite_smaller', 'overwrite_older'] as const)(
    'selects a saved %s',
    async (policy) => {
      settingsState.policy = policy
      const { target } = await mountWithClash()
      expect(checkedPolicy(target)).toBe(policy)
    },
  )

  it('sends the remembered policy when the person confirms without touching it', async () => {
    settingsState.policy = 'skip'
    const { target, sent } = await mountWithClash()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent()).toBe('skip')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('asks per file when nothing clashes, so an unseen remembered overwrite never goes out', async () => {
    settingsState.policy = 'overwrite'
    const captured: { resolution: ConflictResolution | null } = { resolution: null }
    const target = mountDialog({
      onConfirm: ({ conflictResolution }) => {
        captured.resolution = conflictResolution
      },
    })
    await flushMicrotasks()
    expect(target.querySelector('.conflict-policy')).toBeNull()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(captured.resolution).toBe('stop')
  })
})

describe('TransferDialog remembers a confirmed pick', () => {
  it('writes the new pick when the person confirms', async () => {
    const { target, sent } = await mountWithClash()
    radio(target, 'overwrite_older').click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent()).toBe('overwrite_older')
    expect(setSettingMock).toHaveBeenCalledTimes(1)
    expect(setSettingMock).toHaveBeenCalledWith(POLICY_SETTING, 'overwrite_older')
  })

  it('writes nothing when the person picks and then cancels', async () => {
    const onCancel = vi.fn()
    const { target } = await mountWithClash({ onCancel })
    radio(target, 'overwrite').click()
    await tick()
    cancelButton(target).click()
    await flushMicrotasks()
    expect(onCancel).toHaveBeenCalled()
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('writes nothing when the pick lands back on the saved choice', async () => {
    settingsState.policy = 'skip'
    const { target } = await mountWithClash()
    radio(target, 'overwrite').click()
    await tick()
    radio(target, 'skip').click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('TransferDialog MCP paths keep their own policy', () => {
  it('auto-confirm sends the named policy, ignoring and never writing the saved one', async () => {
    settingsState.policy = 'overwrite'
    const { sent } = await mountWithClash({ autoConfirm: true, autoConfirmOnConflict: 'skip_all' })
    expect(sent()).toBe('skip')
    expect(getSettingMock).not.toHaveBeenCalledWith(POLICY_SETTING)
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('auto-confirm with no named policy asks per file, whatever is saved', async () => {
    settingsState.policy = 'overwrite'
    const { sent } = await mountWithClash({ autoConfirm: true })
    expect(sent()).toBe('stop')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('a `dialog confirm` sends the agent’s policy and writes nothing, even over a person’s pick', async () => {
    const confirmer: { press: TransferConfirmer | null } = { press: null }
    const { target, sent } = await mountWithClash({
      registerConfirmer: (confirm) => {
        confirmer.press = confirm
        return () => {}
      },
    })
    radio(target, 'skip').click()
    await tick()
    confirmer.press?.('overwrite_smaller')
    await flushMicrotasks()
    expect(sent()).toBe('overwrite_smaller')
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('TransferDialog flags a remembered overwrite', () => {
  it.each([
    ['overwrite', 'overwrites the files already there'],
    ['overwrite_smaller', 'that are smaller'],
    ['overwrite_older', 'that are older'],
  ] as const)('shows the note for a saved %s and ties it to the radio group', async (policy, words) => {
    settingsState.policy = policy
    const { target } = await mountWithClash()
    const shown = note(target)
    expect(shown, 'note rendered').not.toBeNull()
    expect(shown?.textContent).toContain(words)
    const id = shown?.id ?? ''
    expect(id).not.toBe('')
    const group = target.querySelector('.conflict-policy [role="radiogroup"]')
    expect(group?.getAttribute('aria-describedby')).toBe(id)
  })

  it.each(['stop', 'skip'] as const)('shows no note for a saved %s', async (policy) => {
    settingsState.policy = policy
    const { target } = await mountWithClash()
    expect(note(target)).toBeNull()
    expect(target.querySelector('.conflict-policy [role="radiogroup"]')?.hasAttribute('aria-describedby')).toBe(false)
  })

  it('drops the note once the person picks, even another overwriting choice', async () => {
    settingsState.policy = 'overwrite'
    const { target } = await mountWithClash()
    expect(note(target)).not.toBeNull()
    radio(target, 'overwrite_older').click()
    await tick()
    expect(note(target)).toBeNull()
  })

  it('shows no note for an overwrite an MCP caller named', async () => {
    const { target } = await mountWithClash({ autoConfirm: true, autoConfirmOnConflict: 'overwrite_all' })
    expect(note(target)).toBeNull()
  })
})
