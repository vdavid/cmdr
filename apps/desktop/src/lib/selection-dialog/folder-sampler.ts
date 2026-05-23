/**
 * Folder sampling for Selection's AI mode.
 *
 * The AI translator needs a few filenames to ground its glob/regex. We send a
 * representative sample (not the full folder, which could be 100k+ entries):
 *
 *   - 0-200 entries: all of them.
 *   - 201+ entries: first 200, plus 20 around the cursor (cursorIndex ± 10),
 *     plus 20 from the end of the list.
 *
 * The sample is de-duplicated; order doesn't matter for the LLM. Output capped at
 * `max` (default 240) so a runaway sample can't blow the prompt token budget.
 *
 * Pure function. Deterministic: no `Math.random`, no time-based logic.
 */

const FIRST_BUCKET = 200
const CURSOR_RADIUS = 10
const TAIL_BUCKET = 20
const DEFAULT_MAX = 240

export function sampleFolderNames(names: string[], cursorIndex: number, max: number = DEFAULT_MAX): string[] {
  if (names.length === 0) return []

  // Small folder: return everything (deduped, capped).
  if (names.length <= FIRST_BUCKET) {
    return dedup(names).slice(0, max)
  }

  const picked: string[] = []
  // First N.
  for (let i = 0; i < FIRST_BUCKET && i < names.length; i++) {
    picked.push(names[i])
  }
  // Cursor band: 20 entries centered on the cursor (cursor - 10 .. cursor + 9, half-open),
  // so the three buckets add up to 200 + 20 + 20 = 240 (== DEFAULT_MAX). Clamp to
  // [0, names.length) so a cursor near the start or end doesn't reach into negative
  // indices or past the end.
  if (cursorIndex >= 0 && cursorIndex < names.length) {
    const bandStart = Math.max(0, cursorIndex - CURSOR_RADIUS)
    const bandEnd = Math.min(names.length, cursorIndex + CURSOR_RADIUS)
    for (let i = bandStart; i < bandEnd; i++) {
      picked.push(names[i])
    }
  }
  // Tail.
  const tailStart = Math.max(0, names.length - TAIL_BUCKET)
  for (let i = tailStart; i < names.length; i++) {
    picked.push(names[i])
  }
  return dedup(picked).slice(0, max)
}

function dedup(arr: string[]): string[] {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- pure helper, not reactive state
  const seen = new Set<string>()
  const out: string[] = []
  for (const s of arr) {
    if (seen.has(s)) continue
    seen.add(s)
    out.push(s)
  }
  return out
}
