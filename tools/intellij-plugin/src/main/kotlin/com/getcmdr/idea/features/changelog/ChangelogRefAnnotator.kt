package com.getcmdr.idea.features.changelog

import com.intellij.lang.annotation.AnnotationHolder
import com.intellij.lang.annotation.Annotator
import com.intellij.lang.annotation.HighlightSeverity
import com.intellij.openapi.editor.colors.CodeInsightColors
import com.intellij.psi.PsiElement

/**
 * Colors the commit hashes a changelog entry closes on, so they read as links without hovering.
 *
 * `ChangelogRefGotoDeclarationHandler` is what makes them clickable; this is only paint. Both read the same
 * [commitLinksIn] list, so the two can't disagree about which hashes are links.
 */
class ChangelogRefAnnotator : Annotator {
    override fun annotate(element: PsiElement, holder: AnnotationHolder) {
        commitLinksIn(element).forEach { link ->
            holder.newSilentAnnotation(HighlightSeverity.INFORMATION)
                .range(link.range)
                .textAttributes(HYPERLINK)
                .create()
        }
    }

    companion object {
        /** The IDE's own link color, so a commit ref looks like every other link in the editor and follows the theme. */
        val HYPERLINK = CodeInsightColors.HYPERLINK_ATTRIBUTES
    }
}
