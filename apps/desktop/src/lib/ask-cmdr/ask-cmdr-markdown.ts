/**
 * Markdown-lite rendering for assistant prose — the XSS boundary for untrusted model text.
 *
 * The model's whole reply is untrusted (and a crafted filename it echoes is an injection
 * vector). It is rendered through `snarkdown` + `{@html}`, so the input MUST be neutralized
 * first. But unlike the friendly-error path (trusted template + escaped params), here we
 * also WANT the model's own markdown-lite to render (bold, italic, inline code, lists —
 * spec §3). So this escaper is narrower than `errors/markdown-escape.ts`: it escapes only
 * the HTML- and link-forming characters (`&`, `<`, `>`, `[`, `]`), which
 *
 * - stops the model from injecting raw HTML (`<script>`, `<img onerror>`) — `<`/`>` become
 *   entities, so snarkdown never sees a tag, and
 * - stops link/image syntax (`[x](javascript:…)`, `![…]`) from forming — `[`/`]` are gone,
 *   so snarkdown emits no `<a>`/`<img>` with an attacker-controlled URL,
 *
 * while leaving `*`, `_`, `` ` ``, and line-start `-`/`#` intact so snarkdown renders
 * bold/italic/inline-code/lists/headings. snarkdown's OUTPUT tags (from that reduced set)
 * are a safe fixed vocabulary. Links aren't in the markdown-lite spec, so dropping them is
 * both safe and faithful.
 */

import snarkdown from 'snarkdown'

/** Encode the HTML- and link-forming characters as numeric entities (snarkdown ignores
 * backslash escapes, so entities are the reliable neutralizer). `&` is encoded first so a
 * preexisting entity-like run can't survive. */
export function escapeForMarkdownLite(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('[', '&#91;')
    .replaceAll(']', '&#93;')
}

/** Render one chunk of assistant text as markdown-lite HTML, safe to `{@html}`. */
export function renderAssistantMarkdown(text: string): string {
  return snarkdown(escapeForMarkdownLite(text))
}
