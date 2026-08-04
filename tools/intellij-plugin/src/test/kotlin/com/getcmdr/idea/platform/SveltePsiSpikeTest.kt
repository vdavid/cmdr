package com.getcmdr.idea.platform

import com.getcmdr.idea.RepoFiles
import com.intellij.lang.Language
import com.intellij.lang.javascript.psi.JSCallExpression
import com.intellij.lang.javascript.psi.JSLiteralExpression
import com.intellij.lang.javascript.psi.JSReferenceExpression
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.util.PsiTreeUtil
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.psi.xml.XmlAttribute

/**
 * The M0 spike, and the answer it found: `{tString(…)}` inside a `.svelte` template is ordinary JavaScript PSI, so
 * **the text-matching fallback the spec kept in reserve for Svelte is not needed**. The catch is that a `.svelte` file
 * has a single `SvelteHTML` view-provider root with the JS inlined behind a lazy-parse node, so an extension has to be
 * registered for `SvelteHTML` and walk down; registering for `SvelteJS` or `SvelteTS` would never fire.
 *
 * Every test prints its observation, so `./gradlew test --tests '*SveltePsiSpike*'` is a re-runnable evidence dump.
 * Findings are written up in `DETAILS.md`; this file is what regenerates them after an IDE upgrade.
 */
class SveltePsiSpikeTest : BasePlatformTestCase() {
    private val sveltePluginPresent: Boolean
        get() = Language.findLanguageByID(SVELTE_FILE_LANGUAGE_ID) != null

    /** Question 2: the language IDs the Svelte plugin registers, and the base-language chain of each. */
    fun testSvelteLanguageIdsAndTheirBaseLanguageChain() {
        if (skipWithoutSveltePlugin()) return

        val report = SVELTE_LANGUAGE_IDS.joinToString("\n") { id ->
            val language = Language.findLanguageByID(id)
            "  $id -> ${language?.let { describeLanguageChain(it) } ?: "NOT REGISTERED"}"
        }
        println("[spike] Svelte language IDs:\n$report")

        // `SvelteHTML` is the ID every Svelte registration in `cmdr-svelte.xml` names.
        assertNotNull(
            "the Svelte file language must be resolvable by ID, or no registration for it can bind",
            Language.findLanguageByID(SVELTE_FILE_LANGUAGE_ID),
        )
    }

    /** Question 1, on a synthetic file that reproduces the real construct. */
    fun testTemplateExpressionSurfacesAsJavaScriptPsi() {
        if (skipWithoutSveltePlugin()) return

        val source = """
            <script lang="ts">
                import { tString } from '${'$'}lib/intl/messages.svelte'
                const marker = tString('spike.inScript')
            </script>

            <p>{tString('spike.inTemplate')}</p>
        """.trimIndent()
        myFixture.configureByText("Spike.svelte", source)

        describeFile(myFixture.file, "Spike.svelte")

        describeElementAt("the template expression", source.indexOf("'spike.inTemplate'") + 1)
        describeElementAt("the <script> block", source.indexOf("'spike.inScript'") + 1)

        // The structural fact everything else follows from: one root, language SvelteHTML. There is no second
        // JavaScript root and no language injection, so the folding pass only ever hands us the HTML root.
        assertEquals(listOf(SVELTE_FILE_LANGUAGE_ID), myFixture.file.viewProvider.languages.map { it.id })

        // Two `tString(…)` calls in the file: one in the <script> block, one inside a template expression. Reaching
        // both by walking down from that single SvelteHTML root is the answer to question 1, and it's what lets an
        // extension registered for `SvelteHTML` match real call expressions rather than text.
        val keys = PsiTreeUtil.findChildrenOfType(myFixture.file, JSCallExpression::class.java)
            .mapNotNull { (it.arguments.firstOrNull() as? JSLiteralExpression)?.stringValue }
        println("[spike] keys reachable from the SvelteHTML root: ${keys.joinToString()}")

        assertEquals(listOf("spike.inScript", "spike.inTemplate"), keys)
    }

    /** Question 1 again, on the real repo file the spec names, so the answer isn't an artifact of a tidy fixture. */
    fun testRealCrashReportDialogExposesTheSameShape() {
        if (skipWithoutSveltePlugin()) return

        val real = RepoFiles.find(REAL_SVELTE_FILE)
        if (real == null) {
            println("[spike] SKIPPED: $REAL_SVELTE_FILE not found under cmdr.repo.root; the file moved.")
            return
        }

        val source = real.readText()
        myFixture.configureByText("CrashReportDialog.svelte", source)
        describeFile(myFixture.file, REAL_SVELTE_FILE)

        val callOffset = source.indexOf("{tString('crashReporter.dialog.privacyNote')}")
        assertTrue("the construct the spec cites is gone from the real file", callOffset >= 0)

        val keyOffset = callOffset + "{tString('".length
        val onKey = elementAt(keyOffset)
        println(
            "[spike] real file, element on the key literal: language=${onKey?.language?.id} " +
                "elementType=${onKey?.node?.elementType} class=${onKey?.javaClass?.name}",
        )
        println("[spike] real file, ancestors: ${ancestry(onKey)}")

        // The literal has to be reachable as a JS literal, or the key folding's PSI approach can't work at all.
        assertNotNull("no PSI at the key literal offset", onKey)
        assertTrue(
            "the key literal should live in a JavaScript-family language, got ${onKey?.language?.id}",
            onKey!!.language.isKindOf(JAVASCRIPT_LANGUAGE_ID),
        )

        // What the key folding rests on: the key is the first argument of a real `JSCallExpression`, so matching is a PSI
        // shape check, not a regex. `getParentOfType` has to cross the Svelte lazy-parse boundary to find it.
        val literal = PsiTreeUtil.getParentOfType(onKey, JSLiteralExpression::class.java)
        val call = PsiTreeUtil.getParentOfType(onKey, JSCallExpression::class.java)
        assertNotNull("the key should sit inside a JSLiteralExpression", literal)
        assertNotNull("the tString(…) call should surface as a JSCallExpression", call)
        assertEquals("crashReporter.dialog.privacyNote", literal!!.stringValue)
        assertEquals("tString", (call!!.methodExpression as? JSReferenceExpression)?.referenceName)
    }

    /**
     * The third key-site shape, and the one that is NOT JavaScript: `<Trans key="…">`. Recorded here because
     * "it's a Svelte file, so it's JS PSI" is exactly the wrong generalization to carry into a folding builder.
     */
    fun testTransAttributeIsXmlPsiRatherThanJavaScript() {
        if (skipWithoutSveltePlugin()) return

        val real = RepoFiles.find(REAL_TRANS_FILE)
        if (real == null) {
            println("[spike] SKIPPED: $REAL_TRANS_FILE not found under cmdr.repo.root; the file moved.")
            return
        }

        val source = real.readText()
        myFixture.configureByText("LoadingIcon.svelte", source)

        val marker = "<Trans key=\""
        val markerOffset = source.indexOf(marker)
        assertTrue("the <Trans key=\"…\"> the spec cites is gone from the real file", markerOffset >= 0)

        val onKey = elementAt(markerOffset + marker.length)
        println(
            "[spike] <Trans key> element: language=${onKey?.language?.id} " +
                "elementType=${onKey?.node?.elementType} class=${onKey?.javaClass?.name}",
        )
        println("[spike] <Trans key> ancestors: ${ancestry(onKey)}")

        val attribute = PsiTreeUtil.getParentOfType(onKey, XmlAttribute::class.java)
        assertNotNull("the Trans key should be reachable as an XmlAttribute", attribute)
        assertEquals("key", attribute!!.name)
        assertEquals("ui.loadingIcon.cancelHint", attribute.value)
        assertEquals("Trans", attribute.parent.name)
        assertFalse(
            "the attribute value is XML, not JavaScript; matching it takes its own branch of the walk",
            onKey!!.language.isKindOf(JAVASCRIPT_LANGUAGE_ID),
        )
    }

    private fun skipWithoutSveltePlugin(): Boolean {
        if (sveltePluginPresent) return false
        println("[spike] SKIPPED: the Svelte plugin isn't on the test classpath (see `cmdrSveltePluginPath`).")
        return true
    }

    private fun elementAt(offset: Int): PsiElement? =
        myFixture.file.viewProvider.findElementAt(offset) ?: myFixture.file.findElementAt(offset)

    private fun describeElementAt(label: String, offset: Int) {
        val element = elementAt(offset)
        println(
            "[spike] element inside $label: language=${element?.language?.id} " +
                "elementType=${element?.node?.elementType} class=${element?.javaClass?.name}",
        )
    }

    private fun describeFile(file: PsiFile, label: String) {
        val provider = file.viewProvider
        println("[spike] $label view provider: ${provider.javaClass.name}")
        println("[spike] $label languages: ${provider.languages.joinToString { it.id }}")
        provider.allFiles.forEach { root ->
            println("[spike] $label root: language=${root.language.id} class=${root.javaClass.name}")
        }
    }

    private fun ancestry(element: PsiElement?): String {
        val parts = mutableListOf<String>()
        var current = element?.parent
        var depth = 0
        while (current != null && depth < 8) {
            parts += "${current.javaClass.simpleName}[${current.language.id}]"
            current = current.parent
            depth++
        }
        return parts.joinToString(" < ")
    }

    private fun describeLanguageChain(language: Language): String {
        val chain = generateSequence(language) { it.baseLanguage }.map { it.id }.toList()
        return chain.joinToString(" -> ")
    }

    private fun Language.isKindOf(id: String): Boolean =
        generateSequence(this) { it.baseLanguage }.any { it.id == id }

    private companion object {
        const val SVELTE_FILE_LANGUAGE_ID = "SvelteHTML"
        const val JAVASCRIPT_LANGUAGE_ID = "JavaScript"
        val SVELTE_LANGUAGE_IDS = listOf("SvelteHTML", "SvelteJS", "SvelteTS")
        const val REAL_SVELTE_FILE = "apps/desktop/src/lib/crash-reporter/CrashReportDialog.svelte"
        const val REAL_TRANS_FILE = "apps/desktop/src/lib/ui/LoadingIcon.svelte"
    }
}
