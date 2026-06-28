/**
 * The single frontend primitive for turning a bare path into a navigable
 * `Location` (volume id + path) at navigation's edges: ⌘G jump, MCP
 * `nav_to_path`, search-result activation, downloads reveal, and search-results
 * row activation. Each edge resolves once, here, then routes the `Location`
 * through `navigate()`.
 *
 * Wraps the typed `resolveLocation` IPC command (via `$lib/tauri-commands`),
 * which runs the full protocol dispatch (`mtp://` / `smb://` / local `statfs`).
 * Failure is honest: the caller gets a typed `{ ok: false }` to turn into a
 * friendly toast or a typed refusal, never a silent wrong-volume listing.
 */

import { resolveLocation as resolveLocationCommand, type Location } from '$lib/tauri-commands'

/** Why a path couldn't be turned into a `Location`. Both map to the same
 * "couldn't reach that location's drive" UX; the distinction is for diagnostics.
 * `no-volume`: resolved, but no volume contains the path (unmounted or gone).
 * `timed-out`: the filesystem (or IPC) didn't respond. */
export type ResolveLocationFailureReason = 'no-volume' | 'timed-out'

export type ResolveLocationOutcome =
  | { ok: true; location: Location }
  | { ok: false; reason: ResolveLocationFailureReason }

/**
 * Resolves a bare path into a `Location`, or an honest failure.
 * @param path - The path to resolve (the dir/parent the caller wants to land on)
 */
export async function resolveLocation(path: string): Promise<ResolveLocationOutcome> {
  const result = await resolveLocationCommand(path)
  if (result.timedOut) return { ok: false, reason: 'timed-out' }
  if (result.location == null) return { ok: false, reason: 'no-volume' }
  return { ok: true, location: result.location }
}
