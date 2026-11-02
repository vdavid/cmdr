/**
 * Whether a dialog opening right now may be a tool's doing, for the two sheets a tool only raises
 * indirectly: `select_volume` onto a pinned server asks for a sign-in, and a path into an encrypted
 * archive asks for its password. Both are several async steps away from the tool, with no caller in
 * the arguments, and a stray Enter there is a chat sentence sent as a password attempt.
 *
 * So it reads two facts instead of threading an initiator through navigation and connecting. Either
 * can be wrong in the safe direction only: a person's own sheet that opens in the window takes one
 * click or Tab before it takes text.
 */

/**
 * How long after a tool navigates a sheet counts as its doing. A dial can take this long. Past it,
 * the rail check still covers someone who's typing there.
 */
export const TOOL_NAVIGATION_WINDOW_MS = 30_000

let lastToolNavigationAt: number | null = null

/** A tool just asked a pane to go somewhere. */
export function noteToolNavigation(): void {
  lastToolNavigationAt = Date.now()
}

/** Whether the Ask Cmdr rail holds focus: nobody typing there asked for a sheet. */
function railHoldsFocus(): boolean {
  return document.activeElement?.closest('.ask-cmdr-rail') != null
}

export function mayBeToolRaised(): boolean {
  if (railHoldsFocus()) return true
  return lastToolNavigationAt !== null && Date.now() - lastToolNavigationAt < TOOL_NAVIGATION_WINDOW_MS
}
