package com.getcmdr.idea.features.i18n

import com.getcmdr.idea.core.CmdrProjectService
import com.intellij.codeInsight.folding.CodeFoldingManager
import com.intellij.openapi.Disposable
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.project.Project
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.vfs.VfsUtilCore
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.vfs.VirtualFileManager
import com.intellij.openapi.vfs.newvfs.BulkFileListener
import com.intellij.openapi.vfs.newvfs.events.VFileEvent
import java.util.concurrent.atomic.AtomicReference

/**
 * The English catalog, parsed once per project and shared by everything that resolves a key.
 *
 * Parsing is ~1 MB of JSON across ~30 files: far too much to redo per folding pass, and far too little to be worth a
 * platform index. So it's cached, and invalidated by VFS events under the catalog directory, because a `pnpm dev`
 * session rewrites those files while the IDE is open and a fold showing yesterday's copy is worse than no fold.
 */
@Service(Service.Level.PROJECT)
class MessageCatalogService(private val project: Project) : Disposable {
    private val cached = AtomicReference<Snapshot?>(null)

    /** The glob a missing catalog directory was last reported for, so the warning lands once rather than per pass. */
    private val missingDirectoryReported = AtomicReference<String?>(null)

    init {
        project.messageBus.connect(this).subscribe(
            VirtualFileManager.VFS_CHANGES,
            object : BulkFileListener {
                override fun after(events: List<VFileEvent>) {
                    val watched = cached.get()?.directory?.path ?: return
                    // The directory itself counts, not just what's under it: deleting or renaming `messages/en`
                    // is exactly the event that must drop the cache, and it carries the directory's own path.
                    if (events.none { it.path == watched || it.path.startsWith("$watched/") }) return
                    cached.set(null)
                    refoldOpenEditors()
                }
            },
        )
    }

    /**
     * The catalog, or `null` when there's nothing to resolve against: not a Cmdr checkout, no `i18n` section, or a
     * `catalogGlob` pointing at a directory that isn't there.
     *
     * Callers get a snapshot they can hold for the length of a folding pass; a rewrite mid-pass shows up on the next.
     */
    fun catalog(): MessageCatalog? = snapshot()?.catalog

    /**
     * The catalog file [key] is written in, or `null` when nothing resolves it.
     *
     * This is the whole of what navigation needs from the index. Finding the key's **line** inside the file is the
     * JSON PSI's job at click time, so the index never carries offsets that could go stale against the file.
     */
    fun sourceOf(key: String): VirtualFile? {
        val snapshot = snapshot() ?: return null
        val message = snapshot.catalog[key] ?: return null
        return snapshot.directory.findChild(message.fileName)
    }

    private fun snapshot(): Snapshot? {
        val glob = CmdrProjectService.getInstance(project).config?.get(I18nConfig)?.catalogGlob ?: return null
        cached.get()?.let { if (it.glob == glob) return it }

        val rule = CatalogGlob.parse(glob)
        val directory = project.guessProjectDir()?.findFileByRelativePath(rule.directory)?.takeIf { it.isDirectory }
        if (directory == null) {
            // Once per glob, not once per call: this runs on every folding pass, and a Cmdr checkout whose
            // `messages/en/` has gone away would otherwise fill the log with the same line.
            if (missingDirectoryReported.getAndSet(glob) != glob) {
                log.warn("no catalog directory at `$glob`, so no key can resolve")
            }
            return null
        }
        val catalog = MessageCatalog.of(
            directory.children
                .filter { !it.isDirectory && rule.files.matches(it.name) }
                .mapNotNull { file ->
                    runCatching { VfsUtilCore.loadText(file) }.getOrNull()?.let { CatalogSource(file.name, it) }
                },
        )
        return Snapshot(glob, directory, catalog).also(cached::set)
    }

    /**
     * Asks every open editor to fold again. Dropping the index isn't enough on its own: a fold region that already
     * exists keeps the placeholder it was built with, so a file open when the catalog changed would go on showing the
     * old copy until it was edited or reopened.
     *
     * `scheduleAsyncFoldingUpdate` is the call that gets past that. `updateFoldRegions`, `releaseFoldings`,
     * `dropPsiCaches`, and a daemon restart all leave the old text in place; `DETAILS.md` records the measurement.
     */
    private fun refoldOpenEditors() {
        if (project.isDisposed) return
        val folding = CodeFoldingManager.getInstance(project)
        EditorFactory.getInstance().allEditors
            .filter { it.project == project }
            .forEach(folding::scheduleAsyncFoldingUpdate)
    }

    override fun dispose() = Unit

    /** What was parsed, and what invalidates it: the glob it came from and the directory to watch. */
    private class Snapshot(val glob: String, val directory: VirtualFile, val catalog: MessageCatalog)

    companion object {
        private val log = logger<MessageCatalogService>()

        fun getInstance(project: Project): MessageCatalogService = project.service()
    }
}
