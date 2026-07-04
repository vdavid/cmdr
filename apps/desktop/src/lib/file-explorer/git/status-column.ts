/**
 * Helpers for the optional Git status column in Full mode.
 *
 * Each entry has a single-glyph code with a long-form `aria-label` and tooltip.
 * Backend ships `EntryStatus` rows from `get_git_status_for_paths`; the
 * `byPath` map indexes them by relative path so `FullList.svelte` can render
 * the cell in O(1).
 */
import { getGitStatusForPaths } from '$lib/tauri-commands'

export type EntryStatusCode =
  | 'modified'
  | 'added'
  | 'deleted'
  | 'renamed'
  | 'copied'
  | 'typechange'
  | 'untracked'
  | 'ignored'
  | 'conflicted'

const codeToGlyph: Record<EntryStatusCode, string> = {
  modified: 'M',
  added: 'A',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
  typechange: 'T',
  untracked: '?',
  ignored: '!',
  conflicted: 'U',
}

const codeToLabel: Record<EntryStatusCode, string> = {
  modified: 'Modified',
  added: 'Added',
  deleted: 'Deleted',
  renamed: 'Renamed',
  copied: 'Copied',
  typechange: 'Type changed',
  untracked: 'Untracked',
  ignored: 'Ignored',
  conflicted: 'Conflicted',
}

export function glyphFor(code: EntryStatusCode): string {
  return codeToGlyph[code]
}

export function labelFor(code: EntryStatusCode): string {
  return codeToLabel[code]
}

/**
 * Fetches a per-path status map for a directory inside a worktree.
 * Returns `null` if the lookup timed out so callers can render a placeholder.
 */
export async function fetchStatusMap(repoRoot: string, dir: string): Promise<Map<string, EntryStatusCode> | null> {
  const result = await getGitStatusForPaths(repoRoot, dir)
  if (result.timedOut) return null
  const map = new Map<string, EntryStatusCode>()
  for (const entry of result.data) {
    // For renames the relative path is `old -> new`; key on the new side.
    const path = entry.relativePath.includes(' -> ') ? entry.relativePath.split(' -> ')[1] : entry.relativePath
    map.set(path, entry.code)
  }
  return map
}
