package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.core.CmdrPluginConfig
import com.getcmdr.idea.core.CmdrProjectService
import com.google.gson.GsonBuilder
import com.google.gson.JsonObject
import com.intellij.lang.Language
import com.intellij.openapi.command.WriteCommandAction
import com.intellij.openapi.vfs.VfsUtil
import com.intellij.testFramework.fixtures.CodeInsightTestFixture
import java.io.File

/**
 * The project every i18n test needs: the marker that makes it a Cmdr checkout, plus a `messages/en/` to resolve keys
 * against. Folding and navigation assert different halves of the same setup, so it's shared rather than copied.
 *
 * The marker is always the repo's real `cmdr-plugin.json`, so a test fails the moment the shipped config stops
 * describing the key sites it assumes.
 */

/** The catalog file every fixture catalog is written to. Its name is arbitrary; the glob takes every `*.json`. */
internal const val CATALOG_FILE = "fixture.json"

/** Marks the fixture project a Cmdr checkout and gives it a catalog holding exactly [messages]. */
internal fun CodeInsightTestFixture.cmdrProjectWith(vararg messages: Pair<String, String>) {
    markProjectAsCmdrCheckout()
    addFileToProject("${catalogGlob().directory}/$CATALOG_FILE", catalogJson(messages))
}

/** Writes the marker with the repo's real config, so these tests fail if the shipped file stops saying this. */
internal fun CodeInsightTestFixture.markProjectAsCmdrCheckout() {
    addFileToProject(CmdrProjectService.CONFIG_PATH, RepoFiles.read(CmdrProjectService.CONFIG_PATH))
}

/** Rewrites the fixture catalog on disk, the way a `pnpm dev` run rewrites the real one under an open IDE. */
internal fun CodeInsightTestFixture.rewriteCatalog(vararg messages: Pair<String, String>) {
    val file = findFileInTempDir("${catalogGlob().directory}/$CATALOG_FILE")
    WriteCommandAction.runWriteCommandAction(project) { VfsUtil.saveText(file, catalogJson(messages)) }
}

/** The repo's own `messages/en/` files, so a measurement is against the real index and not a toy one. */
internal fun CodeInsightTestFixture.copyRealCatalogIntoFixture(): List<File> {
    val glob = catalogGlob()
    val files = File(repoRoot(), glob.directory).listFiles().orEmpty().filter { glob.files.matches(it.name) }
    check(files.size > 20) { "the catalog glob matched nothing; did `messages/en/` move?" }
    files.forEach { addFileToProject("${glob.directory}/${it.name}", it.readText()) }
    return files
}

/** The glob the shipped config names, so a fixture catalog lands exactly where the plugin looks for one. */
internal fun catalogGlob(): CatalogGlob = CatalogGlob.parse(repoI18nConfig().catalogGlob)

internal fun repoI18nConfig(): I18nConfig =
    CmdrPluginConfig.parse(RepoFiles.read(CmdrProjectService.CONFIG_PATH)).get(I18nConfig)
        ?: error("the shipped config no longer has an i18n section")

internal fun repoRoot(): String = System.getProperty("cmdr.repo.root") ?: error("cmdr.repo.root isn't set")

/**
 * Whether to skip a `.svelte` case. The Svelte plugin comes from the local IDE's plugin directory, so a fresh clone
 * on another machine has to stay green rather than fail on something it can't have.
 */
internal fun skipWithoutSveltePlugin(): Boolean {
    if (Language.findLanguageByID("SvelteHTML") != null) return false
    println("[test] SKIPPED: the Svelte plugin isn't on the test classpath (see `cmdrSveltePluginPath`).")
    return true
}

private fun catalogJson(messages: Array<out Pair<String, String>>): String {
    val root = JsonObject()
    messages.forEach { (key, value) -> root.addProperty(key, value) }
    return GsonBuilder().setPrettyPrinting().create().toJson(root)
}
