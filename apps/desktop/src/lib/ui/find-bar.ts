/**
 * The find bar's one piece of shared logic: stepping the current match. Every owner of a
 * `FindBar` (the viewer, the Ask Cmdr rail) steps through the same way, so a press past
 * the last match wraps to the first and back.
 */

/** Which way Next / Previous moves. */
export type FindDirection = 'next' | 'previous'

/**
 * The match index after one step `direction` from `current`, wrapping at both ends. With
 * no current match (`-1`), Next lands on the first and Previous on the last. `-1` when
 * there are no matches.
 */
export function stepMatchIndex(current: number, total: number, direction: FindDirection): number {
  if (total <= 0) return -1
  if (current < 0) return direction === 'next' ? 0 : total - 1
  const step = direction === 'next' ? 1 : -1
  return (current + step + total) % total
}
