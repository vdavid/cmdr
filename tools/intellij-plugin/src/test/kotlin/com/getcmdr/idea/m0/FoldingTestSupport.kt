package com.getcmdr.idea.m0

import com.intellij.codeInsight.folding.CodeFoldingManager
import com.intellij.openapi.editor.FoldRegion
import com.intellij.testFramework.fixtures.BasePlatformTestCase

/**
 * Shared plumbing for tier 1 folding tests.
 *
 * Gotcha, and it costs an hour if you don't know it: in a headless fixture, neither `myFixture.doHighlighting()` nor
 * `CodeFoldingManager.buildInitialFoldings(editor)` puts anything in the folding model. Only
 * [CodeFoldingManager.updateFoldRegions] does. Verified on IDEA 2026.2 (build 262.8665.176) by asserting that the
 * stock TypeScript builder's own regions show up, 2026-08-03.
 */
abstract class FoldingTestCase : BasePlatformTestCase() {
    protected fun foldRegions(): List<FoldRegion> {
        val editor = myFixture.editor
        CodeFoldingManager.getInstance(project).updateFoldRegions(editor)
        return editor.foldingModel.allFoldRegions.toList()
    }

    /** The placeholders of every region our probe contributed, in document order. */
    protected fun probePlaceholders(): List<String> =
        foldRegions()
            .filter { it.placeholderText == M0ProbeFoldingBuilder.PLACEHOLDER }
            .sortedBy { it.startOffset }
            .map { it.placeholderText }
}
