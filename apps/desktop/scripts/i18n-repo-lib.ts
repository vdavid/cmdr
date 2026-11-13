/**
 * The repo-side plumbing the translator tools share: git, the main clone (where
 * the gitignored reference pile lives), the English as a branch left a ref, and
 * the one-key-per-line list files. `pnpm i18n:brief`, `pnpm i18n:apply`,
 * `pnpm i18n:pile`, and `sync-locale-keys.ts` all read through here, so a
 * worktree resolves the same main clone and a key list parses the same way
 * everywhere.
 */

import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { BASE_LOCALE, mergeCatalogFiles, readLocaleFiles, resolveMessagesRoot } from './i18n-catalog-lib.ts'

/** Runs git in `cwd`, returning trimmed stdout, or `undefined` when git fails. */
export function git(cwd: string, args: string[]): string | undefined {
  try {
    return execFileSync('git', ['-C', cwd, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      maxBuffer: 64 * 1024 * 1024,
    }).trim()
  } catch {
    return undefined
  }
}

/**
 * The checkout these scripts run in, and the MAIN clone's root. A worktree's
 * common git dir is the main clone's `.git`, so its parent is the main clone,
 * wherever the worktree sits; outside git both fall back to this file's repo.
 */
export function repoRoots(): { repoRoot: string; mainClone: string } {
  const fallback = join(import.meta.dirname, '..', '..', '..')
  const repoRoot = git(import.meta.dirname, ['rev-parse', '--show-toplevel']) ?? fallback
  const common = git(import.meta.dirname, ['rev-parse', '--path-format=absolute', '--git-common-dir'])
  return { repoRoot, mainClone: common ? dirname(common) : repoRoot }
}

/** The reference pile's root (`<main clone>/_ignored/i18n`), reachable from any worktree. */
export function pileRoot(): string {
  return join(repoRoots().mainClone, '_ignored', 'i18n')
}

/**
 * The pile folders a catalog tag mines, most authoritative first, when they aren't
 * the bare tag: Cmdr's `pt` ships Brazilian and the bare `pt/` pile is European;
 * `zh-Hant` keeps its Microsoft sources under `zh-Hant/` and its macOS and Linux
 * ones under `zh-TW/` (`docs/i18n/reference-pile/how-to-mine.md`, traps 1 and 6).
 */
const PILE_FOLDERS: Readonly<Record<string, readonly string[]>> = { pt: ['pt-BR'], 'zh-Hant': ['zh-Hant', 'zh-TW'] }

/** The pile folders a catalog tag mines (`pt` → `pt-BR`), in the order the brief names them. */
export function pileFolders(tag: string): readonly string[] {
  return PILE_FOLDERS[tag] ?? [tag]
}

/**
 * The commit `--changed-since <ref>` diffs against: where this branch left the
 * ref (`git merge-base <ref> HEAD`), not the ref's tip. Diffing against the tip
 * would also pick up every key the ref changed after the split, which this
 * branch never touched.
 * @param runGit runs git with these args, returning trimmed stdout or `undefined` on failure
 */
export function changeBase(ref: string, runGit: (args: string[]) => string | undefined): string {
  const base = runGit(['merge-base', ref, 'HEAD'])
  if (!base) throw new Error(`--changed-since: "${ref}" isn't a git ref here, or shares no history with HEAD`)
  return base
}

/**
 * The English messages where this branch left a git ref (`changeBase`). A catalog
 * file that didn't exist then contributes nothing.
 */
export function englishAt(ref: string, messagesRoot?: string): Record<string, string> {
  const enDir = join(resolveMessagesRoot(messagesRoot), BASE_LOCALE)
  const base = changeBase(ref, (args) => git(enDir, args))
  const files: Record<string, Record<string, unknown>> = {}
  for (const name of Object.keys(readLocaleFiles(BASE_LOCALE, messagesRoot))) {
    const text = git(enDir, ['show', `${base}:./${name}`])
    if (text !== undefined) files[name] = JSON.parse(text) as Record<string, unknown>
  }
  return mergeCatalogFiles(files).messages
}

/** A list file's entries: one per line, trimmed, blank lines and `#` comments skipped. */
export function readListFile(path: string): string[] {
  return parseList(readFileSync(path, 'utf8'))
}

/** `readListFile` over text already in hand. */
export function parseList(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('#'))
}
