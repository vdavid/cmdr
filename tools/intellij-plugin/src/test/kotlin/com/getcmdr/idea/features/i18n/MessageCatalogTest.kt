package com.getcmdr.idea.features.i18n

import junit.framework.TestCase
import java.io.File

/** Turning the repo's catalog JSON into a key index, including over the real files the app ships. */
class MessageCatalogTest : TestCase() {
    fun testAKeyResolvesToItsMessage() {
        val catalog = catalogOf("a.json" to """{"a.title": "Send crash report?"}""")

        assertEquals("Send crash report?", catalog["a.title"]?.text)
        assertNull(catalog["a.missing"])
    }

    /** Navigation's half of an entry: which file to open. Where in it comes from that file's own JSON PSI. */
    fun testAKeyRemembersTheCatalogFileItIsWrittenIn() {
        val catalog = catalogOf(
            "crashReporter.json" to """{"a.title": "Send crash report?"}""",
            "settings.json" to """{"b.label": "Provider"}""",
        )

        assertEquals("crashReporter.json", catalog["a.title"]?.fileName)
        assertEquals("settings.json", catalog["b.label"]?.fileName)
    }

    fun testTranslatorMetadataIsDroppedTheWayTheAppDropsIt() {
        val catalog = catalogOf(
            "a.json" to """{"a.title": "Title", "@a.title": {"description": "Says what the dialog is for."}}""",
        )

        assertEquals(1, catalog.size)
        assertEquals("Title", catalog["a.title"]?.text)
        assertNull("an `@key` entry is translator metadata, never a message", catalog["@a.title"])
    }

    fun testEveryFileContributesToOneIndex() {
        val catalog = catalogOf("a.json" to """{"a.one": "One"}""", "b.json" to """{"b.two": "Two"}""")

        assertEquals("One", catalog["a.one"]?.text)
        assertEquals("Two", catalog["b.two"]?.text)
    }

    /** A `pnpm dev` run can catch a catalog file mid-write; one broken file must not blank the whole index. */
    fun testAMalformedFileContributesNothingRatherThanSinkingTheCatalog() {
        val catalog = catalogOf("broken.json" to "{ not json at all", "a.json" to """{"a.one": "One"}""")

        assertEquals("One", catalog["a.one"]?.text)
    }

    fun testTheRealCatalogParsesAndCarriesTheKeysTheAppUses() {
        val glob = CatalogGlob.parse(repoI18nConfig().catalogGlob)
        val files = File(repoRoot(), glob.directory).listFiles().orEmpty().filter { glob.files.matches(it.name) }
        assertTrue("the catalog glob matched nothing; did `messages/en/` move?", files.size > 20)

        val started = System.nanoTime()
        val catalog = MessageCatalog.of(files.map { CatalogSource(it.name, it.readText()) })
        val millis = (System.nanoTime() - started) / 1_000_000

        println("[perf] parsed ${files.size} catalog files into ${catalog.size} keys in $millis ms")
        assertTrue("expected thousands of keys, got ${catalog.size}", catalog.size > 2_000)
        assertEquals(
            "It includes the app version, macOS version, and which part of the code crashed. " +
                "No file names or personal data.",
            catalog["crashReporter.dialog.privacyNote"]?.text,
        )
        assertEquals("crashReporter.json", catalog["crashReporter.dialog.privacyNote"]?.fileName)
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

    private fun catalogOf(vararg files: Pair<String, String>): MessageCatalog =
        MessageCatalog.of(files.map { (name, text) -> CatalogSource(name, text) })
}
