package com.getcmdr.idea.features.i18n

import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive

/** One catalog file, as the index reads it: the name it's stored under, and what's in it. */
data class CatalogSource(val fileName: String, val text: CharSequence)

/**
 * What a key resolves to: the English text, and the catalog file it's written in.
 *
 * The file is the whole of the location the index carries. Which **line** the key sits on is left to the JSON file's
 * own PSI at the moment someone navigates, so the index stays one parse of text with no positions to keep in step.
 */
data class Message(val text: String, val fileName: String)

/**
 * The English message catalog, flattened across the JSON files [CatalogGlob] names: key to what it resolves to.
 *
 * ARB-style `@key` metadata entries are dropped, exactly as `stripMetadata` in
 * `apps/desktop/src/lib/intl/messages.svelte.ts` drops them, so what's in here is what the app would render.
 */
class MessageCatalog private constructor(private val messages: Map<String, Message>) {
    /** What [key] resolves to, or `null` when it isn't in the catalog. */
    operator fun get(key: String): Message? = messages[key]

    val size: Int get() = messages.size

    companion object {
        /**
         * Parses catalog files into one index. A file that isn't valid JSON contributes nothing rather than sinking
         * the whole catalog, because a `pnpm dev` session can catch one mid-write.
         */
        fun of(sources: List<CatalogSource>): MessageCatalog {
            val messages = HashMap<String, Message>()
            sources.forEach { source ->
                val root = runCatching { JsonParser.parseString(source.text.toString()) as? JsonObject }.getOrNull()
                root?.entrySet()?.forEach { (key, value) ->
                    val text = (value as? JsonPrimitive)?.takeIf { it.isString }?.asString
                    if (!key.startsWith(METADATA_PREFIX) && text != null) {
                        messages[key] = Message(text, source.fileName)
                    }
                }
            }
            return MessageCatalog(messages)
        }

        private const val METADATA_PREFIX = "@"
    }
}

/**
 * The `catalogGlob` config value, split into the directory to read and the file names to take from it.
 *
 * One `*` wildcard in the file part is all the config ever needs (every JSON file in `messages/en`), and keeping it
 * that small is what lets the directory double as the VFS invalidation scope.
 */
data class CatalogGlob(val directory: String, val files: Regex) {
    companion object {
        fun parse(glob: String): CatalogGlob {
            val lastSlash = glob.lastIndexOf('/')
            val names = glob.substring(lastSlash + 1)
            return CatalogGlob(
                directory = if (lastSlash < 0) "" else glob.substring(0, lastSlash),
                files = Regex(names.split("*").joinToString(".*") { Regex.escape(it) }),
            )
        }
    }
}
