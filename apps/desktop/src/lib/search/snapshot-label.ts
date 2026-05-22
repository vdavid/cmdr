/**
 * Builds the friendly label shown in the search-results pane's breadcrumb and tab title.
 *
 * Per search-redesign-plan §3.7 plus the search-fixup brief:
 *   - AI mode: the LLM-produced label (`aiLabel`) wins when present (truncated to ~40
 *     chars), falling back to the original prompt (also truncated). The LLM summarizes
 *     intent better than the verbatim phrasing the user typed.
 *   - Filename mode: the pattern as-is (`*.pdf`).
 *   - Regex mode: the pattern wrapped in slashes (`/foo/`).
 *
 * The truncation cap is a soft guideline; we use a single-char ellipsis when we cut so
 * the label still reads cleanly inside the breadcrumb. The `getOrCreate` cap-annotation
 * in `snapshot-store.svelte.ts` appends ` (first N of M)` when entries were truncated;
 * the label this helper returns is the un-annotated base.
 */

import type { SearchSnapshotMode } from './snapshot-store.svelte'

/** Soft cap on AI prompt labels. Long natural-language prompts get truncated with an ellipsis. */
const AI_LABEL_MAX_CHARS = 40

export interface SnapshotLabelInput {
  mode: SearchSnapshotMode
  /** The user's typed query. For AI mode this is the original natural-language prompt; for filename/regex it's the pattern. */
  query: string
  /**
   * The original AI prompt captured before the AI translation overwrote `query`. When the
   * caller has access to it (the dialog does, via `getLastAiPrompt()`), prefer this over
   * `query` for AI mode so the label reflects what the user actually asked.
   */
  aiPrompt?: string | null
  /**
   * LLM-produced label for AI-mode searches (for example "Big PDFs from this week").
   * When set, wins over the raw prompt: the model summarizes the intent better than the
   * verbatim phrasing the user typed. Ignored for filename / regex modes, which keep
   * their pattern-as-label shapes.
   */
  aiLabel?: string | null
}

/** Returns a short, breadcrumb-friendly label for a snapshot. */
export function buildSnapshotLabel(input: SnapshotLabelInput): string {
  const trimmedQuery = input.query.trim()
  if (input.mode === 'ai') {
    const llmLabel = input.aiLabel?.trim()
    if (llmLabel) return truncate(llmLabel, AI_LABEL_MAX_CHARS)
    const prompt = (input.aiPrompt ?? trimmedQuery).trim()
    if (!prompt) return 'Search'
    return truncate(prompt, AI_LABEL_MAX_CHARS)
  }
  if (input.mode === 'regex') {
    return `/${trimmedQuery}/`
  }
  // filename
  return trimmedQuery || 'Search'
}

/** Truncates `text` to `max` chars, appending a single-char ellipsis when it cuts. */
function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  // Reserve one char for the ellipsis, so the visible width stays at `max`.
  return text.slice(0, Math.max(1, max - 1)).trimEnd() + '…'
}
