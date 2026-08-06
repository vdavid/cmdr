/**
 * The "hide boring folders" tooltip body: the FULL list of excluded directory names, one
 * per line, no "+30 more" truncation (`DETAILS.md` § Search-specific UI behavior).
 *
 * It's rendered as HTML because the tooltip is a rich one, so every name the backend
 * reports is escaped here rather than trusted: a directory name is user data, and one
 * containing `<` would otherwise reach the DOM as markup.
 */

/** Escapes the five characters that could turn a directory name into markup. */
export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/** Builds the tooltip's HTML from the exclude list and its already-resolved heading. */
export function buildSystemDirExcludeTooltip(dirs: string[], heading: string): string {
  const items = dirs
    .map(
      (d) =>
        `<div style="font-family:var(--font-mono);font-size:var(--font-size-xs);color:var(--color-text-secondary);">${escapeHtml(d)}</div>`,
    )
    .join('')
  return (
    '<div style="max-width:360px;max-height:60vh;overflow-y:auto;">' +
    `<div style="font-weight:600;margin-bottom:4px">${escapeHtml(heading)}</div>` +
    items +
    '</div>'
  )
}
