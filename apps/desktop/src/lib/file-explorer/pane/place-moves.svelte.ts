/**
 * How many times each place moved to a new address in this window, so a pane
 * dialing it knows to dial again (`place-connect.svelte.ts`).
 *
 * ❗ The one signal a move that KEEPS its id leaves (a WebDAV base path that
 * moved): the place's row was `saved` before and after, so nothing else the pane
 * reads changes, and the dial to the old URL that the move called off would leave
 * it on "Not connected". Bumped by `server-move-follow.ts`.
 */

const moves = $state<Record<string, number>>({})

/** Records that `volumeIds` (their NEW ids) just moved. */
export function notePlacesMoved(volumeIds: readonly string[]): void {
  for (const id of volumeIds) moves[id] = (moves[id] ?? 0) + 1
}

/** How many times `volumeId` moved, reactively. */
export function placeMoveCount(volumeId: string): number {
  return moves[volumeId] ?? 0
}
