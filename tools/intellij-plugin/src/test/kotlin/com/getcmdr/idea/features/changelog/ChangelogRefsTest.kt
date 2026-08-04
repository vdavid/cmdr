package com.getcmdr.idea.features.changelog

import junit.framework.TestCase

/**
 * The recognition rule on its own, with no platform in the way: every case here is a sentence shape, not a PSI shape.
 * `ChangelogRefLinkTest` is the counterpart that proves the rule is actually wired into the editor.
 */
class ChangelogRefsTest : TestCase() {
    fun testFindsASingleTrailingHash() {
        assertRefs("Add right-click Cut / Copy / Paste in every text field (fd6fc293)", "fd6fc293")
    }

    fun testFindsEveryHashInAGroup() {
        assertRefs(
            "Add a \"Chat memory size\" setting: Automatic, or 16,000 up to 200,000 tokens (75121419, 14aacf89)",
            "75121419",
            "14aacf89",
        )
    }

    fun testFindsAGroupThatWrappedAcrossTwoSourceLines() {
        // A paragraph keeps the newline and the continuation indent, so the rule has to tolerate both inside a group.
        assertRefs(
            "Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a4, 2d41cc14,\n" +
                "  18add0b0, 42f76971)",
            "b626d7a4",
            "2d41cc14",
            "18add0b0",
            "42f76971",
        )
    }

    fun testIgnoresATrailingAsideThatIsNotHashes() {
        assertRefs("Return a broad search in half a second (~40x speed-up!)")
        assertRefs("Track the upstream fix (smb2 0.8.0)")
    }

    fun testIgnoresAHexLookingWordMidSentence() {
        assertRefs("The decade of beaded facade parsing is over")
        assertRefs("Stop the (deadbeef) case from crashing the parser on load")
    }

    fun testIgnoresARefThatIsNotEightCharacters() {
        // The file is normalized to exactly eight, and the check enforces it. A seven-character ref is a mistake, and
        // an unlinked hash is how it becomes visible.
        assertRefs("Fix the thing (fd6fc29)")
        assertRefs("Fix the thing (fd6fc293a)")
    }

    fun testFindsHashesOnAnIndentedNestedBullet() {
        // A nested bullet is its own logical entry: Markdown gives it its own paragraph, so by the time the rule runs
        // the indentation is already gone. What matters is that the entry text still ends on its group.
        assertRefs("A nested detail under a parent entry (deadbeef)", "deadbeef")
    }

    fun testIgnoresAGroupThatIsNotAtTheEndOfTheEntry() {
        assertRefs("Fix the thing (fd6fc293) and then some more prose about it")
    }

    fun testOffsetsPointAtTheHashesThemselves() {
        val entry = "Add a setting (75121419, 14aacf89)"

        val refs = ChangelogRefs.findTrailingRefs(entry)

        assertEquals(2, refs.size)
        refs.forEach { ref ->
            assertEquals(ref.hash, entry.substring(ref.range.startOffset, ref.range.endOffset))
        }
    }

    fun testHonorsAConfiguredPattern() {
        // The pattern is config, so a repo that abbreviates differently only edits `cmdr-plugin.json`.
        val sixOrMore = Regex("""\(([0-9a-f]{6,40}(?:,\s*[0-9a-f]{6,40})*)\)$""")

        assertEquals(listOf("fd6fc29"), ChangelogRefs.findTrailingRefs("Fix the thing (fd6fc29)", sixOrMore).hashes())
    }

    private fun assertRefs(entry: String, vararg expected: String) {
        assertEquals(expected.toList(), ChangelogRefs.findTrailingRefs(entry).hashes())
    }

    private fun List<CommitRef>.hashes(): List<String> = map { it.hash }
}
