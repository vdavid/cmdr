package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.core.CmdrPluginConfig
import com.getcmdr.idea.core.CmdrProjectService
import com.getcmdr.idea.platform.FoldingTestCase
import com.google.gson.GsonBuilder
import com.google.gson.JsonObject
import com.intellij.lang.Language
import com.intellij.openapi.command.WriteCommandAction
import com.intellij.openapi.util.TextRange
import com.intellij.openapi.vfs.VfsUtil
import java.io.File

/**
 * The feature as the editor really sees it: a resolvable key folds to its English text, everything else is left alone.
 *
 * `FoldedMessageTest` covers how a message reads once folded and `MessageCatalogTest` covers the index; this covers
 * that real PSI hands both of them what they expect, in every language we register for.
 */
class I18nKeyFoldingTest : FoldingTestCase() {
    fun testACallToEveryConfiguredFunctionFolds() {
        cmdrProjectWith("a.title" to "Send crash report?")
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
        cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")

        assertEquals(listOf("tString('a.title')" to TITLE), folds())
    }

    /**
     * A key property folds to its value only, unlike a call. `labelKey:` names which slot of a settings definition the
     * copy fills, so hiding it would lose more than it saves.
     */
    fun testAKeyPropertyFoldsItsValueAndKeepsItsName() {
        cmdrProjectWith("settings.ai.provider.label" to "Provider")
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
        cmdrProjectWith("a.title" to "Send crash report?")
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
        cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const gone = tString('a.keyThatWasRenamed')")

        assertEmpty(folds())
    }

    fun testIcuDoubledApostrophesAreUnescapedInTheEditor() {
        cmdrProjectWith("a.body" to "Cmdr quit unexpectedly. Here''s a crash report.")
        myFixture.configureByText("sample.ts", "const body = tString('a.body')")

        assertEquals(listOf("“Cmdr quit unexpectedly. Here's a crash report.”"), placeholders())
    }

    fun testAMultiSentenceMessageFoldsInFull() {
        val long = "It includes the app version, macOS version, and which part of the code crashed. " +
            "No file names or personal data. You can read the whole report before it's sent, and you can say no."
        cmdrProjectWith("a.privacyNote" to long)
        myFixture.configureByText("sample.ts", "const note = tString('a.privacyNote')")

        assertEquals(listOf("“$long”"), placeholders())
    }

    fun testAPlaceholderSurvivesVerbatim() {
        cmdrProjectWith("a.copying" to "Copying {countText} to {target}")
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
        cmdrProjectWith("a.title" to "Send crash report?")
        myFixture.configureByText("sample.ts", "const title = tString('a.title')")
        assertEquals(listOf(TITLE), placeholders())

        rewriteCatalog("a.title" to "Send this crash report?")

        assertEquals(
            "the index kept the old copy, so the VFS invalidation didn't fire",
            "Send this crash report?",
            MessageCatalogService.getInstance(project).catalog()?.get("a.title"),
        )
        assertEquals("the open editor kept the copy it was opened with", listOf(RETITLED), placeholders())
    }

    /**
     * `isCollapsedByDefault` is asserted on the descriptor rather than on a materialized region: the platform call
     * that applies default collapse state, `updateFoldRegionsAsync`, throws on the EDT in a headless fixture. Tier 2
     * is what confirms a freshly opened file really shows the text.
     */
    fun testAFoldIsCollapsedByDefault() {
        cmdrProjectWith("a.title" to "Send crash report?")
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
        cmdrProjectWith("a.title" to "Send crash report?")
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
        cmdrProjectWith("ui.loadingIcon.cancelHint" to "Press {key} to cancel")
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
     * What the feature costs on the real thing: the real catalog, a real settings-definition file dense with keys, the
     * repo's largest generated file (no keys at all, so pure walk cost), and the largest `.svelte` file, where walking
     * down from the `SvelteHTML` root has to expand every lazy-parsed script. Numbers are in `DETAILS.md`.
     */
    fun testFoldingTheRealRepoIsCheap() {
        markProjectAsCmdrCheckout()
        val catalogFiles = copyRealCatalogIntoFixture()

        val dense = measureFoldingCost("settings definitions", DENSE_FILE)
        assertTrue("a settings-definition file should be dense with keys, folded ${dense.folds}", dense.folds > 40)
        assertTrue("folding a dense real file took ${dense.warmMillis} ms", dense.warmMillis < BUDGET_MILLIS)
        println("[perf] catalog: $catalogFiles files, first build included in the ${dense.coldMillis} ms cold run")

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

    /** Marks the fixture a Cmdr checkout and gives it a catalog holding exactly [messages]. */
    private fun cmdrProjectWith(vararg messages: Pair<String, String>) {
        markProjectAsCmdrCheckout()
        myFixture.addFileToProject("${catalogGlob().directory}/$CATALOG_FILE", catalogJson(messages))
    }

    /** Writes the marker with the repo's real config, so these tests fail if the shipped file stops saying this. */
    private fun markProjectAsCmdrCheckout() {
        myFixture.addFileToProject(CmdrProjectService.CONFIG_PATH, RepoFiles.read(CmdrProjectService.CONFIG_PATH))
    }

    private fun rewriteCatalog(vararg messages: Pair<String, String>) {
        val file = myFixture.findFileInTempDir("${catalogGlob().directory}/$CATALOG_FILE")
        WriteCommandAction.runWriteCommandAction(project) { VfsUtil.saveText(file, catalogJson(messages)) }
    }

    /** The repo's own `messages/en/` files, so the cost measurement is the real index and not a toy one. */
    private fun copyRealCatalogIntoFixture(): Int {
        val glob = catalogGlob()
        val files = File(repoRoot(), glob.directory).listFiles().orEmpty().filter { glob.files.matches(it.name) }
        assertTrue("the catalog glob matched nothing; did `messages/en/` move?", files.size > 20)
        files.forEach { myFixture.addFileToProject("${glob.directory}/${it.name}", it.readText()) }
        return files.size
    }

    private fun catalogJson(messages: Array<out Pair<String, String>>): String {
        val root = JsonObject()
        messages.forEach { (key, value) -> root.addProperty(key, value) }
        return GsonBuilder().setPrettyPrinting().create().toJson(root)
    }

    private fun catalogGlob(): CatalogGlob {
        val config = CmdrPluginConfig.parse(RepoFiles.read(CmdrProjectService.CONFIG_PATH)).get(I18nConfig)
            ?: error("the shipped config no longer has an i18n section")
        return CatalogGlob.parse(config.catalogGlob)
    }

    private fun repoRoot(): String = System.getProperty("cmdr.repo.root") ?: error("cmdr.repo.root isn't set")

    private fun skipWithoutSveltePlugin(): Boolean {
        if (Language.findLanguageByID("SvelteHTML") != null) return false
        println("[test] SKIPPED: the Svelte plugin isn't on the test classpath (see `cmdrSveltePluginPath`).")
        return true
    }

    private companion object {
        const val OPEN_QUOTE = "“"
        const val TITLE = "“Send crash report?”"
        const val RETITLED = "“Send this crash report?”"
        const val CATALOG_FILE = "fixture.json"
        const val DENSE_FILE = "apps/desktop/src/lib/settings/definitions/advanced.ts"
        const val SPARSE_FILE = "apps/desktop/src/lib/ipc/bindings.ts"
        const val SVELTE_FILE = "apps/desktop/src/lib/file-explorer/pane/FilePane.svelte"
        const val WARM_RUNS = 5

        /** A folding pass runs per file on every edit, so anything near this would be felt as editor lag. */
        const val BUDGET_MILLIS = 50
    }
}
