package com.getcmdr.idea.features.i18n

import com.intellij.lang.ASTNode
import com.intellij.lang.folding.FoldingBuilderEx
import com.intellij.lang.folding.FoldingDescriptor
import com.intellij.openapi.editor.Document
import com.intellij.openapi.project.DumbAware
import com.intellij.psi.PsiElement

/**
 * Folds a resolvable message key to its English text, so reading a screen's code reads like the screen.
 *
 * Registered once per language in `plugin.xml` (`JavaScript`, `TypeScript`) and once in `cmdr-svelte.xml`
 * (`SvelteHTML`): base-language inheritance does not carry folding registrations, so every language needs its own
 * entry and a line in `LanguageCoverageSpikeTest.FOLDING_LANGUAGES`. See `DETAILS.md`.
 *
 * A key built by template can't fold and isn't meant to; nothing here resolves anything but a literal.
 */
class I18nKeyFoldingBuilder : FoldingBuilderEx(), DumbAware {
    override fun buildFoldRegions(root: PsiElement, document: Document, quick: Boolean): Array<FoldingDescriptor> {
        val config = i18nConfigFor(root) ?: return FoldingDescriptor.EMPTY_ARRAY
        val catalog = MessageCatalogService.getInstance(root.project).catalog() ?: return FoldingDescriptor.EMPTY_ARRAY

        return keySitesIn(root, config).mapNotNull { site ->
            val message = catalog[site.key] ?: return@mapNotNull null
            FoldingDescriptor(site.folded.node, site.folded.textRange, null, foldedMessage(message))
        }.toTypedArray()
    }

    /**
     * Never consulted: every descriptor this builder makes carries its own placeholder, which the platform prefers
     * over this. Folding the key rather than the resolved text would be the wrong fold, so there's no fallback.
     */
    override fun getPlaceholderText(node: ASTNode): String? = null

    /** A freshly opened file shows text rather than keys, which is the entire point of the feature. */
    override fun isCollapsedByDefault(node: ASTNode): Boolean = true
}
