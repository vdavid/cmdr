/** A single search match location. */
export interface SearchMatch {
    /** 0-based line index */
    line: number
    /** 0-based character offset within the line */
    start: number
    /** Length of the match in characters */
    length: number
}

/** Finds all occurrences of a query in the given lines (case-insensitive). */
export function findMatches(lines: string[], query: string): SearchMatch[] {
    if (!query) return []
    const lowerQuery = query.toLowerCase()
    const matches: SearchMatch[] = []
    for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
        const lowerLine = lines[lineIdx].toLowerCase()
        let offset = 0
        while (offset < lowerLine.length) {
            const idx = lowerLine.indexOf(lowerQuery, offset)
            if (idx === -1) break
            matches.push({ line: lineIdx, start: idx, length: query.length })
            offset = idx + 1
        }
    }
    return matches
}

/** Navigates to the next match index, wrapping around. */
export function nextMatchIndex(current: number, total: number): number {
    if (total === 0) return -1
    return (current + 1) % total
}

/** Navigates to the previous match index, wrapping around. */
export function prevMatchIndex(current: number, total: number): number {
    if (total === 0) return -1
    return (current - 1 + total) % total
}
