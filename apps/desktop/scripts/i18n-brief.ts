#!/usr/bin/env node
/**
 * `pnpm i18n:brief`: prints the translation brief for one batch of keys, the only
 * doc a translator agent needs to load up front. Assembly lives in
 * `i18n-brief-lib.ts`; this file parses flags, resolves the repo paths, and reads
 * git for `--changed-since`.
 *
 *   --lang nl | nl,de | all          target locales (`all` = every full translation)
 *   --keys a.b.c,servers.sheet.*     exact keys or `*` globs
 *   --missing                        English keys absent in any target locale
 *   --changed-since <git-ref>        English keys added or changed since the ref
 *   --exclude-target-values          blind run: withhold the batch's current translations and decisions
 *   --out <file>                     write there instead of stdout
 *   --stats                          per-section size to stderr
 *   --messages-root <dir>, --docs-root <dir>   fixtures
 *
 * Key selectors narrow each other: `--missing --keys servers.*` is the missing
 * keys under `servers.`. Schemas and the process: `docs/i18n/termbase.md`,
 * `docs/guides/i18n-translation.md`.
 */

import { execFileSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import {
  BASE_LOCALE,
  listLocales,
  loadCatalog,
  mergeCatalogFiles,
  readLocaleFiles,
  resolveLocaleSource,
  resolveMessagesRoot,
} from './i18n-catalog-lib.ts'
import { EXIT_ERROR } from './i18n-locale-check-lib.ts'
import { buildBrief, changedKeys, renderBrief, roughTokens, selectKeys } from './i18n-brief-lib.ts'

/** The value after a `--flag`, or `undefined`. */
function flagValue(args: readonly string[], flag: string): string | undefined {
  const index = args.indexOf(flag)
  return index === -1 ? undefined : args[index + 1]
}

/** Runs git in `cwd`, returning trimmed stdout, or `undefined` when git fails. */
function git(cwd: string, args: string[]): string | undefined {
  try {
    return execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()
  } catch {
    return undefined
  }
}

/** The worktree's root, and the MAIN clone's root (where the gitignored reference pile lives). */
function repoRoots(): { repoRoot: string; mainClone: string } {
  const fallback = join(import.meta.dirname, '..', '..', '..')
  const repoRoot = git(import.meta.dirname, ['rev-parse', '--show-toplevel']) ?? fallback
  const common = git(import.meta.dirname, ['rev-parse', '--path-format=absolute', '--git-common-dir'])
  return { repoRoot, mainClone: common ? dirname(common) : repoRoot }
}

/** Every locale the brief can target: full translations, not overlays or the base. */
function fullTranslations(messagesRoot?: string): string[] {
  const available = listLocales(messagesRoot)
  return available.filter((tag) => tag !== BASE_LOCALE && !resolveLocaleSource(tag, available).isOverlay)
}

/** The English messages as of a git ref. A catalog file that didn't exist then contributes nothing. */
function englishAt(ref: string, messagesRoot?: string): Record<string, string> {
  const enDir = join(resolveMessagesRoot(messagesRoot), BASE_LOCALE)
  const files: Record<string, Record<string, unknown>> = {}
  for (const name of Object.keys(readLocaleFiles(BASE_LOCALE, messagesRoot))) {
    const text = git(enDir, ['show', `${ref}:./${name}`])
    if (text !== undefined) files[name] = JSON.parse(text) as Record<string, unknown>
  }
  if (git(enDir, ['rev-parse', '--verify', '--quiet', `${ref}^{commit}`]) === undefined) {
    throw new Error(`--changed-since: "${ref}" isn't a git ref here`)
  }
  return mergeCatalogFiles(files).messages
}

/** The `--lang` locales, each checked to be a full translation. */
function resolveLangs(langFlag: string | undefined, messagesRoot?: string): string[] {
  if (!langFlag) throw new Error('Pass --lang <tag>[,<tag>…] or --lang all')
  const full = fullTranslations(messagesRoot)
  const langs = langFlag === 'all' ? full : langFlag.split(',').map((tag) => tag.trim())
  const unknown = langs.filter((tag) => !full.includes(tag))
  if (unknown.length > 0) throw new Error(`Not a full translation: ${unknown.join(', ')} (have: ${full.join(', ')})`)
  return langs
}

/** The batch, from whichever key selectors were passed, plus how to describe it in the header. */
function resolveKeys(args: readonly string[], langs: readonly string[], messagesRoot?: string) {
  const keysFlag = flagValue(args, '--keys')
  const changedRef = flagValue(args, '--changed-since')
  const missing = args.includes('--missing')
  if (!keysFlag && !changedRef && !missing)
    throw new Error('Pick keys: --keys, --missing, and/or --changed-since <ref>')
  const en = loadCatalog(BASE_LOCALE, messagesRoot).messages
  const keys = selectKeys({
    en,
    patterns: keysFlag?.split(',').map((pattern) => pattern.trim()),
    missingIn: missing ? langs.map((tag) => loadCatalog(tag, messagesRoot).messages) : undefined,
    changed: changedRef ? new Set(changedKeys(en, englishAt(changedRef, messagesRoot))) : undefined,
  })
  const selection = [
    keysFlag && `--keys ${keysFlag}`,
    missing && '--missing',
    changedRef && `--changed-since ${changedRef}`,
  ]
    .filter(Boolean)
    .join(' ')
  return { keys, selection }
}

/** One `--stats` line. */
function statsLine(name: string, text: string): string {
  return `${name.padEnd(10)} ${String(text.length).padStart(7)} chars  ~${String(roughTokens(text)).padStart(6)} tokens`
}

function main(): void {
  const args = process.argv.slice(2)
  const messagesRoot = flagValue(args, '--messages-root')
  const langs = resolveLangs(flagValue(args, '--lang'), messagesRoot)
  const { keys, selection } = resolveKeys(args, langs, messagesRoot)
  if (keys.length === 0) {
    console.error('No keys selected: nothing to brief.')
    return
  }

  const { repoRoot, mainClone } = repoRoots()
  const brief = buildBrief({
    langs,
    keys,
    selection,
    messagesRoot,
    docsRoot: flagValue(args, '--docs-root'),
    pileRoot: join(mainClone, '_ignored', 'i18n'),
    repoRoot,
    excludeTargetValues: args.includes('--exclude-target-values'),
  })
  const text = renderBrief(brief)

  const out = flagValue(args, '--out')
  if (out) writeFileSync(out, text)
  else process.stdout.write(text)

  if (args.includes('--stats')) {
    for (const section of brief.sections) console.error(statsLine(section.name, section.text))
    console.error(statsLine('total', text))
  }
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  try {
    main()
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error))
    process.exit(EXIT_ERROR)
  }
}
