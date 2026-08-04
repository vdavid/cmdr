package com.getcmdr.idea.features.i18n

import com.intellij.lang.javascript.psi.JSCallExpression
import com.intellij.lang.javascript.psi.JSLiteralExpression
import com.intellij.lang.javascript.psi.JSProperty
import com.intellij.lang.javascript.psi.JSReferenceExpression
import com.intellij.lang.javascript.psi.ecma6.JSStringTemplateExpression
import com.intellij.psi.PsiElement
import com.intellij.psi.util.PsiTreeUtil
import com.intellij.psi.xml.XmlAttribute

/**
 * One place in the source where a message key is written.
 *
 * Two elements rather than one, because they answer different questions: [folded] is what the editor replaces with the
 * message, [keyElement] is where the key itself is written.
 */
internal data class KeySite(
    /** The whole call, or the quoted value of a key property or component attribute. */
    val folded: PsiElement,
    /** The literal holding the key text: the call's first argument, or the same element as [folded]. */
    val keyElement: PsiElement,
    val key: String,
)

/**
 * Every key site under [root], in document order.
 *
 * One tree walk covering all three shapes [config] describes. `.svelte` needs no second code path for the first two:
 * its JavaScript is inline in the same `SvelteHTML` root behind a lazy-parse node, and walking down reaches it (see
 * `DETAILS.md`). `<Trans key="…">` is the exception, an ordinary `XmlAttribute` in that same root.
 */
internal fun keySitesIn(root: PsiElement, config: I18nConfig): List<KeySite> {
    val sites = mutableListOf<KeySite>()
    PsiTreeUtil.processElements(root) { element ->
        when (element) {
            is JSCallExpression -> config.callSite(element)
            is JSProperty -> config.propertySite(element)
            is XmlAttribute -> config.attributeSite(element)
            else -> null
        }?.let(sites::add)
        true
    }
    return sites
}

/** `tString('a.key')`: the whole call folds, since the function name says nothing the resolved sentence doesn't. */
private fun I18nConfig.callSite(call: JSCallExpression): KeySite? {
    val name = (call.methodExpression as? JSReferenceExpression)?.referenceName ?: return null
    if (name !in functions) return null
    val literal = call.arguments.firstOrNull() as? JSLiteralExpression ?: return null
    val key = literal.keyText() ?: return null
    return KeySite(folded = call, keyElement = literal, key = key)
}

/**
 * `labelKey: 'a.key'`: only the value folds. Unlike a call, the property name says which slot of a settings definition
 * the copy fills, and an object of four folded sentences with no names in front is unreadable.
 */
private fun I18nConfig.propertySite(property: JSProperty): KeySite? {
    if (property.name !in keyProperties) return null
    val literal = property.value as? JSLiteralExpression ?: return null
    val key = literal.keyText() ?: return null
    return KeySite(folded = literal, keyElement = literal, key = key)
}

/** `<Trans key="a.key">`: the quoted value folds, for the same reason a key property's does. */
private fun I18nConfig.attributeSite(attribute: XmlAttribute): KeySite? {
    if (!isKeyAttribute(attribute.parent?.name, attribute.name)) return null
    val value = attribute.valueElement ?: return null
    val key = attribute.value?.takeIf { it.isNotEmpty() } ?: return null
    return KeySite(folded = value, keyElement = value, key = key)
}

/**
 * The key a literal spells out, or `null` when it isn't one.
 *
 * A template literal is never a key site, even when it happens to have no substitutions: a key built by template is
 * the accepted miss, and resolving one would need a resolver this feature deliberately doesn't have.
 */
private fun JSLiteralExpression.keyText(): String? {
    // A template literal with no substitutions has a `stringValue` like any other, so excluding it takes an explicit
    // type check; `isQuotedLiteral` counts backticks as quotes.
    if (this is JSStringTemplateExpression || !isQuotedLiteral) return null
    return stringValue?.takeIf { it.isNotEmpty() }
}
