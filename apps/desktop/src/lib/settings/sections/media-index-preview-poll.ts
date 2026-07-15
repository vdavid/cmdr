/**
 * Pure decision for the image-index slider's covered-count preview: should it keep
 * re-polling?
 *
 * The preview fetches once on mount and on slider changes, but the backend answer can
 * be `pending` (a drive is still scanning, or importance hasn't scored the volume
 * yet), or the first fetch may not have landed (`null`). The per-volume progress line
 * polls on a timer; the preview folds a re-fetch into that same timer ONLY while it's
 * unresolved, so a `pending` result resolves on its own instead of sitting forever
 * (plan M1). Once resolved, polling stops — the pass-completion invalidation keeps
 * later fetches honest.
 */
import type { CoveredCount } from '$lib/tauri-commands'

/** Whether the covered-count preview is unresolved and should be re-polled. */
export function shouldRepollPreview(covered: CoveredCount | null): boolean {
  return covered === null || covered.pending
}
