/**
 * The brief's notes about where a run's files live: each language's pile folders (`pilePaths`), its run directory and its
 * batch-scoped check (`workDirLines`, in the header), and the frozen macOS text a key carries (`frozenMacosNote`, on the
 * key). Pure; `i18n-brief-lib.ts` assembles them, and the CLI makes the directories and reads the baseline.
 */

import { join } from 'node:path'
import { pileFolders } from './i18n-repo-lib.ts'

/**
 * Each language's run directory, its values file (what `pnpm i18n:apply` reads by default), and the check scoped to
 * the batch, so "done when every line is a ✓" is reachable: the locale's other stale keys belong to other batches.
 */
export function workDirLines(
  langs: readonly string[],
  workDirs: Readonly<Record<string, string>> | undefined,
): string[] {
  if (!workDirs) return []
  return langs.flatMap((tag) => {
    const dir = workDirs[tag] as string | undefined
    if (dir === undefined) return []
    const who = langs.length > 1 ? `${tag}: ` : ''
    return [
      `- ${who}Your files: \`${dir}/\` (gitignored). Write your values to \`${join(dir, 'values.json')}\`; \`pnpm i18n:apply --lang ${tag}\` reads it and checks it against \`keys.txt\` there.`,
      `- ${who}Check: \`pnpm i18n:check-locale ${tag} --keys-file ${join(dir, 'keys.txt')}\` (in apps/desktop); \`stale\` judges only this batch.`,
    ]
  })
}

/**
 * The note on a key whose macOS text a Mac already reads (the frozen copy baseline): which targets hold it, that the
 * text stays, and the one reviewed way to fix a wrong one. Per-OS keys get it too, beside their per-OS note.
 */
export function frozenMacosNote(tags: readonly string[], perOs: boolean): string | undefined {
  if (tags.length === 0) return undefined
  const what = perOs ? 'the branch macOS reads' : 'this whole value (macOS reads it)'
  return `Frozen macOS text (${tags.join(', ')}): keep ${what} byte for byte. If it's wrong (a mistranslation, an outdated term) or its English was reworded so it no longer fits, fix it and add an \`ACCEPTED_TRANSLATION_CHANGES\` entry (tag, key, the new macOS text, a one-line reason) in apps/desktop/scripts/i18n-macos-copy-baseline-lib.ts; \`pnpm i18n:apply\` refuses the change until it's there.`
}

/** A language's pile folders as the header names them: `<root>/pt-BR/`, or several joined. */
export function pilePaths(pileRootDir: string, tag: string): string {
  return pileFolders(tag)
    .map((folder) => `\`${join(pileRootDir, folder)}/\``)
    .join(' + ')
}
