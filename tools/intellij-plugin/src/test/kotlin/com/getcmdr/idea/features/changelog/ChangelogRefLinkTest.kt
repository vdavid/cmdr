package com.getcmdr.idea.features.changelog

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.core.CmdrProjectService
import com.intellij.codeInsight.navigation.actions.GotoDeclarationAction
import com.intellij.ide.browsers.BrowserLauncher
import com.intellij.ide.browsers.WebBrowser
import com.intellij.openapi.actionSystem.IdeActions
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.project.Project
import com.intellij.pom.Navigatable
import com.intellij.psi.util.PsiTreeUtil
import com.intellij.testFramework.fixtures.BasePlatformTestCase
import com.intellij.testFramework.replaceService
import org.intellij.plugins.markdown.lang.psi.impl.MarkdownParagraph
import java.io.File
import java.nio.file.Path

/**
 * The feature as the editor really sees it: a hash in `CHANGELOG.md` navigates to its GitHub commit and is painted like
 * a link. `ChangelogRefsTest` covers which hashes count as a sentence shape; this covers that real Markdown PSI hands
 * the rule what it expects, and that the answer reaches the editor at all.
 */
class ChangelogRefLinkTest : BasePlatformTestCase() {
    fun testAHashNavigatesToItsGithubCommit() {
        val text = changelog("## 0.37.0\n\n- Add a \"Chat memory size\" setting (75121419, 14aacf89)\n")

        assertEquals(commitUrl("75121419"), linkAt(text.indexOf("75121419") + 2))
        assertEquals(commitUrl("14aacf89"), linkAt(text.indexOf("14aacf89") + 2))
        assertNull("the prose before the group isn't a link", linkAt(text.indexOf("Chat memory")))
    }

    /**
     * That the rule is right is one thing; that ⌘-click reaches it at all is another, and it's the half a contributed
     * `WebReference` silently fails at on Markdown. This asserts the platform's own goto-declaration lookup, which is
     * what ⌘-click actually runs, finds the target.
     */
    fun testGotoDeclarationFindsTheCommitTarget() {
        val text = changelog("- Add a setting (75121419)\n")
        val offset = text.indexOf("75121419") + 2

        val targets = GotoDeclarationAction.findAllTargetElements(project, myFixture.editor, offset)

        assertEquals(1, targets.size)
        assertTrue("the target has to be navigable, or ⌘-click does nothing", targets.single() is Navigatable)
    }

    /**
     * The whole gesture, end to end: caret on a hash, run the action ⌘-click runs, and the IDE is asked to open the
     * commit page. This is the authoritative check on the click, because the tier 2 sandbox can't show it: the sandbox
     * IDE resolves `BrowserLauncher` to the remote-development backend one, which sends the request to a client that
     * isn't there, so no browser ever opens no matter how well the feature works.
     */
    fun testGoToDeclarationAsksTheBrowserForTheCommitPage() {
        val browser = RecordingBrowserLauncher()
        ApplicationManager.getApplication().replaceService(BrowserLauncher::class.java, browser, testRootDisposable)
        val text = changelog("- Add a setting (75121419)\n")
        myFixture.editor.caretModel.moveToOffset(text.indexOf("75121419") + 2)

        myFixture.performEditorAction(IdeActions.ACTION_GOTO_DECLARATION)

        assertEquals(listOf(commitUrl("75121419")), browser.urls)
    }

    fun testAHashIsPaintedWithTheLinkColor() {
        val text = changelog("- Add a setting (75121419)\n")

        assertEquals(listOf("75121419"), paintedAsLinks(text))
    }

    /**
     * The platform assumption the whole parse rests on: a bullet's paragraph spans the entry's wrapped source lines, so
     * a group broken across two lines is still one entry ending on its group. If Markdown ever stops doing that, the
     * rule silently stops matching wrapped entries, and this is what says so.
     */
    fun testAGroupWrappedAcrossTwoSourceLinesStillLinks() {
        val text = changelog(
            "- Credit all 775 open-source packages Cmdr ships (b626d7a4, 2d41cc14,\n  18add0b0, 42f76971)\n",
        )

        assertEquals(listOf("b626d7a4", "2d41cc14", "18add0b0", "42f76971"), paintedAsLinks(text))
        assertEquals(commitUrl("42f76971"), linkAt(text.indexOf("42f76971") + 2))
    }

    /** A nested bullet is its own entry, which is only true because Markdown gives it its own paragraph. */
    fun testAnIndentedNestedBulletLinksOnItsOwn() {
        val text = changelog("- A parent entry with prose and no group\n  - A nested detail (deadbeef)\n")

        assertEquals(listOf("deadbeef"), paintedAsLinks(text))
        assertEquals(commitUrl("deadbeef"), linkAt(text.indexOf("deadbeef") + 2))
    }

    fun testProseThatOnlyLooksLikeARefIsLeftAlone() {
        val text = changelog(
            "- Return a broad search in half a second (~40x speed-up!)\n" +
                "- Stop the (deadbeef) case crashing the parser on load\n" +
                "- Fix the thing (fd6fc29)\n",
        )

        assertNull(linkAt(text.indexOf("40x") + 1))
        assertNull("a hex-looking word mid-sentence is prose", linkAt(text.indexOf("deadbeef") + 2))
        assertNull(
            "a seven-character ref isn't one; the file is normalized to eight",
            linkAt(text.indexOf("fd6fc29") + 2),
        )
        assertEmpty(paintedAsLinks(text))
    }

    fun testAMarkdownFileThatIsNotAConfiguredChangelogIsLeftAlone() {
        markProjectAsCmdrCheckout()
        val text = "- Add a setting (75121419)\n"
        myFixture.configureByText("README.md", text)

        assertNull(linkAt(text.indexOf("75121419") + 2))
        assertEmpty(paintedAsLinks(text))
    }

    fun testAProjectWithoutTheMarkerIsLeftAlone() {
        // No `markProjectAsCmdrCheckout()`: the plugin has to be inert in every project that isn't this repo.
        val text = "- Add a setting (75121419)\n"
        myFixture.configureByText(CHANGELOG, text)

        assertNull(CmdrProjectService.getInstance(project).config)
        assertNull(linkAt(text.indexOf("75121419") + 2))
        assertEmpty(paintedAsLinks(text))
    }

    /** Marks the fixture project a Cmdr checkout and opens [text] as its `CHANGELOG.md`. Returns the text. */
    private fun changelog(text: String): String {
        markProjectAsCmdrCheckout()
        myFixture.configureByText(CHANGELOG, text)
        return text
    }

    /**
     * Writes the marker with the repo's real config, so these tests fail if the shipped file stops saying what they
     * assume. That one file is all detection needs; `CmdrProjectServiceTest` covers the rule itself.
     */
    private fun markProjectAsCmdrCheckout() {
        myFixture.addFileToProject(CmdrProjectService.CONFIG_PATH, RepoFiles.read(CmdrProjectService.CONFIG_PATH))
    }

    private fun linkAt(offset: Int): String? {
        val entry = PsiTreeUtil.getParentOfType(myFixture.file.findElementAt(offset), MarkdownParagraph::class.java)
        return entry?.let { commitLinkAt(it, offset) }
    }

    private fun paintedAsLinks(text: String): List<String> =
        myFixture.doHighlighting()
            .filter { it.forcedTextAttributesKey == ChangelogRefAnnotator.HYPERLINK }
            .map { text.substring(it.startOffset, it.endOffset) }

    private fun commitUrl(hash: String) = "https://github.com/vdavid/cmdr/commit/$hash"

    /** Stands in for the IDE's browser so the assertion is "we asked for this URL", with nothing actually opening. */
    private class RecordingBrowserLauncher : BrowserLauncher() {
        val urls = mutableListOf<String>()

        override fun open(url: String) {
            urls += url
        }

        override fun browse(url: String, browser: WebBrowser?, project: Project?) {
            urls += url
        }

        override fun browse(file: File) = error("the feature only ever browses URLs, never files")

        override fun browse(file: Path) = error("the feature only ever browses URLs, never files")
    }

    private companion object {
        const val CHANGELOG = "CHANGELOG.md"
    }
}
