package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.core.CmdrProjectService
import com.intellij.codeInsight.navigation.actions.GotoDeclarationAction
import com.intellij.json.psi.JsonObject
import com.intellij.json.psi.JsonProperty
import com.intellij.psi.PsiElement
import com.intellij.psi.impl.source.resolve.reference.ReferenceProvidersRegistry
import com.intellij.testFramework.fixtures.BasePlatformTestCase

/**
 * ⌘-click on a message key, as the editor really runs it: the caret lands on the key's line in the English catalog,
 * next to the translator description that sits below it.
 *
 * The assertions go through the platform's own goto-declaration lookup rather than through our classes, because
 * "the rule is right" and "the gesture reaches it" are different questions, and the second is the one that silently
 * fails (see `DETAILS.md` on Markdown).
 */
class I18nKeyNavigationTest : BasePlatformTestCase() {
    fun testACallArgumentGoesToItsCatalogLine() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        val offset = keyOffsetIn("sample.ts", "const title = tString('a.title')")

        assertEquals("a.title", declarationAt(offset)?.name)
        assertEquals(CATALOG_FILE, declarationAt(offset)?.containingFile?.name)
    }

    /** Every configured function, since the config is what the feature reads and not a Kotlin constant. */
    fun testEveryConfiguredFunctionNavigates() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        val text = """
            const one = t('a.title')
            const two = tString('a.title')
            const three = getMessage('a.title')
        """.trimIndent()
        myFixture.configureByText("sample.ts", text)

        val functions = listOf("t", "tString", "getMessage")
        assertEquals(
            functions.associateWith { "a.title" },
            // Two past the opening paren is inside the key: `(`, `'`, then the key itself.
            functions.associateWith { declarationAt(text.indexOf("$it($KEY") + it.length + 3)?.name },
        )
    }

    fun testAKeyPropertyGoesToItsCatalogLine() {
        myFixture.cmdrProjectWith("settings.ai.label" to "Provider")
        val offset = keyOffsetIn("definition.ts", "export const provider = { labelKey: 'settings.ai.label' }", KEY_2)

        assertEquals("settings.ai.label", declarationAt(offset)?.name)
    }

    /** The one key site that isn't JavaScript: an ordinary XML attribute in the same `SvelteHTML` root. */
    fun testATransAttributeGoesToItsCatalogLine() {
        if (skipWithoutSveltePlugin()) return
        myFixture.cmdrProjectWith("ui.cancelHint" to "Press {key} to cancel")
        val offset = keyOffsetIn("Sample.svelte", """<Trans key="ui.cancelHint" />""", KEY_3)

        assertEquals("ui.cancelHint", declarationAt(offset)?.name)
    }

    fun testAKeyInASvelteTemplateGoesToItsCatalogLine() {
        if (skipWithoutSveltePlugin()) return
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        val offset = keyOffsetIn("Sample.svelte", "<p>{tString('a.title')}</p>")

        assertEquals("a.title", declarationAt(offset)?.name)
    }

    /**
     * The measurement the mechanism rests on: **these hosts really do ask the reference registry.** Markdown builds a
     * contributed reference and then never asks for it, which is why the changelog feature is a goto-declaration
     * handler instead; the same three values are printed here so an IDE upgrade can be re-measured the same way.
     */
    fun testTheHostPsiReallyAsksTheReferenceRegistry() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?", "ui.cancelHint" to "Press {key} to cancel")

        assertReferenceReaches("sample.ts", "const title = tString('a.title')", KEY)
        if (skipWithoutSveltePlugin()) return
        assertReferenceReaches("Sample.svelte", "<p>{tString('a.title')}</p>", KEY)
        assertReferenceReaches("Attribute.svelte", """<Trans key="ui.cancelHint" />""", KEY_3)
    }

    fun testAKeyTheCatalogDoesNotHaveGoesNowhere() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        val text = "const gone = tString('a.keyThatWasRenamed')"
        myFixture.configureByText("sample.ts", text)

        assertEmpty(catalogTargetsAt(text.indexOf("a.keyThatWasRenamed") + 2))
    }

    fun testAStringThatIsNotAKeySiteGoesNowhere() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        val text = """
            const plain = 'a.title'
            const nested = log('a.title')
            const built = tString(`a.title`)
            const second = tString('a.title', 'a.title')
        """.trimIndent()
        myFixture.configureByText("sample.ts", text)

        assertEmpty("a bare string isn't a key site", catalogTargetsAt(text.indexOf("'a.title'") + 2))
        assertEmpty("an unconfigured function isn't a key site", catalogTargetsAt(text.indexOf("log('") + 6))
        assertEmpty("a template literal is the accepted miss", catalogTargetsAt(text.indexOf("`a.title`") + 2))
        assertEmpty(
            "only the first argument carries the key",
            catalogTargetsAt(text.lastIndexOf("'a.title'") + 2),
        )
    }

    fun testAProjectWithoutTheMarkerGoesNowhere() {
        // No `cmdrProjectWith()`: the plugin has to be inert in every project that isn't this repo.
        val text = "const title = tString('a.title')"
        myFixture.configureByText("sample.ts", text)

        assertNull(CmdrProjectService.getInstance(project).config)
        assertEmpty(catalogTargetsAt(text.indexOf(KEY) + 2))
    }

    /**
     * The payoff, on the repo's own catalog: the caret lands on the key, and the entry right below it is the
     * translator's `@key` description. That adjacency is most of why this beats searching for the key by hand.
     */
    fun testTheRealCatalogLandsOnTheKeyAboveItsTranslatorDescription() {
        myFixture.markProjectAsCmdrCheckout()
        myFixture.copyRealCatalogIntoFixture()
        val offset = keyOffsetIn("sample.ts", "const title = tString('$REAL_KEY')", "'$REAL_KEY'")

        val target = declarationAt(offset)

        assertEquals(REAL_KEY, target?.name)
        assertEquals("crashReporter.json", target?.containingFile?.name)
        val entries = (target?.parent as? JsonObject)?.propertyList.orEmpty()
        assertEquals(
            "the translator description should sit right below the key it describes",
            "@$REAL_KEY",
            entries[entries.indexOf(target) + 1].name,
        )
    }

    /**
     * What the gesture costs on the real catalog: a first click builds the index and parses the JSON file it lands
     * in, and every later one is a map lookup plus a property scan. Numbers are in `DETAILS.md`.
     */
    fun testNavigatingTheRealCatalogIsCheap() {
        myFixture.markProjectAsCmdrCheckout()
        myFixture.copyRealCatalogIntoFixture()
        val offset = keyOffsetIn("sample.ts", "const title = tString('$REAL_KEY')", "'$REAL_KEY'")

        val coldStarted = System.nanoTime()
        assertNotNull(declarationAt(offset))
        val coldMillis = (System.nanoTime() - coldStarted) / 1_000_000

        val warmStarted = System.nanoTime()
        repeat(WARM_RUNS) { declarationAt(offset) }
        val warmMillis = (System.nanoTime() - warmStarted) / 1_000_000 / WARM_RUNS

        println("[perf] resolving a key against the real catalog: $coldMillis ms cold, $warmMillis ms warm")
        assertTrue("a ⌘-click took $warmMillis ms to resolve", warmMillis < BUDGET_MILLIS)
    }

    /**
     * Opens [text] and asserts the key in [literal] is reachable as a reference every way it can be asked for: built
     * by the registry, handed out by the element itself, and found by the file at an offset. The middle one is what
     * Markdown answers zero to.
     */
    private fun assertReferenceReaches(fileName: String, text: String, literal: String) {
        val offset = keyOffsetIn(fileName, text, literal)
        val element = checkNotNull(myFixture.file.findElementAt(offset)?.parent)
        val fromRegistry = ReferenceProvidersRegistry.getReferencesFromProviders(element).size
        val fromElement = element.references.size
        val reference = myFixture.file.findReferenceAt(offset)

        println("[spike] $fileName ${element.javaClass.simpleName}: registry $fromRegistry, element $fromElement")
        assertEquals("the registry didn't build a reference at all", 1, fromRegistry)
        assertEquals("the host PSI doesn't ask the registry, so a reference can't be the mechanism", 1, fromElement)
        assertTrue("the reference resolved to nothing rather than a catalog entry", reference?.resolve() is JsonProperty)
        assertEquals(
            "the ⌘-hover underline should cover the key and not the quotes around it",
            literal.trim('\'', '"'),
            reference?.rangeInElement?.substring(element.text),
        )
    }

    /** Opens [text] as [fileName] and returns an offset inside the key written in [literal]. */
    private fun keyOffsetIn(fileName: String, text: String, literal: String = KEY): Int {
        myFixture.configureByText(fileName, text)
        return text.indexOf(literal) + 2
    }

    /** The catalog entry ⌘-click at [offset] lands on, or `null` when the gesture goes nowhere. */
    private fun declarationAt(offset: Int): JsonProperty? = catalogTargetsAt(offset).singleOrNull() as JsonProperty?

    /**
     * Every catalog entry the platform's own goto-declaration lookup finds at [offset]. Targets that aren't catalog
     * entries are other extensions doing their job (a `.ts` file resolves plenty), so they're filtered out rather
     * than asserted about.
     */
    private fun catalogTargetsAt(offset: Int): List<PsiElement> =
        GotoDeclarationAction.findAllTargetElements(project, myFixture.editor, offset)
            .filterIsInstance<JsonProperty>()

    private companion object {
        const val KEY = "'a.title'"
        const val KEY_2 = "'settings.ai.label'"
        const val KEY_3 = "\"ui.cancelHint\""
        const val REAL_KEY = "crashReporter.dialog.title"
        const val WARM_RUNS = 5

        /** A ⌘-click is one gesture rather than a pass over the file, so the bar is only "not felt as a pause". */
        const val BUDGET_MILLIS = 50
    }
}
