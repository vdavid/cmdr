package com.getcmdr.idea.features.changelog

import com.intellij.openapi.util.TextRange
import com.intellij.psi.PsiElement
import org.intellij.plugins.markdown.lang.psi.impl.MarkdownListItem
import org.intellij.plugins.markdown.lang.psi.impl.MarkdownParagraph

/** One commit hash in the document: where it sits, and where it goes. */
internal data class CommitLink(val url: String, val range: TextRange)

/**
 * The commit links a changelog entry closes on, with document-absolute ranges, or empty when [element] carries none.
 *
 * Both extension points call exactly this, which is what makes "a hash that colors also clicks" structural rather than
 * a promise: the annotator paints these ranges and the goto handler navigates whichever one holds the caret.
 *
 * The first line is also the cheap rejection every element that isn't a bullet's paragraph falls out on, before
 * anything touches the config.
 *
 * Markdown does the hard part. A bullet's [MarkdownParagraph] already spans the entry's wrapped source lines, newlines
 * and continuation indent included, and a nested bullet is its own list item rather than part of its parent's
 * paragraph. So there's no line-rejoining here, unlike `scripts/check/checks/changelog-commit-links.go` and
 * `apps/website/src/lib/changelog.ts`, which read raw lines.
 */
internal fun commitLinksIn(element: PsiElement): List<CommitLink> {
    val entry = (element as? MarkdownParagraph)?.takeIf { it.parent is MarkdownListItem } ?: return emptyList()
    val config = changelogConfigFor(entry) ?: return emptyList()

    val entryStart = entry.textRange.startOffset
    return ChangelogRefs.findTrailingRefs(entry.text, config.trailingGroupPattern)
        .map { CommitLink(config.commitUrl(it.hash), it.range.shiftRight(entryStart)) }
}
