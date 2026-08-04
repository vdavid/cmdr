package com.getcmdr.idea.features.changelog

import com.intellij.openapi.util.TextRange

/** One commit hash, with where it sits inside the entry text it was found in. */
data class CommitRef(val hash: String, val startOffset: Int) {
    val range: TextRange get() = TextRange(startOffset, startOffset + hash.length)
}

/**
 * The recognition rule for the commit hashes `CHANGELOG.md` carries: a parenthesized, comma-separated group of bare
 * hashes closing a logical entry, as in `- Add a thing (75121419, 14aacf89)`.
 *
 * Anchoring to the END of the entry is the whole safety story. Entries routinely close on an aside like
 * `(~40x speed-up!)` or `(smb2 0.8.0)`, and a hex-looking word mid-sentence must never be touched.
 *
 * The same rule is implemented twice more, in `scripts/check/checks/changelog-commit-links.go` and
 * `apps/website/src/lib/changelog.ts`. Nothing guards the three against drift, deliberately: this is private dev
 * tooling and the failure mode is a link that doesn't show up, which one glance at the file reveals.
 */
object ChangelogRefs {
    /**
     * Used when `cmdr-plugin.json` names no pattern of its own. Exactly eight lowercase hex characters, because the
     * file is normalized to that length and the check enforces it.
     *
     * Group 1 must capture the comma-separated hash list; that's the contract a configured pattern has to honor.
     */
    const val DEFAULT_TRAILING_GROUP_PATTERN: String = """\(([0-9a-f]{8}(?:,\s*[0-9a-f]{8})*)\)$"""

    /**
     * The hashes in [entryText]'s trailing group, in document order, with offsets relative to [entryText].
     *
     * [entryText] is one logical entry with its wrapped source lines still joined by their original newlines, which is
     * exactly what a Markdown paragraph inside a list item gives us. Empty when the entry doesn't close on a group.
     */
    fun findTrailingRefs(entryText: String, pattern: Regex = Regex(DEFAULT_TRAILING_GROUP_PATTERN)): List<CommitRef> {
        // Trimming the end, not the start: a trailing newline would let `$` match one character short of the group's
        // closing paren, and dropping it costs no offsets because everything before it keeps its position.
        val group = pattern.find(entryText.trimEnd())?.groups?.get(1) ?: return emptyList()

        val refs = mutableListOf<CommitRef>()
        var cursor = group.range.first
        group.value.split(',').forEach { piece ->
            val hash = piece.trim()
            if (hash.isNotEmpty()) {
                refs += CommitRef(hash, cursor + piece.indexOf(hash))
            }
            cursor += piece.length + 1 // the comma the split consumed
        }
        return refs
    }
}
