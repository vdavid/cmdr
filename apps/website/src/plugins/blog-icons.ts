/**
 * Inline icon registry for blog markdown. Write `:name:` (e.g. `:yes:`) to drop a small brand-colored
 * Lucide glyph inline — handy for scannable comparison tables. There's no real Markdown standard for
 * inline colored icons (emoji shortcodes are the closest convention, hence the `:name:` form), so this
 * is a small controlled set rather than a free-form `color="…"` syntax: authors stay concise and the
 * palette stays consistent. Add an entry here to add an icon; the colors live in global.css
 * (`.md-icon--<name>`).
 *
 * Shared by the rehype plugin (blog-media.ts, builds hast) and the dev-editor preview (entry.ts,
 * builds DOM) so both render identically. Each icon is one or more Lucide `<path d>` strings. Icons
 * are decorative (`aria-hidden`); keep the cell's text so screen readers still get the meaning.
 */

export interface InlineIcon {
  /** One or more Lucide `<path d>` strings drawn at a 24×24 viewBox. */
  paths: string[]
}

export const INLINE_ICONS: Record<string, InlineIcon> = {
  yes: { paths: ['M20 6 9 17l-5-5'] }, // lucide check — green
  no: { paths: ['M18 6 6 18', 'm6 6 12 12'] }, // lucide x — red
  warn: {
    paths: ['m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3', 'M12 9v4', 'M12 17h.01'],
  }, // lucide triangle-alert — amber
  soon: {
    paths: [
      'M5 22h14',
      'M5 2h14',
      'M17 22v-4.172a2 2 0 0 0-.586-1.414L12 12l-4.414 4.414A2 2 0 0 0 7 17.828V22',
      'M7 2v4.172a2 2 0 0 0 .586 1.414L12 12l4.414-4.414A2 2 0 0 0 17 6.172V2',
    ],
  }, // lucide hourglass — blue
}

export const ICON_NAMES: string[] = Object.keys(INLINE_ICONS)

/** Build a fresh global matcher for `:name:` of registered names (callers need their own lastIndex). */
export function inlineIconMatcher(): RegExp {
  return new RegExp(`:(${ICON_NAMES.join('|')}):`, 'g')
}

/** The SVG attributes every inline icon shares (line-art, currentColor — the `.md-icon--*` color). */
export const ICON_SVG_ATTRS = {
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  'stroke-width': '2',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
  'aria-hidden': 'true',
}
