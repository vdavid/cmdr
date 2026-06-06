/**
 * Shared fixtures for the `drag-drop-controller.svelte.ts` characterization
 * suites, split across `drag-drop-controller.svelte.test.ts` (handler contracts)
 * and `drag-drop-controller.listeners.svelte.test.ts` (init / native listeners).
 *
 * Everything here is independent of the per-file `vi.mock` spies: the volume
 * constants, the `DragDropPayload` shape, and the `buildAccess` / `buildDialogs`
 * / `paneTarget` / `folderTarget` / `flushDrop` builders. Helpers that read the
 * hoisted module-mock spies (`lastOverlayArgs`, `dragDropHandler`,
 * `listenHandler`) stay duplicated in each test file — those spies are hoisted
 * per file and can't be shared through an import.
 */
import { vi } from 'vitest'
import type { DropTarget } from '../drag/drop-target-hit-testing'
import type { PaneAccess } from './pane-access'
import type { VolumeInfo } from '../types'
import type { createDialogState } from './dialog-state.svelte'
import type { TransferDialogPropsData } from './transfer-operations'

type DialogState = ReturnType<typeof createDialogState>
type ShowTransferSpy = ReturnType<typeof vi.fn<(props: TransferDialogPropsData) => void>>

/** Tauri drag-drop event payloads (a structural subset of the real shape). */
export type DragDropPayload =
  | { type: 'enter'; paths: string[]; position: { x: number; y: number } }
  | { type: 'over'; position: { x: number; y: number } }
  | { type: 'drop'; paths: string[]; position: { x: number; y: number } }
  | { type: 'leave' }

export const SAME_VOL_PATH_A = '/Users/x/a'
export const SAME_VOL_PATH_B = '/Users/x/b'
export const EXT_VOL_PATH = '/Volumes/Ext/dest'

export const ROOT_VOLUME: VolumeInfo = {
  id: 'root',
  name: 'Macintosh HD',
  path: '/',
  volumeType: 'local',
  isReadOnly: false,
  supportsTrash: true,
} as unknown as VolumeInfo

export const EXT_VOLUME: VolumeInfo = {
  id: 'ext',
  name: 'Ext',
  path: '/Volumes/Ext',
  volumeType: 'local',
  isReadOnly: false,
  supportsTrash: true,
} as unknown as VolumeInfo

// A read-only MTP SD card and a writable MTP device, both registered with
// `mtp://…` roots so `findVolumeIdForPath` resolves a dropped MTP-shaped path
// to them via longest-prefix.
export const SD_CARD_VOLUME: VolumeInfo = {
  id: 'mtp-dev:65538',
  name: 'Virtual Pixel 9 - SD Card',
  path: 'mtp://dev/65538',
  volumeType: 'mtp',
  isReadOnly: true,
  supportsTrash: false,
} as unknown as VolumeInfo

export const MTP_VOLUME: VolumeInfo = {
  id: 'mtp-dev:65537',
  name: 'Virtual Pixel 9',
  path: 'mtp://dev/65537',
  volumeType: 'mtp',
  isReadOnly: false,
  supportsTrash: false,
} as unknown as VolumeInfo

// An smb2-native SMB share, registered with an `smb://…` root. Its listing paths
// are volume-relative, same class as MTP.
export const SMB_VOLUME: VolumeInfo = {
  id: 'smb-server-share',
  name: 'share on server',
  path: 'smb://server/share',
  category: 'network',
  isReadOnly: false,
  supportsTrash: false,
} as unknown as VolumeInfo

export interface AccessConfig {
  focusedPane?: 'left' | 'right'
  paths?: Partial<Record<'left' | 'right', string>>
  volumeIds?: Partial<Record<'left' | 'right', string>>
  volumes?: VolumeInfo[]
}

export function buildAccess(config: AccessConfig = {}): PaneAccess {
  const otherPane = (pane: 'left' | 'right'): 'left' | 'right' => (pane === 'left' ? 'right' : 'left')
  return {
    getPaneRef: () => undefined,
    getPanePath: (pane) => config.paths?.[pane] ?? (pane === 'left' ? '/left/dir' : '/right/dir'),
    getPaneVolumeId: (pane) => config.volumeIds?.[pane] ?? 'root',
    getPaneSort: () => ({ sortBy: 'name', sortOrder: 'ascending' }),
    getPaneHistory: () => ({ stack: [], currentIndex: 0 }),
    getFocusedPane: () => config.focusedPane ?? 'left',
    otherPane,
    getShowHiddenFiles: () => true,
    getVolumes: () => config.volumes ?? [ROOT_VOLUME],
    focusContainer: vi.fn(),
  }
}

export function buildDialogs(): {
  dialogs: DialogState
  showTransfer: ShowTransferSpy
  showAlert: ReturnType<typeof vi.fn>
} {
  const showTransfer = vi.fn<(props: TransferDialogPropsData) => void>()
  const showAlert = vi.fn<(title: string, message: string) => void>()
  const dialogs = { showTransfer, showAlert } as unknown as DialogState
  return { dialogs, showTransfer, showAlert }
}

/**
 * `handleDrop` fires `handleFileDrop` without awaiting; `handleFileDrop` awaits
 * `statPathsKinds` before opening the dialog. Flush a couple of microtask turns
 * so the dialog open lands before the assertion.
 */
export async function flushDrop(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

export function paneTarget(paneId: 'left' | 'right'): DropTarget {
  return { type: 'pane', paneId }
}

export function folderTarget(path: string, paneId: 'left' | 'right' = 'left'): DropTarget {
  return {
    type: 'folder',
    path,
    paneId,
    element: { classList: { add: vi.fn(), remove: vi.fn() } } as unknown as HTMLElement,
  }
}
