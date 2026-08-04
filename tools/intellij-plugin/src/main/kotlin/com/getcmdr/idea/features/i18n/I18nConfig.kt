package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.core.CmdrProjectService
import com.getcmdr.idea.core.ConfigSection
import com.getcmdr.idea.core.FeatureConfig
import com.intellij.psi.PsiElement

/** One `<Component attribute="…">` shape that carries a key, straight out of `componentAttributes`. */
data class ComponentAttribute(val component: String, val attribute: String)

/**
 * The `i18n` section of `cmdr-plugin.json`: where the English catalog is, and what a key site looks like.
 *
 * Every shape is config, never a Kotlin constant, so renaming `tString` or adding a `tooltipKey` property is a JSON
 * edit. The one thing config can't carry is **which languages fold**: `<lang.foldingBuilder>` registrations are static
 * XML, so they live in `plugin.xml` and `cmdr-svelte.xml` and are pinned by `LanguageCoverageSpikeTest`.
 */
data class I18nConfig(
    /** Project-relative, `<directory>/<name-pattern>`, matched by [CatalogGlob]. */
    val catalogGlob: String,
    /** Functions whose first string-literal argument is a key: `t`, `tString`, `getMessage`. */
    val functions: Set<String>,
    /** Property names whose string-literal value is a key: `labelKey` and friends. */
    val keyProperties: Set<String>,
    val componentAttributes: List<ComponentAttribute>,
) {
    /** Whether an attribute named [attribute] on a `<[component] …>` tag carries a key. */
    fun isKeyAttribute(component: String?, attribute: String): Boolean =
        componentAttributes.any { it.component == component && it.attribute == attribute }

    companion object : FeatureConfig<I18nConfig>("i18n") {
        override fun read(section: ConfigSection): I18nConfig? {
            // No catalog, nothing to resolve a key against, so the feature has nothing to do.
            val glob = section.string("catalogGlob") ?: return null
            return I18nConfig(
                catalogGlob = glob,
                functions = section.stringList("functions").toSet(),
                keyProperties = section.stringList("keyProperties").toSet(),
                componentAttributes = section.objects("componentAttributes").mapNotNull { it.componentAttribute() },
            )
        }

        private fun ConfigSection.componentAttribute(): ComponentAttribute? {
            val component = string("component") ?: return null
            val attribute = string("attribute") ?: return null
            return ComponentAttribute(component, attribute)
        }
    }
}

/**
 * The config that applies where [element] sits, or `null` when the feature has nothing to do: not a Cmdr checkout, or
 * no `i18n` section. Unlike the changelog feature there's no file allowlist, because a key site is worth folding
 * wherever it's written.
 */
internal fun i18nConfigFor(element: PsiElement): I18nConfig? =
    CmdrProjectService.getInstance(element.project).config?.get(I18nConfig)
