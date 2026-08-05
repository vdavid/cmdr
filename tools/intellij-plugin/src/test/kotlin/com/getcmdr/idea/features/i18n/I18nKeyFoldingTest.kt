package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.core.CmdrProjectService
import com.getcmdr.idea.platform.FoldingTestCase
import com.intellij.openapi.util.TextRange
import com.intellij.psi.PsiFile
import com.intellij.psi.util.PsiTreeUtil
import java.io.File

/**
 * The feature as the editor really sees it: a resolvable key folds to its English text, everything else is left alone.
 *
 * `FoldedMessageTest` covers how a message reads once folded and `MessageCatalogTest` covers the index; this covers
 * that real PSI hands both of them what they expect, in every language we register for.
 */
class I18nKeyFoldingTest : FoldingTestCase() {
    fun testACallToEveryConfiguredFunctionFolds() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText(
            "sample.ts",
            """
            const one = t('a.title')
            const two = tString('a.title')
            const three = getMessage('a.title')
            """.trimIndent(),
        )

        assertEquals(
            listOf(
                "t('a.title')" to TITLE,
                "tString('a.title')" to TITLE,
                "getMessage('a.title')" to TITLE,
            ),
            folds(),
        )
    }

    /** The whole call folds, not just the argument: `tString(` around a sentence is noise once the sentence is there. */
    fun testACallFoldsWholeRatherThanJustItsArgument() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")

        assertEquals(listOf("tString('a.title')" to TITLE), folds())
    }

    /**
     * A key property folds to its value only, unlike a call. `labelKey:` names which slot of a settings definition the
     * copy fills, so hiding it would lose more than it saves.
     */
    fun testAKeyPropertyFoldsItsValueAndKeepsItsName() {
        myFixture.cmdrProjectWith("settings.ai.provider.label" to "Provider")
        myFixture.configureByText(
            "definition.ts",
            """
            export const provider = {
                labelKey: 'settings.ai.provider.label',
                descriptionKey: 'settings.ai.provider.label',
                titleKey: 'settings.ai.provider.label',
                cardKey: 'settings.ai.provider.label',
                otherKey: 'settings.ai.provider.label',
            }
            """.trimIndent(),
        )

        assertEquals(
            "only the four configured properties carry keys",
            List(4) { "'settings.ai.provider.label'" to "“Provider”" },
            folds(),
        )
    }

    fun testAKeyOnlyFoldsWhereTheCodeReallyUsesItAsOne() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText(
            "sample.ts",
            """
            // tString('a.title') in a comment is prose
            const plain = 'a.title'
            const nested = log('a.title')
            const built = tString(`a.title`)
            """.trimIndent(),
        )

        assertEmpty(folds())
    }

    fun testAKeyTheCatalogDoesNotHaveIsLeftAlone() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const gone = tString('a.keyThatWasRenamed')")

        assertEmpty(folds())
    }

    fun testIcuDoubledApostrophesAreUnescapedInTheEditor() {
        myFixture.cmdrProjectWith("a.body" to "Cmdr quit unexpectedly. Here''s a crash report.")
        myFixture.configureByText("sample.ts", "const body = tString('a.body')")

        assertEquals(listOf("“Cmdr quit unexpectedly. Here's a crash report.”"), placeholders())
    }

    fun testAMultiSentenceMessageFoldsInFull() {
        val long = "It includes the app version, macOS version, and which part of the code crashed. " +
            "No file names or personal data. You can read the whole report before it's sent, and you can say no."
        myFixture.cmdrProjectWith("a.privacyNote" to long)
        myFixture.configureByText("sample.ts", "const note = tString('a.privacyNote')")

        assertEquals(listOf("“$long”"), placeholders())
    }

    fun testAPlaceholderSurvivesVerbatim() {
        myFixture.cmdrProjectWith("a.copying" to "Copying {countText} to {target}")
        myFixture.configureByText("sample.ts", "const label = tString('a.copying')")

        assertEquals(listOf("“Copying {countText} to {target}”"), placeholders())
    }

    /**
     * The catalog changes under an open IDE, and both halves have to notice: the index, and the editor already
     * showing a fold built from it. A fold showing yesterday's copy is worse than no fold at all.
     *
     * The second assertion is the one that pins a platform quirk: dropping the index isn't enough, because an
     * existing fold region keeps its placeholder text. See `DETAILS.md` for what does and doesn't refresh it.
     */
    fun testTheCatalogReloadsAfterTheJsonChangesOnDisk() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")
        assertEquals(listOf(TITLE), placeholders())

        myFixture.rewriteCatalog("a.title" to "Send this crash report?")

        assertEquals(
            "the index kept the old copy, so the VFS invalidation didn't fire",
            "Send this crash report?",
            MessageCatalogService.getInstance(project).catalog()?.get("a.title")?.text,
        )
        assertEquals("the open editor kept the copy it was opened with", listOf(RETITLED), placeholders())
    }

    /**
     * `isCollapsedByDefault` is asserted on the descriptor rather than on a materialized region: the platform call
     * that applies default collapse state, `updateFoldRegionsAsync`, throws on the EDT in a headless fixture. Tier 2
     * is what confirms a freshly opened file really shows the text.
     */
    fun testAFoldIsCollapsedByDefault() {
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")

        val builder = I18nKeyFoldingBuilder()
        val descriptors = builder.buildFoldRegions(myFixture.file, myFixture.editor.document, false)

        assertEquals(1, descriptors.size)
        assertEquals(TITLE, descriptors.single().placeholderText)
        assertTrue("a fold that opens expanded shows keys, which is the thing we're fixing", descriptors.all {
            builder.isCollapsedByDefault(it.element)
        })
    }

    fun testAProjectWithoutTheMarkerFoldsNothing() {
        // No `cmdrProjectWith()`: the plugin has to be inert in every project that isn't this repo.
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")

        assertNull(CmdrProjectService.getInstance(project).config)
        assertEmpty(folds())
    }

    fun testASvelteTemplateAndScriptBothFold() {
        if (skipWithoutSveltePlugin()) return
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText(
            "Sample.svelte",
            """
            <script lang="ts">
                const inScript = tString('a.title')
            </script>

            <p>{tString('a.title')}</p>
            """.trimIndent(),
        )

        assertEquals(listOf("tString('a.title')" to TITLE, "tString('a.title')" to TITLE), folds())
    }

    /** The one key site that isn't JavaScript: an ordinary XML attribute in the same `SvelteHTML` root. */
    fun testATransKeyAttributeFolds() {
        if (skipWithoutSveltePlugin()) return
        myFixture.cmdrProjectWith("ui.loadingIcon.cancelHint" to "Press {key} to cancel")
        myFixture.configureByText(
            "Sample.svelte",
            """
            <Trans key="ui.loadingIcon.cancelHint" snippets={{ key: escKeyChip }} />
            <div key="ui.loadingIcon.cancelHint"></div>
            """.trimIndent(),
        )

        assertEquals(
            "only the configured component's attribute is a key site",
            listOf("\"ui.loadingIcon.cancelHint\"" to "“Press {key} to cancel”"),
            folds(),
        )
    }

    /**
     * The regression that shipped: in a real IDE, every JavaScript key site in a `.svelte` file silently stopped
     * folding while `<Trans key="…">` went on working.
     *
     * `SvelteTS` carries no folding registration of its own, so `LanguageFolding.allForLanguage` climbs its base
     * chain to `TypeScript` and finds ours. The folding pass then hands this builder the embedded `<script>` and each
     * `{…}` template expression as roots in their own right, on top of the whole-file `SvelteHTML` root it has
     * already walked. Every JS key site came back twice, and the platform answers two fold regions over one range by
     * keeping neither. Only the `<Trans>` attribute, which no JavaScript root can reach, survived.
     *
     * Pre-fix this would have passed wrongly through `folds()`: tier 1 never runs those extra passes, so no
     * end-to-end fold assertion can see the duplication. Ask the builder for the embedded roots directly instead.
     */
    fun testAnEmbeddedSvelteRootFoldsNothingTheWholeFileAlreadyDid() {
        if (skipWithoutSveltePlugin()) return
        myFixture.cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText(
            "Sample.svelte",
            """
            <script lang="ts">
                const inScript = tString('a.title')
            </script>

            <p>{tString('a.title')}</p>
            """.trimIndent(),
        )

        val file = myFixture.file
        val config = requireNotNull(i18nConfigFor(file)) { "the fixture project should be a Cmdr checkout" }
        val builder = I18nKeyFoldingBuilder()
        val document = myFixture.editor.document

        // Every root the folding pass can offer besides the file itself: a child whose language differs from its
        // parent's is exactly where the platform starts a pass of its own.
        val embeddedRoots = PsiTreeUtil.collectElements(file) {
            it !is PsiFile && it.parent?.language?.equals(it.language) == false
        }
        assertNotEmpty(embeddedRoots.toList())

        val reachableSites = embeddedRoots.sumOf { keySitesIn(it, config).size }
        assertTrue(
            "the embedded roots have to be able to see key sites, or this test proves nothing",
            reachableSites > 0,
        )
        embeddedRoots.forEach { root ->
            assertEmpty(
                "${root.javaClass.simpleName} re-offered a fold the file's own pass already built, " +
                    "and the platform drops both",
                builder.buildFoldRegions(root, document, false).toList(),
            )
        }
        val fromTheFile = builder.buildFoldRegions(file, document, false)
        assertEquals("the file's own pass still folds every key site", 2, fromTheFile.size)
    }

    /**
     * What the feature costs on the real thing: the real catalog, a real settings-definition file dense with keys, the
     * repo's largest generated file (no keys at all, so pure walk cost), and the largest `.svelte` file, where walking
     * down from the `SvelteHTML` root has to expand every lazy-parsed script. Numbers are in `DETAILS.md`.
     */
    fun testFoldingTheRealRepoIsCheap() {
        myFixture.markProjectAsCmdrCheckout()
        val catalogFiles = myFixture.copyRealCatalogIntoFixture()

        val dense = measureFoldingCost("settings definitions", DENSE_FILE)
        assertTrue("a settings-definition file should be dense with keys, folded ${dense.folds}", dense.folds > 40)
        assertTrue("folding a dense real file took ${dense.warmMillis} ms", dense.warmMillis < BUDGET_MILLIS)
        println(
            "[perf] catalog: ${catalogFiles.size} files, first build included in the ${dense.coldMillis} ms cold run",
        )

        val generated = measureFoldingCost("generated bindings", SPARSE_FILE)
        assertEquals("the generated IPC bindings carry no user copy", 0, generated.folds)
        assertTrue("walking a 9k-line file took ${generated.warmMillis} ms", generated.warmMillis < BUDGET_MILLIS)

        if (skipWithoutSveltePlugin()) return
        val svelte = measureFoldingCost("the largest component", SVELTE_FILE)
        assertTrue("a real component should carry some copy, folded ${svelte.folds}", svelte.folds > 0)
        assertTrue("folding a 2k-line component took ${svelte.warmMillis} ms", svelte.warmMillis < BUDGET_MILLIS)
    }

    private class FoldingCost(val folds: Int, val coldMillis: Long, val warmMillis: Long)

    private fun measureFoldingCost(label: String, repoPath: String): FoldingCost {
        val source = RepoFiles.read(repoPath)
        myFixture.configureByText(File(repoPath).name, source)
        val builder = I18nKeyFoldingBuilder()

        val coldStarted = System.nanoTime()
        val descriptors = builder.buildFoldRegions(myFixture.file, myFixture.editor.document, false)
        val coldMillis = (System.nanoTime() - coldStarted) / 1_000_000

        val warmStarted = System.nanoTime()
        repeat(WARM_RUNS) { builder.buildFoldRegions(myFixture.file, myFixture.editor.document, false) }
        val warmMillis = (System.nanoTime() - warmStarted) / 1_000_000 / WARM_RUNS

        println(
            "[perf] $label (${source.lines().size} lines): ${descriptors.size} folds, " +
                "$coldMillis ms cold, $warmMillis ms warm",
        )
        return FoldingCost(descriptors.size, coldMillis, warmMillis)
    }

    /** Our fold regions, as the source text each replaces paired with what the editor shows instead. */
    private fun folds(): List<Pair<String, String>> {
        val document = myFixture.editor.document
        return foldRegions()
            .filter { it.placeholderText.startsWith(OPEN_QUOTE) }
            .sortedBy { it.startOffset }
            .map { document.getText(TextRange(it.startOffset, it.endOffset)) to it.placeholderText }
    }

    private fun placeholders(): List<String> = folds().map { it.second }

    private companion object {
        const val OPEN_QUOTE = "“"
        const val TITLE = "“Send crash report?”"
        const val RETITLED = "“Send this crash report?”"
        const val DENSE_FILE = "apps/desktop/src/lib/settings/definitions/advanced.ts"
        const val SPARSE_FILE = "apps/desktop/src/lib/ipc/bindings.ts"
        const val SVELTE_FILE = "apps/desktop/src/lib/file-explorer/pane/FilePane.svelte"
        const val WARM_RUNS = 5

        /** A folding pass runs per file on every edit, so anything near this would be felt as editor lag. */
        const val BUDGET_MILLIS = 50
    }
}
