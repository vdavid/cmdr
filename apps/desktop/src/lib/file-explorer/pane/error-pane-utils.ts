import snarkdown from 'snarkdown'

/**
 * Renders markdown from friendly error messages to HTML.
 * Input is always hardcoded strings from Rust, never user-generated content.
 */
export function renderErrorMarkdown(md: string): string {
  return snarkdown(md)
}
