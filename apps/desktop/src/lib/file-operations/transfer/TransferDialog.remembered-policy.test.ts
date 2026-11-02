/**
 * `TransferDialog`'s remembered "files already exist" choice, end to end through
 * the component: it opens on `fileOperations.defaultConflictPolicy`, a confirm
 * saves the pick only under a ticked "Remember this choice" (and forgets it when
 * unticked), MCP paths neither read nor write it, and a remembered overwriting
 * policy is flagged in words. The rules themselves are
 * unit-tested in `remembered-conflict-policy.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import type { VolumeConflictInfo } from '#lib/tauri-commands/index.js'
import type { ConflictResolution } from '#lib/file-explorer/types.js'
import type { TransferConfirmPayload, TransferConfirmer } from '#lib/file-explorer/pane/dialog-props.js'

const POLICY_SETTING = 'fileOperations.defaultConflictPolicy'

const settingsState: { policy: unknown } = { policy: 'stop' }
const setSettingMock = vi.fn<(id: string, value: unknown) => void>()
const getSettingMock = vi.fn<(id: string) => unknown>()

const scanVolumeForConflictsMock = vi.fn<() => Promise<VolumeConflictInfo[]>>(() => Promise.resolve([]))

vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
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

vi.mock('#lib/settings/index.js', () => ({
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

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
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
  presetOnConflict?: string
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
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      presetOnConflict: opts.presetOnConflict,
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

function rememberBox(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('.remember-choice input[type="checkbox"]')
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

describe('TransferDialog’s “Remember this choice” box', () => {
  it('starts unticked, and hidden while “Ask later” is selected', async () => {
    const { target } = await mountWithClash()
    expect(checkedPolicy(target)).toBe('stop')
    expect(rememberBox(target)).toBeNull()
    radio(target, 'skip').click()
    await tick()
    expect(rememberBox(target)?.checked).toBe(false)
  })

  it('starts ticked when a choice is saved', async () => {
    settingsState.policy = 'overwrite_older'
    const { target } = await mountWithClash()
    expect(rememberBox(target)?.checked).toBe(true)
  })

  it('hides again (rather than disabling) once the person picks “Ask later”', async () => {
    settingsState.policy = 'skip'
    const { target } = await mountWithClash()
    radio(target, 'stop').click()
    await tick()
    expect(rememberBox(target)).toBeNull()
  })

  it('carries an info tip beside it, outside its label', async () => {
    settingsState.policy = 'skip'
    const { target } = await mountWithClash()
    const tip = target.querySelector('.remember-choice button[aria-label]')
    expect(tip).not.toBeNull()
    expect(tip?.closest('label')).toBeNull()
  })

  it('saves the pick when the person ticks it and confirms', async () => {
    const { target, sent } = await mountWithClash()
    radio(target, 'overwrite_older').click()
    await tick()
    rememberBox(target)?.click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent()).toBe('overwrite_older')
    expect(setSettingMock).toHaveBeenCalledTimes(1)
    expect(setSettingMock).toHaveBeenCalledWith(POLICY_SETTING, 'overwrite_older')
  })

  it('saves nothing when the person picks without ticking it', async () => {
    const { target, sent } = await mountWithClash()
    radio(target, 'overwrite').click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent()).toBe('overwrite')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('forgets the saved choice when the person unticks it and confirms', async () => {
    settingsState.policy = 'skip'
    const { target, sent } = await mountWithClash()
    rememberBox(target)?.click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent()).toBe('skip')
    expect(setSettingMock).toHaveBeenCalledWith(POLICY_SETTING, 'stop')
  })

  it('saves on a Background start too', async () => {
    const { target } = await mountWithClash()
    radio(target, 'skip').click()
    await tick()
    rememberBox(target)?.click()
    await tick()
    target.querySelector<HTMLButtonElement>('button[aria-keyshortcuts="F2"]')?.click()
    await flushMicrotasks()
    expect(setSettingMock).toHaveBeenCalledWith(POLICY_SETTING, 'skip')
  })

  it('saves nothing when the person ticks it and then cancels', async () => {
    const onCancel = vi.fn()
    const { target } = await mountWithClash({ onCancel })
    radio(target, 'overwrite').click()
    await tick()
    rememberBox(target)?.click()
    await tick()
    cancelButton(target).click()
    await flushMicrotasks()
    expect(onCancel).toHaveBeenCalled()
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('saves nothing when the confirm lands on the saved choice', async () => {
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
  it("a preset takes the saved policy's place, and a confirm sends it without saving it", async () => {
    settingsState.policy = 'overwrite'
    const { target, sent } = await mountWithClash({ presetOnConflict: 'skip_all' })
    expect(radio(target, 'skip').checked).toBe(true)

    confirmButton(target).click()
    await flushMicrotasks()

    expect(sent()).toBe('skip')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('a preset never saves, even with "Remember this choice" ticked', async () => {
    settingsState.policy = 'overwrite_older'
    const { target } = await mountWithClash({ presetOnConflict: 'overwrite_all' })
    expect(rememberBox(target)?.checked).toBe(true)

    confirmButton(target).click()
    await flushMicrotasks()

    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('a person picking over a preset makes it theirs, saved by the box as usual', async () => {
    settingsState.policy = 'overwrite_older'
    const { target, sent } = await mountWithClash({ presetOnConflict: 'overwrite_all' })

    radio(target, 'skip').click()
    await tick()
    confirmButton(target).click()
    await flushMicrotasks()

    expect(sent()).toBe('skip')
    expect(setSettingMock).toHaveBeenCalledWith(POLICY_SETTING, 'skip')
  })

  it('a preset falls back to asking per file while the conflict check is pending', async () => {
    settingsState.policy = 'stop'
    scanVolumeForConflictsMock.mockReturnValue(new Promise(() => {}))
    let sent: ConflictResolution | null = null
    const target = mountDialog({
      presetOnConflict: 'overwrite_all',
      onConfirm: (payload) => {
        sent = payload.conflictResolution
      },
    })
    await flushMicrotasks()

    // An Enter here must not send a model-chosen overwrite with nothing on screen.
    confirmButton(target).click()
    await flushMicrotasks()

    expect(sent).toBe('stop')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('a `dialog_confirm` naming a policy overrides the preset', async () => {
    const confirmer: { press: TransferConfirmer | null } = { press: null }
    const { sent } = await mountWithClash({
      presetOnConflict: 'overwrite_all',
      registerConfirmer: (confirm) => {
        confirmer.press = confirm
        return () => {}
      },
    })

    await confirmer.press?.('skip')
    await flushMicrotasks()

    expect(sent()).toBe('skip')
    expect(setSettingMock).not.toHaveBeenCalled()
  })

  it('a `dialog_confirm` with no policy named sends the one the dialog shows', async () => {
    settingsState.policy = 'overwrite_older'
    const confirmer: { press: TransferConfirmer | null } = { press: null }
    const { sent } = await mountWithClash({
      registerConfirmer: (confirm) => {
        confirmer.press = confirm
        return () => {}
      },
    })
    void confirmer.press?.(undefined)
    await flushMicrotasks()
    expect(sent()).toBe('overwrite_older')
  })

  it('a `dialog_confirm` sends the agent’s policy and writes nothing, even over a person’s ticked pick', async () => {
    const confirmer: { press: TransferConfirmer | null } = { press: null }
    const { target, sent } = await mountWithClash({
      registerConfirmer: (confirm) => {
        confirmer.press = confirm
        return () => {}
      },
    })
    radio(target, 'skip').click()
    await tick()
    rememberBox(target)?.click()
    await tick()
    void confirmer.press?.('overwrite_smaller')
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

  it('shows a preset overwrite its own note, gone once the person picks', async () => {
    const { target } = await mountWithClash({ presetOnConflict: 'overwrite_older_all' })
    expect(note(target)?.textContent).toContain(
      'This dialog opened set to overwrite files already there that are older',
    )

    radio(target, 'skip').click()
    await tick()
    expect(note(target)).toBeNull()
  })

  it('shows no note for a preset that overwrites nothing', async () => {
    const { target } = await mountWithClash({ presetOnConflict: 'skip_all' })
    expect(note(target)).toBeNull()
  })
})
