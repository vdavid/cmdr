package com.getcmdr.idea.core

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.intellij.openapi.diagnostic.logger
import java.util.Optional
import java.util.concurrent.ConcurrentHashMap

/**
 * What `cmdr-plugin.json` says, as sections.
 *
 * Core deliberately doesn't know what any section means: each feature declares a [FeatureConfig] and reads its own, so
 * adding a feature is a package under `features/`, a section here, and a registration in `plugin.xml`. **A feature is
 * off when its section is absent**, which is the whole toggle story (there's no settings panel, and no
 * defaults-versus-overrides merge).
 *
 * A section this build doesn't understand is ignored rather than rejected, so a config written for a later build still
 * loads everything the current one knows.
 */
class CmdrPluginConfig private constructor(private val root: JsonObject?) {
    private val features = ConcurrentHashMap<FeatureConfig<*>, Optional<Any>>()

    /**
     * [feature]'s view of its own section, read once per config instance, or `null` when the section is absent and the
     * feature is therefore off.
     *
     * Extension points run per PSI element, so re-reading JSON and recompiling a regex on every call would show up as
     * editor lag on a long file. The config object is replaced wholesale when the file changes, so the memo can't go
     * stale, and keying it by the [FeatureConfig] itself is what makes the cast back out safe.
     */
    fun <T : Any> get(feature: FeatureConfig<T>): T? {
        val cached = features.computeIfAbsent(feature) { Optional.ofNullable(section(feature.section)?.let(it::read)) }
        @Suppress("UNCHECKED_CAST")
        return cached.orElse(null) as T?
    }

    private fun section(name: String): ConfigSection? =
        root?.runCatching { getAsJsonObject(name) }?.getOrNull()?.let { ConfigSection(it) }

    companion object {
        /** No sections at all: what an unreadable or malformed file yields, so every feature simply stays off. */
        val EMPTY: CmdrPluginConfig = CmdrPluginConfig(null)

        private val log = logger<CmdrPluginConfig>()

        /** Parses the file's text. Malformed content yields [EMPTY] rather than throwing. */
        fun parse(text: String): CmdrPluginConfig {
            val root = runCatching { JsonParser.parseString(text) as? JsonObject }
                .onFailure { log.warn("cmdr-plugin.json isn't valid JSON, so every feature stays off", it) }
                .getOrNull()
                ?: return EMPTY
            return CmdrPluginConfig(root)
        }
    }
}

/**
 * A feature's typed view of its own section, and the key its parsed value is memoized under.
 *
 * One object per feature, normally the feature's config companion, so `config.get(ChangelogConfig)` reads the
 * `changelog` section and can only ever hand back a `ChangelogConfig`.
 */
abstract class FeatureConfig<T : Any>(val section: String) {
    /** Builds the feature's view. Returning `null` leaves the feature off even though its section exists. */
    abstract fun read(section: ConfigSection): T?
}

/** One section, read defensively: a missing field or a wrong type is an absent value, never an exception. */
class ConfigSection internal constructor(private val json: JsonObject) {
    fun string(name: String): String? = runCatching { json.get(name)?.asString }.getOrNull()

    fun stringList(name: String): List<String> =
        runCatching { json.getAsJsonArray(name)?.mapNotNull { it.asString } }.getOrNull().orEmpty()

    /** An array of objects, each read as its own section. Anything that isn't an object is skipped, not an error. */
    fun objects(name: String): List<ConfigSection> =
        runCatching { json.getAsJsonArray(name)?.mapNotNull { (it as? JsonObject)?.let(::ConfigSection) } }
            .getOrNull()
            .orEmpty()
}
