import type { RenameTarget } from './rename-state.svelte'

/** A list row's identity, as `FullList` / `BriefList` know it while rendering. */
export interface RenameEditorRow {
  path: string
}

/**
 * Whether the inline rename editor should mount on `row`. The caller gates on
 * `renameState.active`; this predicate decides identity, shared by both views so
 * they can't drift.
 *
 * Identity is by PATH: the editor follows its file when a watcher diff inserts or
 * removes OTHER rows (which shifts every row's index but not its path), and never
 * renders on a different file that slid into the target's old index. The row
 * `{#each}` is already keyed by `file.path` and Svelte 5 throws on duplicate keys,
 * so path uniqueness within a listing is an enforced invariant. A diff that changes
 * the TARGET's own path (external rename/delete) is a removal handled by the
 * diff-cancel in `listing-diff-sync`, not a follow.
 */
export function shouldMountRenameEditor(target: RenameTarget | null | undefined, row: RenameEditorRow): boolean {
  return target?.path === row.path
}
