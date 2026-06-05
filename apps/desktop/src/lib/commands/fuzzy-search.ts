/**
 * Fuzzy search for commands using uFuzzy.
 *
 * uFuzzy is optimized for short search phrases against lists of short-to-medium phrases,
 * making it ideal for command palette use cases. It handles typos and out-of-order matches.
 */

import uFuzzy from '@leeoniya/ufuzzy'
import type { CommandMatch } from './types'
import { getPaletteCommands } from './command-registry'

// Configure uFuzzy for command palette behavior
const fuzzy = new uFuzzy({
  intraMode: 1, // Allow fuzzy matching within words (handles typos like "tyoe" → "type")
  interIns: 3, // Max 3 insertions between matched characters
})

/**
 * Search commands with fuzzy matching.
 *
 * @param query - Search query string
 * @param recentCommandIds - Optional list of recently executed command IDs, most-recent first.
 *   When the query is empty, recents (filtered to still-valid palette commands) lead the result,
 *   followed by the remaining palette commands in registry order. Ignored for non-empty queries.
 * @returns Array of matched commands with highlight indices, ordered by relevance
 */
export function searchCommands(query: string, recentCommandIds: string[] = []): CommandMatch[] {
  const paletteCommands = getPaletteCommands()

  // Empty query: show recents first, then the rest of the palette in registry order.
  if (!query.trim()) {
    // Keyed by plain `string`: `recentCommandIds` can hold stale ids (persisted
    // before a command was renamed/removed), and looking those up is exactly how
    // they get filtered out below.
    const byId = new Map<string, (typeof paletteCommands)[number]>(paletteCommands.map((c) => [c.id, c]))
    const recents = recentCommandIds.flatMap((id) => {
      const command = byId.get(id)
      return command ? [{ command, matchedIndices: [] }] : []
    })
    const recentIds = new Set(recents.map((m) => m.command.id))
    const rest = paletteCommands.filter((c) => !recentIds.has(c.id)).map((command) => ({ command, matchedIndices: [] }))
    return [...recents, ...rest]
  }

  // Build haystack from command names plus any extra keywords. A keyword match still ranks and
  // returns the command, but its indices land past `name.length` and get clamped out below so the
  // visible label never shows a bogus highlight.
  const haystack = paletteCommands.map((c) => (c.keywords?.length ? `${c.name} ${c.keywords.join(' ')}` : c.name))

  // Perform fuzzy search
  const [idxs, info, order] = fuzzy.search(haystack, query)

  // No matches
  if (!idxs || !order) {
    return []
  }

  // Map results back to commands with match info
  return order.map((orderIdx) => {
    const haystackIdx = idxs[orderIdx]
    const command = paletteCommands[haystackIdx]

    // Get matched character indices for highlighting
    const matchedIndices: number[] = []

    // uFuzzy's info.ranges contains arrays of [start, end) pairs (end is exclusive)
    const ranges = info.ranges[orderIdx]
    // ranges is an array of [start, end, start, end, ...] pairs where end is exclusive.
    // Clamp to the visible name length: indices that fall in the appended keyword text
    // (>= name.length) would highlight characters the user can't see.
    const nameLength = command.name.length
    for (let i = 0; i < ranges.length; i += 2) {
      const start = ranges[i]
      const end = Math.min(ranges[i + 1], nameLength)
      for (let j = start; j < end; j++) {
        matchedIndices.push(j)
      }
    }

    return { command, matchedIndices }
  })
}
