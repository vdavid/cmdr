package com.getcmdr.idea.features.i18n

import junit.framework.TestCase

/** How a catalog message reads once folded. Pure text; `I18nKeyFoldingTest` is the same rules through real PSI. */
class FoldedMessageTest : TestCase() {
    fun testTheMessageIsWrappedInCurlyQuotes() {
        assertEquals("“Send crash report?”", foldedMessage("Send crash report?"))
    }

    fun testIcuDoubledApostrophesCollapseToOne() {
        assertEquals(
            "“Here's a crash report with details that can help.”",
            foldedMessage("Here''s a crash report with details that can help."),
        )
    }

    fun testPlaceholdersAndTagMarkersSurviveVerbatim() {
        assertEquals(
            "“Copying {countText} to <strong>{target}</strong>”",
            foldedMessage("Copying {countText} to <strong>{target}</strong>"),
        )
    }

    fun testNewlinesCollapseToASingleSpace() {
        assertEquals("“First line. Second line.”", foldedMessage("First line.\n  Second line."))
        assertEquals("“One. Two.”", foldedMessage("One.\r\n\r\nTwo."))
    }

    fun testALongMessageIsNeverTruncated() {
        val long = (1..40).joinToString(" ") { "sentence number $it is here." }

        val folded = foldedMessage(long)

        assertTrue("the whole message has to survive, however long", folded.contains("sentence number 40 is here."))
        assertEquals(long.length + 2, folded.length)
        assertFalse("no ellipsis, no cut", folded.contains("…"))
    }

    fun testSurroundingWhitespaceIsDropped() {
        assertEquals("“Trimmed”", foldedMessage("\n  Trimmed  \n"))
    }

    fun testAnEmptyMessageStillReadsAsAFold() {
        assertEquals("“”", foldedMessage(""))
    }
}
