package com.getcmdr.idea.features.changelog

import junit.framework.TestCase

/**
 * The recognition rule on its own, with no platform in the way: every case here is a sentence shape, not a PSI shape.
 * `ChangelogRefLinkTest` is the counterpart that proves the rule is actually wired into the editor.
 */
class ChangelogRefsTest : TestCase() {
    fun testFindsASingleTrailingHash() {
        assertRefs("Add right-click Cut / Copy / Paste in every text field (fd6fc293a)", "fd6fc293a")
    }

    fun testFindsEveryHashInAGroup() {
        assertRefs(
            "Add a \"Chat memory size\" setting: Automatic, or 16,000 up to 200,000 tokens (751214190, 14aacf891)",
            "751214190",
            "14aacf891",
        )
    }

    fun testFindsAGroupThatWrappedAcrossTwoSourceLines() {
        // A paragraph keeps the newline and the continuation indent, so the rule has to tolerate both inside a group.
        assertRefs(
            "Add an Acknowledgements dialog crediting all 775 open-source packages Cmdr ships (b626d7a4b, 2d41cc147,\n" +
                "  18add0b0c, 42f76971d)",
            "b626d7a4b",
            "2d41cc147",
            "18add0b0c",
            "42f76971d",
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

    fun testIgnoresARefThatIsNotNineCharacters() {
        // The file is normalized to exactly nine, and the check enforces it. An eight-character ref is a mistake, and
        // an unlinked hash is how it becomes visible.
        assertRefs("Fix the thing (fd6fc293)")
        assertRefs("Fix the thing (fd6fc293ab)")
    }

    fun testFindsHashesOnAnIndentedNestedBullet() {
        // A nested bullet is its own logical entry: Markdown gives it its own paragraph, so by the time the rule runs
        // the indentation is already gone. What matters is that the entry text still ends on its group.
        assertRefs("A nested detail under a parent entry (deadbeef1)", "deadbeef1")
    }

    fun testIgnoresAGroupThatIsNotAtTheEndOfTheEntry() {
        assertRefs("Fix the thing (fd6fc293a) and then some more prose about it")
    }

    fun testOffsetsPointAtTheHashesThemselves() {
        val entry = "Add a setting (751214190, 14aacf891)"

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
