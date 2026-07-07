import type { RenameTarget } from './rename-state.svelte'

/** A list row's identity, as `FullList` / `BriefList` know it while rendering. */
export interface RenameEditorRow {
  index: number
  path: string
}

/**
 * Whether the inline rename editor should mount on `row`. The caller gates on
 * `renameState.active`; this predicate decides identity, shared by both views so
 * they can't drift.
 *
 * Identity is by INDEX today. M3 switches it to `path` so the editor follows its
 * file when a watcher diff inserts or removes OTHER rows above it, instead of
 * staying pinned to a stale index that now points at a different file.
 */
export function shouldMountRenameEditor(target: RenameTarget | null | undefined, row: RenameEditorRow): boolean {
  return target?.index === row.index
}
