/**
 * Tests for the Compress-mode compression-level control.
 *
 * Two layers:
 *   1. `CompressLevelControl` on its own: it renders the shared `SettingSlider`
 *      seeded from `behavior.archiveCompressionLevel`, framed by "Faster" /
 *      "Smaller" and a caption, and persists a change through `setSetting` by
 *      id (so the dialog and Settings stay one value, no dialog-local state).
 *   2. `TransferDialog`: the control shows in Compress mode and is absent in
 *      Copy mode.
 *
 * The settings barrel is mocked so `SettingSlider` renders without a store, and
 * (for the dialog) the Tauri IPC, volume store, and path resolver are stubbed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const { getSettingMock, setSettingMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn((key: string): number => (key === 'behavior.archiveCompressionLevel' ? 6 : 500)),
  setSettingMock: vi.fn(),
}))

// Full barrel mock: `SettingSlider` reads its metadata (definition, default) and
// value through `$lib/settings`, and writes back through `setSetting`.
vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
  getDefaultValue: vi.fn(() => 6),
  getSettingDefinition: vi.fn(() => ({
    label: 'Compression level',
    description: '',
    constraints: { min: 1, max: 9, step: 1, sliderStops: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  })),
}))

vi.mock('@tauri-apps/api/path', () => ({
  homeDir: () => Promise.resolve('/Users/test'),
}))

vi.mock('$lib/tauri-commands', () => ({
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
  pathExistsChecked: vi.fn(() => Promise.resolve({ data: true, timedOut: false })),
  DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
}))

import CompressLevelControl from './CompressLevelControl.svelte'
import TransferDialog from './TransferDialog.svelte'

beforeEach(() => {
  getSettingMock.mockReset()
  getSettingMock.mockImplementation((key: string): number => (key === 'behavior.archiveCompressionLevel' ? 6 : 500))
  setSettingMock.mockReset()
})

function mountControl(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(CompressLevelControl, { target })
  return target
}

describe('CompressLevelControl', () => {
  it('renders the slider seeded from the setting, framed by Faster/Smaller with a caption', async () => {
    getSettingMock.mockImplementation((key: string): number => (key === 'behavior.archiveCompressionLevel' ? 9 : 500))
    const target = mountControl()
    await tick()

    const slider = target.querySelector('[role="slider"]')
    expect(slider).not.toBeNull()
    expect(slider?.getAttribute('aria-valuenow')).toBe('9')

    const endLabels = Array.from(target.querySelectorAll('.sl-ends span')).map((el) => el.textContent.trim())
    expect(endLabels).toEqual(['Faster', 'Smaller'])
    expect(target.querySelector('.level-caption')?.textContent).toMatch(/smaller zip/)

    target.remove()
  })

  it('persists a change through setSetting by id (thumb double-click resets to the default)', async () => {
    getSettingMock.mockImplementation((key: string): number => (key === 'behavior.archiveCompressionLevel' ? 3 : 500))
    const target = mountControl()
    await tick()

    const thumb = target.querySelector('.sl-thumb')
    if (!thumb) throw new Error('Slider thumb not found')
    thumb.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await tick()

    expect(setSettingMock).toHaveBeenCalledWith('behavior.archiveCompressionLevel', 6)
    target.remove()
  })
})

async function flushMicrotasks(rounds = 8): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise<void>((resolve) => setTimeout(resolve, 0))
    await tick()
  }
}

function mountDialog(operationType: 'copy' | 'compress'): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferDialog, {
    target,
    props: {
      operationType,
      sourcePaths: ['/Users/test/photos'],
      destinationPath: '/Users/test/dest',
      currentVolumeId: 'root',
      fileCount: 1,
      folderCount: 1,
      sourceFolderPath: '/Users/test',
      sortColumn: 'name',
      sortOrder: 'ascending',
      sourceVolumeId: 'root',
      destVolumeId: 'root',
      autoConfirm: false,
      onConfirm: () => {},
      onCancel: () => {},
    },
  })
  return target
}

describe('TransferDialog compression-level control', () => {
  it('shows the compression-level control in Compress mode', async () => {
    const target = mountDialog('compress')
    await flushMicrotasks()
    expect(target.querySelector('.compress-level')).not.toBeNull()
    expect(target.querySelector('.compress-level [role="slider"]')).not.toBeNull()
    target.remove()
  })

  it('hides the compression-level control in Copy mode', async () => {
    const target = mountDialog('copy')
    await flushMicrotasks()
    expect(target.querySelector('.compress-level')).toBeNull()
    target.remove()
  })
})
