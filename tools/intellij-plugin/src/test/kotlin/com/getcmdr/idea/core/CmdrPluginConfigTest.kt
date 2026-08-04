package com.getcmdr.idea.core

import com.getcmdr.idea.RepoFiles
import com.getcmdr.idea.features.changelog.ChangelogConfig
import junit.framework.TestCase

/** Reading `cmdr-plugin.json`, including the real one the repo ships. */
class CmdrPluginConfigTest : TestCase() {
    fun testTheRepoConfigSaysWhatTheChangelogFeatureNeeds() {
        val changelog = CmdrPluginConfig.parse(RepoFiles.read(CmdrProjectService.CONFIG_PATH)).get(ChangelogConfig)

        assertNotNull("the shipped config no longer has a changelog section", changelog)
        assertEquals(listOf("CHANGELOG.md"), changelog!!.files)
        assertEquals("https://github.com/vdavid/cmdr/commit/75121419", changelog.commitUrl("75121419"))
    }

    fun testAFeatureIsOffWhenItsSectionIsAbsent() {
        assertNull(CmdrPluginConfig.parse("""{"i18n": {}}""").get(ChangelogConfig))
    }

    fun testASectionThisBuildDoesNotKnowIsIgnored() {
        // The whole point: a section a later build adds must not break the features that are here today.
        val config = CmdrPluginConfig.parse(
            """{"changelog": {"files": ["CHANGELOG.md"]}, "somethingNew": [1, 2]}""",
        )

        assertEquals(listOf("CHANGELOG.md"), config.get(ChangelogConfig)!!.files)
    }

    fun testMalformedContentLeavesEveryFeatureOffRatherThanThrowing() {
        assertNull(CmdrPluginConfig.parse("not json at all {{{").get(ChangelogConfig))
        assertNull(CmdrPluginConfig.parse("").get(ChangelogConfig))
        assertNull(CmdrPluginConfig.parse("[1, 2, 3]").get(ChangelogConfig))
    }

    fun testAWrongTypedFieldReadsAsAbsentRatherThanThrowing() {
        val changelog = CmdrPluginConfig.parse("""{"changelog": {"files": "CHANGELOG.md"}}""").get(ChangelogConfig)

        assertNotNull(changelog)
        assertEmpty(changelog!!.files)
    }

    fun testAnInvalidPatternFallsBackToTheBuiltInRule() {
        val section = """{"changelog": {"files": ["CHANGELOG.md"], "trailingGroupPattern": "([unclosed"}}"""

        val changelog = CmdrPluginConfig.parse(section).get(ChangelogConfig)

        assertNotNull(changelog)
        assertTrue(changelog!!.trailingGroupPattern.containsMatchIn("Add a setting (75121419)"))
    }

    private fun assertEmpty(actual: Collection<*>) = assertTrue("expected empty, got $actual", actual.isEmpty())
}
