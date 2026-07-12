/**
 * Pure helpers for attachment chips. Kept separate from the components so the path
 * parsing is unit-testable without mounting Svelte.
 */

/** The last path segment (file or folder name) for a chip label, or the whole path when
 * it has no separator. Trailing slashes are trimmed so a folder path shows its own name. */
export function attachmentBasename(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, '')
  const lastSep = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))
  const name = lastSep >= 0 ? trimmed.slice(lastSep + 1) : trimmed
  return name.length > 0 ? name : path
}
