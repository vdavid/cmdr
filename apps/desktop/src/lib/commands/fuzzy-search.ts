/**
 * Fuzzy search for commands using uFuzzy.
 *
 * uFuzzy is optimized for short search phrases against lists of short-to-medium phrases,
 * making it ideal for command palette use cases. It handles typos and out-of-order matches.
 */

import uFuzzy from '@leeoniya/ufuzzy'
import type { Command, CommandMatch } from './types'
import { commands, getPaletteCommands } from './command-registry'

// Configure uFuzzy for command palette behavior
const fuzzy = new uFuzzy({
  intraMode: 1, // Allow fuzzy matching within words (handles typos like "tyoe" → "type")
  interIns: 3, // Max 3 insertions between matched characters
})

/**
 * Search commands with fuzzy matching.
 *
 * Searches palette-visible commands only — this is the command palette's engine. Surfaces that
 * render the full registry (the shortcuts editor) must use `searchAllCommands` instead, or
 * non-palette commands like "Open command palette" become unfindable.
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

  return fuzzyMatchCommands(paletteCommands, query)
}

/**
 * Search the FULL command registry, including `showInPalette: false` entries.
 *
 * For surfaces whose result set is the whole registry: the shortcuts editor renders (and lets you
 * rebind) every command, so its search must cover the same set. An empty query returns everything
 * in registry order.
 */
export function searchAllCommands(query: string): CommandMatch[] {
  if (!query.trim()) {
    return commands.map((command) => ({ command, matchedIndices: [] }))
  }
  return fuzzyMatchCommands(commands, query)
}

/** Runs uFuzzy over the given commands and maps hits back to `CommandMatch`es. */
function fuzzyMatchCommands(commandList: Command[], query: string): CommandMatch[] {
  // Build haystack from command names plus any extra keywords. A keyword match still ranks and
  // returns the command, but its indices land past `name.length` and get clamped out below so the
  // visible label never shows a bogus highlight.
  const haystack = commandList.map((c) => (c.keywords?.length ? `${c.name} ${c.keywords.join(' ')}` : c.name))

  // Perform fuzzy search
  const [idxs, info, order] = fuzzy.search(haystack, query)

  // No matches
  if (!idxs || !order) {
    return []
  }

  // Map results back to commands with match info
  return order.map((orderIdx) => {
    const haystackIdx = idxs[orderIdx]
    const command = commandList[haystackIdx]

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
