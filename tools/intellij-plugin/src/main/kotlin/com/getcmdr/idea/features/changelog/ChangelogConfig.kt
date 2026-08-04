package com.getcmdr.idea.features.changelog

import com.getcmdr.idea.core.CmdrProjectService
import com.getcmdr.idea.core.ConfigSection
import com.getcmdr.idea.core.FeatureConfig
import com.intellij.openapi.diagnostic.logger
import com.intellij.psi.PsiElement

/** The `changelog` section of `cmdr-plugin.json`: which files carry refs, how one becomes a URL, how one is spotted. */
data class ChangelogConfig(
    /** Project-relative paths, matched exactly. `CHANGELOG.md` is the only one today. */
    val files: List<String>,
    val commitUrlTemplate: String,
    val trailingGroupPattern: Regex,
) {
    fun commitUrl(hash: String): String = commitUrlTemplate.replace(HASH_PLACEHOLDER, hash)

    companion object : FeatureConfig<ChangelogConfig>("changelog") {
        private const val HASH_PLACEHOLDER = "{hash}"
        private const val DEFAULT_COMMIT_URL = "https://github.com/vdavid/cmdr/commit/$HASH_PLACEHOLDER"

        private val log = logger<ChangelogConfig>()

        override fun read(section: ConfigSection): ChangelogConfig = ChangelogConfig(
            files = section.stringList("files"),
            commitUrlTemplate = section.string("commitUrl") ?: DEFAULT_COMMIT_URL,
            trailingGroupPattern = section.trailingGroupPattern(),
        )

        private fun ConfigSection.trailingGroupPattern(): Regex {
            val configured = string("trailingGroupPattern") ?: return Regex(ChangelogRefs.DEFAULT_TRAILING_GROUP_PATTERN)
            return runCatching { Regex(configured) }
                .onFailure { log.warn("trailingGroupPattern isn't a valid regex, using the built-in rule", it) }
                .getOrElse { Regex(ChangelogRefs.DEFAULT_TRAILING_GROUP_PATTERN) }
        }
    }
}

/**
 * The config that applies to the file [element] sits in, or `null` when the feature has nothing to do here: not a Cmdr
 * checkout, no `changelog` section, or a markdown file that isn't one of the configured ones.
 *
 * This is what keeps the feature free in every other project and every other file.
 */
internal fun changelogConfigFor(element: PsiElement): ChangelogConfig? {
    val file = element.containingFile?.originalFile?.virtualFile ?: return null
    val service = CmdrProjectService.getInstance(element.project)
    val config: ChangelogConfig = service.config?.get(ChangelogConfig) ?: return null
    val relativePath = service.projectRelativePath(file) ?: return null
    return config.takeIf { relativePath in it.files }
}
