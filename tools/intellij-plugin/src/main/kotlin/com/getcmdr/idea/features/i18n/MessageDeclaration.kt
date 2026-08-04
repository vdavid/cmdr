package com.getcmdr.idea.features.i18n

import com.intellij.json.psi.JsonFile
import com.intellij.json.psi.JsonObject
import com.intellij.json.psi.JsonProperty
import com.intellij.openapi.project.Project
import com.intellij.psi.PsiManager

/**
 * Where a message key is written: the entry in the English catalog, so navigation lands on the key itself with the
 * translator's `@key` description in the next line or two.
 *
 * Split in two on purpose. [MessageCatalogService] answers **which file**, out of the index it already keeps for
 * folding; the file's own JSON PSI answers **where in it**, once, when someone actually navigates. So the index stays
 * a single Gson parse of text with no positions in it, and the position that reaches the editor is always the file as
 * it is now rather than as it was when the index was built.
 */
internal fun messageDeclaration(project: Project, key: String): JsonProperty? {
    val file = MessageCatalogService.getInstance(project).sourceOf(key) ?: return null
    val json = PsiManager.getInstance(project).findFile(file) as? JsonFile ?: return null
    return (json.topLevelValue as? JsonObject)?.findProperty(key)
}
