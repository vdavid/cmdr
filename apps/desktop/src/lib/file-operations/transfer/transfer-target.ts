/** Resolve the complete single-item Copy/Move target before preflight or dispatch. */
import type { VolumeInfo } from '$lib/file-explorer/types'
import { capabilitiesForPane } from '$lib/file-explorer/pane/volume-capabilities'
import { isPathOnVolume } from '$lib/path/canonical'
import { DEFAULT_VOLUME_ID } from '$lib/tauri-commands'
import { containingFolder, getFolderName, resolveTransferFilename, toVolumeRelativePath } from './transfer-dialog-utils'

function volumeRoot(volumes: VolumeInfo[], id: string): string {
  return volumes.find((v) => v.id === id)?.path ?? '/'
}

function parentForVolume(parent: string, root: string, localTarget: boolean): string {
  return localTarget || isPathOnVolume(parent, root) ? toVolumeRelativePath(parent, root) : parent
}

export function resolveTransferTarget(args: {
  enteredPath: string
  sourceFolderPath: string
  sourcePath: string
  sourceVolumeId: string
  selectedVolumeId: string
  volumes: VolumeInfo[]
  homePath: string
}): { parent: string; name: string; volumeId: string; path: string; fullPath: string } | null {
  const { enteredPath, sourceFolderPath, sourcePath, sourceVolumeId, selectedVolumeId, volumes, homePath } = args
  const entered = enteredPath.trim()
  const relative = !entered.startsWith('/') && !entered.startsWith('~/')
  const sourceRoot = volumeRoot(volumes, sourceVolumeId)
  const sourceFolder = containingFolder(sourcePath) ?? sourceFolderPath
  const sourceIsLocal = capabilitiesForPane(sourceVolumeId, sourceFolder).kind === 'local'
  const named = resolveTransferFilename(
    entered,
    sourceIsLocal ? sourceFolder : parentForVolume(sourceFolder, sourceRoot, false),
    homePath,
    getFolderName(sourcePath),
  )
  if (!named) return null
  const selectedRoot = volumeRoot(volumes, selectedVolumeId)
  const localTarget = relative
    ? sourceIsLocal
    : entered.startsWith('~/') || capabilitiesForPane(selectedVolumeId, selectedRoot).kind === 'local'
  let volumeId = relative ? sourceVolumeId : selectedVolumeId
  if (localTarget) {
    volumeId =
      volumes
        .filter(
          (v) =>
            v.category !== 'favorite' &&
            capabilitiesForPane(v.id, v.path).kind === 'local' &&
            isPathOnVolume(named.parent, v.path),
        )
        .sort((a, b) => b.path.length - a.path.length)[0]?.id ?? DEFAULT_VOLUME_ID
  }
  const root = volumeRoot(volumes, volumeId)
  const path = parentForVolume(named.parent, root, localTarget)
  const fullParent = root === '/' ? path : `${root.replace(/\/+$/, '')}${path === '/' ? '' : path}`
  return {
    ...named,
    volumeId,
    path,
    fullPath: `${fullParent.replace(/\/+$/, '')}/${named.name}`,
  }
}

/**
 * A single-item Move whose target stays in the item's own folder on the same volume
 * is a rename, so the pane's rename flow runs it (`dialog-state.svelte.ts`): the
 * same conflict and extension asks as F2, a case-only change on a case-folding
 * volume, and one undo row. S3 keeps the transfer, which is what its rename runs on.
 */
export function isRenameInPlace(args: {
  target: { volumeId: string; fullPath: string } | null
  sourcePath: string
  sourceVolumeId: string
}): boolean {
  const { target, sourcePath, sourceVolumeId } = args
  if (!target || target.volumeId !== sourceVolumeId) return false
  const sourceFolder = containingFolder(sourcePath)
  if (sourceFolder === null || capabilitiesForPane(sourceVolumeId, sourceFolder).kind === 's3') return false
  const trim = (path: string) => path.replace(/\/+$/, '') || '/'
  return trim(containingFolder(target.fullPath) ?? '') === trim(sourceFolder) && target.fullPath !== sourcePath
}
