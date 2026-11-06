/**
 * `TransferDialog`'s "Skip Mac-only hidden files" toggle (#314), through the component: it
 * starts from `fileOperations.macOnlyFiles`, its link names that default (or offers to stop
 * asking) and opens the setting row, a flip is a one-off that the confirm carries and the
 * preview walks again for, and rename mode shows none. The policy mapping itself is
 * unit-tested in `mac-only-files.test.ts`; which names match is the backend's.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferDialog from './TransferDialog.svelte'
import type { TransferConfirmPayload } from '#lib/file-explorer/pane/dialog-props.js'
import * as commands from '#lib/tauri-commands/index.js'

const settingsState: { macOnly: unknown } = { macOnly: 'ask' }
const setSettingMock = vi.fn<(id: string, value: unknown) => void>()
const openSettingsWindowMock = vi.fn<(surface: string, section?: string[], anchor?: string) => Promise<void>>(() =>
  Promise.resolve(),
)

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
  scanVolumeForConflicts: vi.fn(() => Promise.resolve([])),
  destinationExists: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  destinationWriteAccess: vi.fn(() => Promise.resolve({ kind: 'unknown' })),
  destinationRootEcho: vi.fn(() => Promise.resolve(null)),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('#lib/settings/index.js', () => ({
  getSetting: (id: string) => (id === 'fileOperations.macOnlyFiles' ? settingsState.macOnly : 500),
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

vi.mock('#lib/settings/settings-window.js', () => ({
  openSettingsWindow: (surface: string, section?: string[], anchor?: string) =>
    openSettingsWindowMock(surface, section, anchor),
  settingAnchorId: (id: string) => `setting-${id}`,
}))

vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

const startScanPreviewMock = vi.mocked(commands.startScanPreview)

function mountDialog(opts: { newName?: string; onConfirm?: (payload: TransferConfirmPayload) => void } = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferDialog, {
    target,
    props: {
      operationType: 'copy',
      sourcePaths: ['/Users/test/photos'],
      destinationPath: '/Users/test/dest',
      currentVolumeId: 'root',
      fileCount: 0,
      folderCount: 1,
      sourceFolderPath: '/Users/test',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      newName: opts.newName,
      onConfirm: opts.onConfirm ?? (() => {}),
      onCancel: () => {},
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

function toggle(target: HTMLElement): HTMLInputElement | null {
  return target.querySelector<HTMLInputElement>('.mac-only-row input[type="checkbox"]')
}

function link(target: HTMLElement): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>('.mac-only-row button')
}

function confirmButton(target: HTMLElement): HTMLButtonElement {
  const btn = target.querySelector<HTMLButtonElement>('.btn-primary')
  if (!btn) throw new Error('confirm button not rendered')
  return btn
}

/** The `macOnlyFiles` argument of every `startScanPreview` call so far. */
function walkedWith(): unknown[] {
  return startScanPreviewMock.mock.calls.map((call) => call[4])
}

beforeEach(() => {
  settingsState.macOnly = 'ask'
  setSettingMock.mockReset()
  openSettingsWindowMock.mockClear()
  startScanPreviewMock.mockClear()
  document.body.innerHTML = ''
})

describe('the toggle starts from the setting', () => {
  it.each([
    ['ask', false, 'Don’t ask again?'],
    ['include', false, 'Including by default'],
    ['skip', true, 'Skipping by default'],
  ] as const)('on %s it starts %s, and the link reads “%s”', async (policy, ticked, linkText) => {
    settingsState.macOnly = policy
    const target = mountDialog()
    await flushMicrotasks()
    expect(toggle(target)?.checked).toBe(ticked)
    expect(link(target)?.textContent.trim()).toBe(linkText)
    expect(walkedWith()).toEqual([ticked ? 'skip' : 'include'])
  })
})

describe('the toggle is a one-off override', () => {
  it('carries a flip to the confirm and walks the preview again for it, never touching the setting', async () => {
    settingsState.macOnly = 'include'
    let sent: TransferConfirmPayload | null = null
    const target = mountDialog({
      onConfirm: (payload) => {
        sent = payload
      },
    })
    await flushMicrotasks()
    toggle(target)?.click()
    await flushMicrotasks()
    expect(walkedWith()).toEqual(['include', 'skip'])

    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent).toMatchObject({ macOnlyFiles: 'skip' })
    expect(setSettingMock).not.toHaveBeenCalled()
  })
})

describe('the link', () => {
  it('opens Settings at the “Skip Mac-only hidden files” row', async () => {
    const target = mountDialog()
    await flushMicrotasks()
    link(target)?.click()
    await flushMicrotasks()
    expect(openSettingsWindowMock).toHaveBeenCalledWith(
      'transfer-dialog',
      ['Behavior', 'Navigation & file ops'],
      'setting-fileOperations.macOnlyFiles',
    )
  })
})

describe('rename mode', () => {
  it('shows no toggle and leaves nothing out, whatever the default', async () => {
    settingsState.macOnly = 'skip'
    let sent: TransferConfirmPayload | null = null
    const target = mountDialog({
      newName: 'renamed',
      onConfirm: (payload) => {
        sent = payload
      },
    })
    await flushMicrotasks()
    expect(toggle(target)).toBeNull()
    confirmButton(target).click()
    await flushMicrotasks()
    expect(sent).toMatchObject({ macOnlyFiles: 'include' })
  })
})
