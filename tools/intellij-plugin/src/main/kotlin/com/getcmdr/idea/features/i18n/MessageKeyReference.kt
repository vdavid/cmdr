package com.getcmdr.idea.features.i18n

import com.intellij.lang.javascript.psi.JSLiteralExpression
import com.intellij.openapi.util.TextRange
import com.intellij.patterns.PlatformPatterns
import com.intellij.psi.ElementManipulators
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiReference
import com.intellij.psi.PsiReferenceBase
import com.intellij.psi.PsiReferenceContributor
import com.intellij.psi.PsiReferenceProvider
import com.intellij.psi.PsiReferenceRegistrar
import com.intellij.psi.xml.XmlAttributeValue
import com.intellij.util.ProcessingContext

/**
 * Makes a message key a reference to its catalog entry, so ⌘-click on the key opens
 * `messages/en/<area>.json` at the line it's defined on, right above the translator's `@key` description.
 *
 * **Why a reference and not a `GotoDeclarationHandler`.** A contributed reference is the platform's own shape for
 * "this text points at that declaration", and it carries the underline, ⌘B, quick definition, and Find Usages with
 * it. The changelog feature has a handler instead only because Markdown never asks the reference registry (see
 * `DETAILS.md`); JavaScript literals and XML attribute values both do, measured by `I18nKeyNavigationTest`.
 *
 * Registered for every language at once, unlike the folding builder: reference contributors registered for
 * `Language.ANY` are merged into every language's registrar, so `.ts`, `.js`, and `.svelte` are covered by one
 * registration and the pattern is what narrows it.
 */
class I18nKeyReferenceContributor : PsiReferenceContributor() {
    override fun registerReferenceProviders(registrar: PsiReferenceRegistrar) {
        // The two elements a key is ever written in: a JavaScript literal (a call argument or a key property's
        // value) and an XML attribute value (`<Trans key="…">`).
        registrar.registerReferenceProvider(
            PlatformPatterns.psiElement(JSLiteralExpression::class.java),
            MessageKeyReferenceProvider,
        )
        registrar.registerReferenceProvider(
            PlatformPatterns.psiElement(XmlAttributeValue::class.java),
            MessageKeyReferenceProvider,
        )
    }
}

private object MessageKeyReferenceProvider : PsiReferenceProvider() {
    /**
     * A reference only where the key really resolves. An unresolvable key gets none at all, so a renamed key reads as
     * ordinary text: no underline promising a jump that can't happen, and nothing to fail when it's clicked.
     */
    override fun getReferencesByElement(element: PsiElement, context: ProcessingContext): Array<PsiReference> {
        val config = i18nConfigFor(element) ?: return PsiReference.EMPTY_ARRAY
        val site = keySiteFor(element, config) ?: return PsiReference.EMPTY_ARRAY
        val catalog = MessageCatalogService.getInstance(element.project).catalog() ?: return PsiReference.EMPTY_ARRAY
        if (catalog[site.key] == null) return PsiReference.EMPTY_ARRAY

        return arrayOf(MessageKeyReference(element, ElementManipulators.getValueTextRange(element), site.key))
    }
}

/**
 * The key text as a pointer to its catalog entry.
 *
 * Soft, because a key that stops resolving is a rename to notice in the app's own type checking, not something for the
 * IDE to paint red in the middle of a file.
 */
private class MessageKeyReference(element: PsiElement, range: TextRange, private val key: String) :
    PsiReferenceBase<PsiElement>(element, range, true) {
    override fun resolve(): PsiElement? = messageDeclaration(element.project, key)
}
