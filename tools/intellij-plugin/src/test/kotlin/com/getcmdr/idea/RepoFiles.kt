package com.getcmdr.idea

import java.io.File

/**
 * Reads files out of the Cmdr checkout the plugin lives in, so a test can assert against the real thing rather than a
 * fixture that agrees with it by hand. The root comes from the `cmdr.repo.root` system property, set in
 * `build.gradle.kts`.
 */
internal object RepoFiles {
    /** The file, or `null` when it moved or the property isn't set. Callers decide whether that's a skip or a fail. */
    fun find(relativePath: String): File? {
        val root = System.getProperty("cmdr.repo.root") ?: return null
        return File(root, relativePath).takeIf { it.isFile }
    }

    /** The file's text. Fails the test when it's missing, for files the repo is supposed to always carry. */
    fun read(relativePath: String): String =
        find(relativePath)?.readText() ?: error("$relativePath not found under cmdr.repo.root; did it move?")
}
