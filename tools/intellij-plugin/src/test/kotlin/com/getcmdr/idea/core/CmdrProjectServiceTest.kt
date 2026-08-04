package com.getcmdr.idea.core

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.features.changelog.ChangelogConfig
import com.intellij.openapi.application.WriteAction
import com.intellij.openapi.vfs.VfsUtil
import com.intellij.testFramework.fixtures.BasePlatformTestCase

/** Detection and config loading: the seam every feature opens with. */
class CmdrProjectServiceTest : BasePlatformTestCase() {
    fun testAProjectWithoutTheMarkerIsNotACmdrCheckout() {
        myFixture.addFileToProject("CHANGELOG.md", "- Add a setting (75121419)\n")

        assertNull(service.config)
    }

    fun testAnyProjectCarryingTheMarkerIsACmdrCheckout() {
        // Detection is the file's presence and nothing else: no project name to match, no absolute path to configure.
        // This fixture project lives in a temp directory under a generated name, which is exactly why a worktree
        // checkout, or a second clone under any name at all, is recognized for free.
        addMarker(RepoFiles.read(CmdrProjectService.CONFIG_PATH))

        assertNotNull(service.config)
        assertEquals(listOf("CHANGELOG.md"), service.config!!.get(ChangelogConfig)!!.files)
    }

    fun testTheMarkerOnlyCountsAtTheProjectBase() {
        // A vendored copy of the repo, or the plugin's own directory opened as a project, isn't a Cmdr checkout.
        myFixture.addFileToProject("vendor/cmdr/${CmdrProjectService.CONFIG_PATH}", """{"changelog": {}}""")

        assertNull(service.config)
    }

    fun testRewritingTheMarkerIsPickedUp() {
        val marker = addMarker("""{"changelog": {"files": ["CHANGELOG.md"]}}""")
        assertEquals(listOf("CHANGELOG.md"), service.config!!.get(ChangelogConfig)!!.files)

        WriteAction.runAndWait<Throwable> {
            VfsUtil.saveText(marker, """{"changelog": {"files": ["docs/CHANGELOG.md"]}}""")
        }

        assertEquals(listOf("docs/CHANGELOG.md"), service.config!!.get(ChangelogConfig)!!.files)
    }

    fun testDeletingTheMarkerStopsTheProjectBeingACmdrCheckout() {
        val marker = addMarker("""{"changelog": {"files": ["CHANGELOG.md"]}}""")
        assertNotNull(service.config)

        WriteAction.runAndWait<Throwable> { marker.delete(this) }

        assertNull(service.config)
    }

    fun testProjectRelativePathIsWhatTheConfigMatchesAgainst() {
        val nested = myFixture.addFileToProject("docs/notes/CHANGELOG.md", "")

        assertEquals("docs/notes/CHANGELOG.md", service.projectRelativePath(nested.virtualFile))
    }

    private val service: CmdrProjectService get() = CmdrProjectService.getInstance(project)

    private fun addMarker(text: String) =
        myFixture.addFileToProject(CmdrProjectService.CONFIG_PATH, text).virtualFile
}
