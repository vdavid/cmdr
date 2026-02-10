import { getFileAt, getListingStats } from '$lib/tauri-commands'
import { toBackendIndices, toBackendCursorIndex } from '$lib/file-operations/transfer/transfer-dialog-utils'
import type { SortColumn, SortOrder, TransferOperationType, VolumeInfo } from '../types'
import type FilePane from './FilePane.svelte'

export interface TransferContext {
    showHiddenFiles: boolean
    sourcePath: string
    destPath: string
    sourceVolumeId: string
    destVolumeId: string
    sortColumn: SortColumn
    sortOrder: SortOrder
}

export interface TransferDialogPropsData {
    operationType: TransferOperationType
    sourcePaths: string[]
    destinationPath: string
    direction: 'left' | 'right'
    currentVolumeId: string
    fileCount: number
    folderCount: number
    sourceFolderPath: string
    sortColumn: SortColumn
    sortOrder: SortOrder
    sourceVolumeId: string
    destVolumeId: string
}

interface MtpVolumeInfo {
    id: string
    deviceId: string
    name: string
    isReadOnly: boolean
}

export async function getSelectedFilePaths(
    listingId: string,
    backendIndices: number[],
    showHiddenFiles: boolean,
): Promise<string[]> {
    const paths: string[] = []
    for (const index of backendIndices) {
        const file = await getFileAt(listingId, index, showHiddenFiles)
        if (file && file.name !== '..') {
            paths.push(file.path)
        }
    }
    return paths
}

export async function buildTransferPropsFromSelection(
    operationType: TransferOperationType,
    listingId: string,
    selectedIndices: number[],
    hasParent: boolean,
    isLeft: boolean,
    context: TransferContext,
): Promise<TransferDialogPropsData | null> {
    const backendIndices = toBackendIndices(selectedIndices, hasParent)
    if (backendIndices.length === 0) return null

    const stats = await getListingStats(listingId, context.showHiddenFiles, backendIndices)
    const sourcePaths = await getSelectedFilePaths(listingId, backendIndices, context.showHiddenFiles)
    if (sourcePaths.length === 0) return null

    return {
        operationType,
        sourcePaths,
        destinationPath: context.destPath,
        direction: isLeft ? 'right' : 'left',
        currentVolumeId: context.destVolumeId,
        fileCount: stats.selectedFiles ?? 0,
        folderCount: stats.selectedDirs ?? 0,
        sourceFolderPath: context.sourcePath,
        sortColumn: context.sortColumn,
        sortOrder: context.sortOrder,
        sourceVolumeId: context.sourceVolumeId,
        destVolumeId: context.destVolumeId,
    }
}

export async function buildTransferPropsFromCursor(
    operationType: TransferOperationType,
    listingId: string,
    paneRef: FilePane | undefined,
    hasParent: boolean,
    isLeft: boolean,
    context: TransferContext,
): Promise<TransferDialogPropsData | null> {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorIndex = paneRef?.getCursorIndex?.() as number | undefined
    const backendIndex = toBackendCursorIndex(cursorIndex ?? -1, hasParent)
    if (backendIndex === null) return null

    const file = await getFileAt(listingId, backendIndex, context.showHiddenFiles)
    if (!file || file.name === '..') return null

    return {
        operationType,
        sourcePaths: [file.path],
        destinationPath: context.destPath,
        direction: isLeft ? 'right' : 'left',
        currentVolumeId: context.destVolumeId,
        fileCount: file.isDirectory ? 0 : 1,
        folderCount: file.isDirectory ? 1 : 0,
        sourceFolderPath: context.sourcePath,
        sortColumn: context.sortColumn,
        sortOrder: context.sortOrder,
        sourceVolumeId: context.sourceVolumeId,
        destVolumeId: context.destVolumeId,
    }
}

export function getDestinationVolumeInfo(
    volumeId: string,
    volumes: VolumeInfo[],
    mtpVolumes: MtpVolumeInfo[],
): { name: string; isReadOnly: boolean } | undefined {
    // Check MTP volumes first (they have the isReadOnly flag)
    if (volumeId.startsWith('mtp-')) {
        const mtpVolume = mtpVolumes.find((v) => v.id === volumeId || v.deviceId === volumeId)
        if (mtpVolume) {
            return { name: mtpVolume.name, isReadOnly: mtpVolume.isReadOnly }
        }
    }

    // Regular volumes
    const volume = volumes.find((v) => v.id === volumeId)
    if (volume) {
        return { name: volume.name, isReadOnly: volume.isReadOnly ?? false }
    }

    return undefined
}
