package com.getcmdr.idea.m0

import com.intellij.lang.ASTNode
import com.intellij.lang.folding.FoldingBuilderEx
import com.intellij.lang.folding.FoldingDescriptor
import com.intellij.lang.javascript.psi.JSLiteralExpression
import com.intellij.openapi.editor.Document
import com.intellij.psi.PsiElement
import com.intellij.psi.util.PsiTreeUtil

/**
 * The M0 probe: folds the string literal `'CMDR_M0_PROBE'` to `«m0»`, and nothing else.
 *
 * It exists to prove two things before any real feature is written:
 *  1. the tier 1 loop (a headless [com.intellij.testFramework.fixtures.BasePlatformTestCase] asserting a fold region)
 *     actually runs the platform's folding lifecycle, and
 *  2. an extension registered for `language="JavaScript"` reaches `.ts` and `.svelte` template expressions by
 *     language inheritance, which is the question M4 depends on.
 *
 * Delete it when M3 lands a real feature. Nothing else may depend on it.
 */
class M0ProbeFoldingBuilder : FoldingBuilderEx() {
    override fun buildFoldRegions(root: PsiElement, document: Document, quick: Boolean): Array<FoldingDescriptor> =
        PsiTreeUtil.findChildrenOfType(root, JSLiteralExpression::class.java)
            .filter { it.isStringLiteral && it.stringValue == PROBE_TOKEN }
            .map { FoldingDescriptor(it.node, it.textRange, null, PLACEHOLDER) }
            .toTypedArray()

    override fun getPlaceholderText(node: ASTNode): String = PLACEHOLDER

    override fun isCollapsedByDefault(node: ASTNode): Boolean = true

    companion object {
        const val PROBE_TOKEN: String = "CMDR_M0_PROBE"
        const val PLACEHOLDER: String = "«m0»"
    }
}
