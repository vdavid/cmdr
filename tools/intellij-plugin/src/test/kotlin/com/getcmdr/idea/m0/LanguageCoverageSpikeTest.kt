package com.getcmdr.idea.m0

import com.intellij.lang.Language
import com.intellij.lang.folding.LanguageFolding
import com.intellij.testFramework.fixtures.BasePlatformTestCase

/**
 * The registration rule, pinned: **one `<lang.foldingBuilder>` per language we care about, no inheritance shortcuts.**
 *
 * `LanguageExtension.allForLanguage` walks the base-language chain, but it returns the registrations of the *nearest*
 * language that has any, not the union of all of them. So `TypeScript` inheriting from `JavaScript` buys nothing:
 * the platform's own `TypeScriptFoldingBuilder` occupies the TypeScript level and shadows everything registered for
 * `JavaScript`, ours included. Register for every language explicitly, or lose it silently.
 */
class LanguageCoverageSpikeTest : BasePlatformTestCase() {
    fun testBaseLanguageRegistrationsAreShadowedRatherThanMerged() {
        val chain = baseLanguageChain("TypeScript")
        println("[spike] TypeScript base-language chain: ${chain.joinToString(" -> ")}")
        assertTrue("TypeScript is expected to be a JavaScript dialect", chain.contains("JavaScript"))

        // Despite that inheritance, the platform's own JavaScript-registered builder is invisible at the TypeScript
        // level. That's the shadowing, shown with the platform's own extensions so it doesn't depend on ours.
        val forTypeScript = foldingBuilderNames("TypeScript")
        val forJavaScript = foldingBuilderNames("JavaScript")
        println("[spike] folding builders for JavaScript: ${forJavaScript.joinToString()}")
        println("[spike] folding builders for TypeScript: ${forTypeScript.joinToString()}")

        assertTrue(JS_FOLDING_BUILDER, forJavaScript.contains(JS_FOLDING_BUILDER))
        assertFalse(
            "if this fails, base-language registrations now merge and the per-language duplication can go",
            forTypeScript.contains(JS_FOLDING_BUILDER),
        )
    }

    fun testSvelteDialectsDoInheritFromTheirJsFamilyBase() {
        val svelteJs = baseLanguageChain("SvelteJS")
        val svelteTs = baseLanguageChain("SvelteTS")
        println("[spike] SvelteJS base-language chain: ${svelteJs.joinToString(" -> ")}")
        println("[spike] SvelteTS base-language chain: ${svelteTs.joinToString(" -> ")}")

        if (svelteJs.isEmpty()) {
            println("[spike] SKIPPED: the Svelte plugin isn't on the test classpath.")
            return
        }
        assertTrue("SvelteJS should be a JavaScript dialect", svelteJs.contains("JavaScript"))
        assertTrue("SvelteTS should be a TypeScript dialect", svelteTs.contains("TypeScript"))
    }

    fun testEveryLanguageWeRegisterForSeesOurBuilder() {
        REGISTERED_LANGUAGES.forEach { id ->
            if (Language.findLanguageByID(id) == null) {
                println("[spike] $id: not registered in this IDE, skipping")
                return@forEach
            }
            val builders = foldingBuilderNames(id)
            println("[spike] folding builders for $id: ${builders.joinToString()}")
            assertTrue(
                "$id has no Cmdr folding builder; the `plugin.xml` registration is missing or misspelled",
                builders.contains(M0ProbeFoldingBuilder::class.java.name),
            )
        }
    }

    fun testTheProbeCollapsesByDefault() {
        myFixture.configureByText("probe.ts", "const marker = '${M0ProbeFoldingBuilder.PROBE_TOKEN}'\n")
        val descriptors = M0ProbeFoldingBuilder()
            .buildFoldRegions(myFixture.file, myFixture.editor.document, false)

        // Asserted on the descriptor, not on the live region: `updateFoldRegions` deliberately preserves whatever
        // expansion state the editor already has and never applies defaults, so a region's `isExpanded` in tier 1
        // says nothing. Whether a freshly opened file really shows the placeholder is a tier 2 observation.
        assertEquals(1, descriptors.size)
        assertTrue(M0ProbeFoldingBuilder().isCollapsedByDefault(descriptors.single().element))
    }

    private fun baseLanguageChain(id: String): List<String> {
        val language = Language.findLanguageByID(id) ?: return emptyList()
        return generateSequence(language) { it.baseLanguage }.map { it.id }.toList()
    }

    private fun foldingBuilderNames(id: String): List<String> {
        val language = Language.findLanguageByID(id) ?: return emptyList()
        return LanguageFolding.INSTANCE.allForLanguage(language).map { it.javaClass.name }
    }

    private companion object {
        /** Must mirror every `<lang.foldingBuilder>` in `plugin.xml` and `cmdr-svelte.xml`. */
        val REGISTERED_LANGUAGES = listOf("JavaScript", "TypeScript", "SvelteHTML")
        const val JS_FOLDING_BUILDER = "com.intellij.lang.javascript.folding.JavaScriptFoldingBuilder"
    }
}
