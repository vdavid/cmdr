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
 * @returns Array of matched commands with highlight indices, ordered by relevance
 */
export function searchCommands(query: string): CommandMatch[] {
    const paletteCommands = getPaletteCommands()

    // Empty query returns all commands (no highlighting)
    if (!query.trim()) {
        return paletteCommands.map((command) => ({
            command,
            matchedIndices: [],
        }))
    }

    // Build haystack from command names
    const haystack = paletteCommands.map((c) => c.name)

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
        // ranges is an array of [start, end, start, end, ...] pairs where end is exclusive
        for (let i = 0; i < ranges.length; i += 2) {
            const start = ranges[i]
            const end = ranges[i + 1]
            for (let j = start; j < end; j++) {
                matchedIndices.push(j)
            }
        }

        return { command, matchedIndices }
    })
}
