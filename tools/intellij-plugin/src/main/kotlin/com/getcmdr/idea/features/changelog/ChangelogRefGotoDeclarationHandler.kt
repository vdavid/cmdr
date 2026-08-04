package com.getcmdr.idea.features.changelog

import com.intellij.codeInsight.navigation.actions.GotoDeclarationHandler
import com.intellij.openapi.editor.Editor
import com.intellij.openapi.paths.WebReference
import com.intellij.psi.PsiElement
import com.intellij.psi.util.PsiTreeUtil
import org.intellij.plugins.markdown.lang.psi.impl.MarkdownParagraph

/**
 * Sends ⌘-click on a commit hash to its GitHub commit page.
 *
 * The gesture is ⌘-click, or Cmd+B on the caret, the same as every other reference in the IDE; there's no supported way
 * to get plain-left-click navigation in an editor. Which browser opens, and whether in a new window, is the IDE's own
 * web-browser setting.
 *
 * **Why a goto-declaration handler rather than a `PsiReferenceContributor`.** A contributed `WebReference` is the
 * obvious shape, and on Markdown it silently reaches nobody: the contributor runs and produces the reference, but no
 * Markdown PSI element ever asks the reference registry for it, so `findReferenceAt` stays null. Measured, with the
 * mechanism, in `tools/intellij-plugin/DETAILS.md`. [WebReference] is still the platform's own machinery here, used
 * directly for the navigation target it builds.
 */
class ChangelogRefGotoDeclarationHandler : GotoDeclarationHandler {
    override fun getGotoDeclarationTargets(
        sourceElement: PsiElement?,
        offset: Int,
        editor: Editor,
    ): Array<PsiElement>? {
        val entry = PsiTreeUtil.getParentOfType(sourceElement, MarkdownParagraph::class.java) ?: return null
        val url = commitLinkAt(entry, offset) ?: return null
        val target = WebReference(entry, url).resolve() ?: return null
        return arrayOf(target)
    }
}

/** The commit URL the hash at [offset] points at, or `null` when [entry] carries no link there. */
internal fun commitLinkAt(entry: PsiElement, offset: Int): String? =
    commitLinksIn(entry).firstOrNull { it.range.contains(offset) }?.url
