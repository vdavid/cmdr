package com.getcmdr.idea.m0

/**
 * Tier 1 in its simplest form: a headless platform fixture that asserts a real fold region on real JS/TS PSI.
 * If this goes red, the loop every later milestone lives in is broken, not the feature under test.
 */
class M0ProbeFoldingTest : FoldingTestCase() {
    fun testFoldsTheProbeTokenInJavaScript() {
        myFixture.configureByText("probe.js", "const marker = '${M0ProbeFoldingBuilder.PROBE_TOKEN}'\n")

        assertEquals(listOf(M0ProbeFoldingBuilder.PLACEHOLDER), probePlaceholders())
    }

    fun testFoldsTheProbeTokenInTypeScript() {
        myFixture.configureByText("probe.ts", "const marker: string = '${M0ProbeFoldingBuilder.PROBE_TOKEN}'\n")

        assertEquals(listOf(M0ProbeFoldingBuilder.PLACEHOLDER), probePlaceholders())
    }

    fun testFoldsEveryOccurrenceAndTheRegionCoversTheWholeLiteral() {
        val token = M0ProbeFoldingBuilder.PROBE_TOKEN
        myFixture.configureByText("probe.ts", "const a = '$token'\nconst b = '$token'\n")

        val regions = foldRegions().filter { it.placeholderText == M0ProbeFoldingBuilder.PLACEHOLDER }
        assertEquals(2, regions.size)
        val text = myFixture.editor.document.charsSequence
        regions.forEach { assertEquals("'$token'", text.subSequence(it.startOffset, it.endOffset).toString()) }
    }

    fun testDoesNotFoldAnUnrelatedLiteral() {
        myFixture.configureByText("probe.ts", "const marker = 'something else'\n")

        assertEquals(emptyList<String>(), probePlaceholders())
    }

}
