package com.getcmdr.idea.platform

import com.intellij.codeInsight.folding.CodeFoldingManager
import com.intellij.openapi.editor.FoldRegion
import com.intellij.testFramework.fixtures.BasePlatformTestCase

/**
 * Shared plumbing for folding tests.
 *
 * Gotcha, and it costs an hour if you don't know it: in a headless fixture, neither `myFixture.doHighlighting()` nor
 * `CodeFoldingManager.buildInitialFoldings(editor)` puts anything in the folding model. Only
 * [CodeFoldingManager.updateFoldRegions] does, and both wrong ways look like a passing test that asserts nothing.
 * [FoldingHarnessTest] is what keeps this honest.
 */
abstract class FoldingTestCase : BasePlatformTestCase() {
    protected fun foldRegions(): List<FoldRegion> {
        val editor = myFixture.editor
        CodeFoldingManager.getInstance(project).updateFoldRegions(editor)
        return editor.foldingModel.allFoldRegions.toList()
    }
}

/**
 * Pins [FoldingTestCase.foldRegions] against the platform's own folding, so a folding feature's tests can never
 * quietly degrade into asserting nothing. The stock TypeScript builder is the witness: when this goes red the harness
 * is broken, not the feature under test.
 */
class FoldingHarnessTest : FoldingTestCase() {
    fun testTheHarnessSeesThePlatformsOwnFoldRegions() {
        myFixture.configureByText(
            "harness.ts",
            """
            export function shape(): number {
                const total = 1 + 2
                return total
            }
            """.trimIndent(),
        )

        assertNotEmpty(foldRegions())
    }
}
