package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.core.CmdrPluginConfig
import com.getcmdr.idea.core.CmdrProjectService
import junit.framework.TestCase
import java.io.File

/** Turning the repo's catalog JSON into a key index, including over the real files the app ships. */
class MessageCatalogTest : TestCase() {
    fun testAKeyResolvesToItsMessage() {
        val catalog = MessageCatalog.of(listOf("""{"a.title": "Send crash report?"}"""))

        assertEquals("Send crash report?", catalog["a.title"])
        assertNull(catalog["a.missing"])
    }

    fun testTranslatorMetadataIsDroppedTheWayTheAppDropsIt() {
        val catalog = MessageCatalog.of(
            listOf("""{"a.title": "Title", "@a.title": {"description": "Says what the dialog is for."}}"""),
        )

        assertEquals(1, catalog.size)
        assertEquals("Title", catalog["a.title"])
        assertNull("an `@key` entry is translator metadata, never a message", catalog["@a.title"])
    }

    fun testEveryFileContributesToOneIndex() {
        val catalog = MessageCatalog.of(listOf("""{"a.one": "One"}""", """{"b.two": "Two"}"""))

        assertEquals("One", catalog["a.one"])
        assertEquals("Two", catalog["b.two"])
    }

    /** A `pnpm dev` run can catch a catalog file mid-write; one broken file must not blank the whole index. */
    fun testAMalformedFileContributesNothingRatherThanSinkingTheCatalog() {
        val catalog = MessageCatalog.of(listOf("{ not json at all", """{"a.one": "One"}"""))

        assertEquals("One", catalog["a.one"])
    }

    fun testTheRealCatalogParsesAndCarriesTheKeysTheAppUses() {
        val glob = CatalogGlob.parse(repoI18nConfig().catalogGlob)
        val files = File(repoRoot(), glob.directory).listFiles().orEmpty().filter { glob.files.matches(it.name) }
        assertTrue("the catalog glob matched nothing; did `messages/en/` move?", files.size > 20)

        val started = System.nanoTime()
        val catalog = MessageCatalog.of(files.map { it.readText() })
        val millis = (System.nanoTime() - started) / 1_000_000

        println("[perf] parsed ${files.size} catalog files into ${catalog.size} keys in $millis ms")
        assertTrue("expected thousands of keys, got ${catalog.size}", catalog.size > 2_000)
        assertEquals(
            "It includes the app version, macOS version, and which part of the code crashed. " +
                "No file names or personal data.",
            catalog["crashReporter.dialog.privacyNote"],
        )
    }

    fun testTheGlobSplitsIntoADirectoryAndAFileRule() {
        val glob = CatalogGlob.parse("apps/desktop/src/lib/intl/messages/en/*.json")

        assertEquals("apps/desktop/src/lib/intl/messages/en", glob.directory)
        assertTrue(glob.files.matches("settings.json"))
        assertFalse("only the English catalog, and only JSON", glob.files.matches("settings.json.bak"))
    }

    fun testTheShippedConfigDescribesEveryKeySiteShape() {
        val i18n = repoI18nConfig()

        assertTrue(i18n.functions.containsAll(listOf("t", "tString", "getMessage")))
        assertTrue(i18n.keyProperties.containsAll(listOf("labelKey", "descriptionKey", "titleKey", "cardKey")))
        assertTrue(i18n.isKeyAttribute("Trans", "key"))
        assertFalse(i18n.isKeyAttribute("Trans", "snippets"))
        assertFalse(i18n.isKeyAttribute("div", "key"))
    }

    private fun repoI18nConfig(): I18nConfig =
        CmdrPluginConfig.parse(RepoFiles.read(CmdrProjectService.CONFIG_PATH)).get(I18nConfig)
            ?: error("the shipped config no longer has an i18n section")

    private fun repoRoot(): String = System.getProperty("cmdr.repo.root") ?: error("cmdr.repo.root isn't set")
}
