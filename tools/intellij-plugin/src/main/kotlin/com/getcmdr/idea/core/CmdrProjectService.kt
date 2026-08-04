package com.getcmdr.idea.core

import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.vfs.VfsUtilCore
import com.intellij.openapi.vfs.VirtualFile
import java.util.concurrent.atomic.AtomicReference

/**
 * Answers one question for every feature: **is this project a Cmdr checkout, and what does its config say?**
 *
 * A checkout is one with [CONFIG_PATH] under its base directory. Nothing else counts: no project-name matching and no
 * absolute paths, so a worktree, a second clone, or a directory called anything at all is recognized for free.
 *
 * Every feature's extension point calls [config] first and returns immediately when it's `null`, which is what keeps
 * the plugin inert in every project that isn't this repo.
 */
@Service(Service.Level.PROJECT)
class CmdrProjectService(private val project: Project) {
    private val parsed = AtomicReference<Parsed?>(null)

    /**
     * The parsed config, or `null` when this project isn't a Cmdr checkout.
     *
     * Locating the file is a couple of VFS lookups and happens every call, so a marker that appears, disappears, or
     * moves with a branch switch is picked up with no listener to keep in sync. Only the parse is cached, keyed by the
     * file's modification stamp, because that's the part that costs: reading JSON and compiling a regex on every
     * annotated element would show up as editor lag on a long file.
     */
    val config: CmdrPluginConfig?
        get() {
            val marker = project.guessProjectDir()?.findFileByRelativePath(CONFIG_PATH) ?: return null
            val stamp = marker.modificationStamp
            parsed.get()?.let { if (it.file == marker && it.stamp == stamp) return it.config }

            val config = runCatching { CmdrPluginConfig.parse(VfsUtilCore.loadText(marker)) }
                .onFailure { log.warn("couldn't read $CONFIG_PATH, so every feature stays off", it) }
                .getOrDefault(CmdrPluginConfig.EMPTY)
            parsed.set(Parsed(marker, stamp, config))
            return config
        }

    /** The file's path relative to the project base dir, `/`-separated, or `null` when it sits outside. */
    fun projectRelativePath(file: VirtualFile): String? {
        val baseDir = project.guessProjectDir() ?: return null
        return VfsUtilCore.getRelativePath(file, baseDir)
    }

    private class Parsed(val file: VirtualFile, val stamp: Long, val config: CmdrPluginConfig)

    companion object {
        /** The marker file, and the config: one file does both jobs. Project-relative, always. */
        const val CONFIG_PATH: String = "tools/intellij-plugin/cmdr-plugin.json"

        private val log = logger<CmdrProjectService>()

        fun getInstance(project: Project): CmdrProjectService = project.service()
    }
}
