// Picks the operation (move or copy) for a drag-and-drop drop, following Finder conventions:
//   - Option (Alt) held       → Copy (force, beats other modifiers)
//   - Cmd or Shift held       → Move (force; Cmd is Finder, Shift is the Windows accelerator)
//   - Otherwise, same volume  → Move
//   - Otherwise               → Copy

import type { VolumeInfo } from '$lib/file-explorer/types'

export interface ModifierState {
  altHeld: boolean
  cmdHeld: boolean
  shiftHeld: boolean
}

/**
 * Returns the volume ID whose `path` is the longest prefix of `path`, or `null` if none matches.
 * Treats `/` specially so it matches everything but loses to longer mounts (`/Volumes/Foo`).
 */
export function findVolumeIdForPath(path: string, volumes: readonly VolumeInfo[]): string | null {
  let best: VolumeInfo | null = null
  for (const v of volumes) {
    if (!v.path) continue
    const matches = v.path === '/' ? path.startsWith('/') : path === v.path || path.startsWith(`${v.path}/`)
    if (matches && (!best || v.path.length > best.path.length)) {
      best = v
    }
  }
  return best?.id ?? null
}

/** True when source and target paths resolve to the same volume. False if either can't be resolved. */
export function isSameVolume(sourcePath: string, targetPath: string, volumes: readonly VolumeInfo[]): boolean {
  const a = findVolumeIdForPath(sourcePath, volumes)
  if (a === null) return false
  return a === findVolumeIdForPath(targetPath, volumes)
}

/**
 * Picks the drop operation. The `sourcePath` should be the first path in the drag (volume affinity is
 * deterministic and matches the common case of single-volume selections). When the source can't be
 * resolved to a volume, falls back to Copy (the safer default).
 */
export function pickDropOperation(opts: {
  sourcePath: string | null
  targetPath: string | null
  volumes: readonly VolumeInfo[]
  modifiers: ModifierState
}): 'move' | 'copy' {
  const { altHeld, cmdHeld, shiftHeld } = opts.modifiers
  if (altHeld) return 'copy'
  if (cmdHeld || shiftHeld) return 'move'
  if (opts.sourcePath && opts.targetPath && isSameVolume(opts.sourcePath, opts.targetPath, opts.volumes)) {
    return 'move'
  }
  return 'copy'
}
