import snarkdown from 'snarkdown'
import type { Markdown } from '$lib/ipc/bindings'

/**
 * Renders trusted markdown from friendly error messages to HTML.
 *
 * The `Markdown` brand (see `markdown.rs` on the Rust side and the
 * post-processed bindings.ts) guarantees the input came from a backend
 * `md!(...)` site, which auto-escapes interpolated runtime values. Plain
 * strings are rejected at the type level.
 */
export function renderErrorMarkdown(md: Markdown): string {
  return snarkdown(md)
}
