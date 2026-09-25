/**
 * What changed in English since a translation was made, for the brief's keys
 * section: a stale key's `@key.sourceHash` names the English it was translated
 * from, and git history still holds that English. Without this a translator
 * re-derives it with `git log -p` (18k–50k chars of tool output per agent, when
 * measured).
 *
 * Deterministic: the answer depends only on the catalogs and the commits that
 * touched them. Fast: one `git log` per English catalog file in play, then one
 * `git show` per commit, newest first, until every stamp in that file is found
 * (or `MAX_COMMITS` pass). Outside a git repo, or for a stamp no commit carries,
 * it says nothing.
 */

import { execFileSync } from 'node:child_process'
import { join } from 'node:path'
import {
  BASE_LOCALE,
  isMetadataKey,
  loadCatalog,
  readLocaleFiles,
  resolveMessagesRoot,
  sourceHash,
} from './i18n-catalog-lib.ts'

/** How far back one catalog file's history is searched. */
const MAX_COMMITS = 400

/** Batch key → target locale → the English that locale's translation was made from. */
export type PreviousEnglish = Record<string, Record<string, string>>

function git(cwd: string, args: string[]): string | undefined {
  try {
    return execFileSync('git', ['-C', cwd, ...args], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      maxBuffer: 64 * 1024 * 1024,
    })
  } catch {
    return undefined
  }
}

/**
 * For each batch key a target locale translated from DIFFERENT English than
 * today's, the English it was translated from. Keys with no stamp, a current
 * stamp, or a stamp no commit carries are left out.
 */
export function previousEnglish({
  keys,
  langs,
  messagesRoot,
}: {
  keys: readonly string[]
  langs: readonly string[]
  messagesRoot?: string
}): PreviousEnglish {
  const en = loadCatalog(BASE_LOCALE, messagesRoot).messages
  // file → key → the stamps still to find
  const wanted = new Map<string, Map<string, Set<string>>>()
  const stamps: { key: string; lang: string; hash: string }[] = []
  const fileOf = englishFileOf(messagesRoot)
  for (const lang of langs) {
    const metadata = loadCatalog(lang, messagesRoot).metadata
    for (const key of keys) {
      const hash = key in metadata ? metadata[key].sourceHash : undefined
      if (typeof hash !== 'string' || !(key in en) || hash === sourceHash(en[key])) continue
      const file = fileOf.get(key)
      if (file === undefined) continue
      stamps.push({ key, lang, hash })
      const byKey = wanted.get(file) ?? new Map<string, Set<string>>()
      byKey.set(key, (byKey.get(key) ?? new Set()).add(hash))
      wanted.set(file, byKey)
    }
  }
  const found = new Map<string, string>() // `${key}\0${hash}` → English
  const enDir = join(resolveMessagesRoot(messagesRoot), BASE_LOCALE)
  for (const [file, byKey] of wanted) searchFile(enDir, file, byKey, found)
  const out: PreviousEnglish = {}
  for (const { key, lang, hash } of stamps) {
    const english = found.get(`${key}\0${hash}`)
    if (english !== undefined) (out[key] ??= {})[lang] = english
  }
  return out
}

/** Which English catalog file holds each key today. */
function englishFileOf(messagesRoot?: string): Map<string, string> {
  const map = new Map<string, string>()
  for (const [file, entries] of Object.entries(readLocaleFiles(BASE_LOCALE, messagesRoot))) {
    for (const key of Object.keys(entries)) if (!isMetadataKey(key)) map.set(key, file)
  }
  return map
}

/** Walks one English file's history newest first, recording each wanted stamp's English. */
function searchFile(enDir: string, file: string, byKey: Map<string, Set<string>>, found: Map<string, string>): void {
  const log = git(enDir, ['log', `--max-count=${String(MAX_COMMITS)}`, '--format=%H', '--', file])
  const path = git(enDir, ['ls-files', '--full-name', '--', file])?.trim()
  if (log === undefined || !path) return
  let pending = [...byKey.values()].reduce((sum, hashes) => sum + hashes.size, 0)
  const commits = log.split('\n').filter(Boolean)
  // A chunk of commits per `git cat-file --batch` call: few processes, and an early stop once every stamp is found.
  for (let start = 0; start < commits.length && pending > 0; start += BATCH) {
    for (const text of blobs(enDir, commits.slice(start, start + BATCH), path)) {
      let entries: Record<string, unknown>
      try {
        entries = JSON.parse(text) as Record<string, unknown>
      } catch {
        continue
      }
      for (const [key, hashes] of byKey) {
        const value = entries[key]
        if (typeof value !== 'string') continue
        const hash = sourceHash(value)
        if (hashes.has(hash) && !found.has(`${key}\0${hash}`)) {
          found.set(`${key}\0${hash}`, value)
          pending--
        }
      }
      if (pending === 0) return
    }
  }
}

/** Commits read per `git cat-file` call. */
const BATCH = 20

/** The file at each commit, in commit order, through one `git cat-file --batch`; a missing one is skipped. */
function blobs(cwd: string, commits: readonly string[], path: string): string[] {
  let raw: Buffer
  try {
    raw = execFileSync('git', ['-C', cwd, 'cat-file', '--batch'], {
      input: commits.map((commit) => `${commit}:${path}`).join('\n') + '\n',
      stdio: ['pipe', 'pipe', 'ignore'],
      maxBuffer: 256 * 1024 * 1024,
    })
  } catch {
    return []
  }
  const out: string[] = []
  let at = 0
  while (at < raw.length) {
    const eol = raw.indexOf(10, at)
    if (eol === -1) break
    const header = raw.toString('utf8', at, eol).split(' ')
    at = eol + 1
    if (header[1] !== 'blob') continue // `<name> missing`
    const size = Number(header[2])
    out.push(raw.toString('utf8', at, at + size))
    at += size + 1
  }
  return out
}
