import snarkdown from 'snarkdown'

/**
 * Renders trusted friendly-error markdown to HTML. The input is composed on the
 * FE from a trusted template plus runtime params already escaped via `esc(...)`
 * in the message factories (`lib/errors/`). This is the single `{@html}`-injected
 * render site, so the escaping done in the factories is XSS-load-bearing.
 */
export function renderErrorMarkdown(md: string): string {
  return snarkdown(md)
}
