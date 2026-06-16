#!/usr/bin/env node
/**
 * Generates `src/lib/intl/keys.gen.ts`: the `MessageKey` union of every catalog
 * key under `src/lib/intl/messages/en/`, and reports key drift:
 *  - keys referenced in code but ABSENT from the catalog → a build failure
 *    (exit 1), because `t('missing.key')` would silently fall back to the raw
 *    key string at runtime,
 *  - catalog keys NEVER referenced in code → a dead-key WARNING (exit 0), since
 *    a dead key is harmless but worth pruning. The scan only sees statically
 *    written keys, so a dynamically-built key reads as dead (don't blindly
 *    delete on this warning alone — see the `messages/` docs).
 *
 * Pure logic lives in `gen-message-keys-lib.js` (unit-tested); this CLI does the
 * filesystem I/O and the source scan. Run via `pnpm intl:keys` from the desktop
 * app dir, or through the `keys-fresh` check in the pipeline. Never hand-edit
 * `keys.gen.ts`.
 */

import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { dirname, join, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  collectCatalogKeys,
  extractUsedKeys,
  findCatalogKeyMentions,
  diffKeys,
  emitKeysModule,
} from './gen-message-keys-lib.js'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')
const messagesDir = join(desktopDir, 'src', 'lib', 'intl', 'messages', 'en')
const srcDir = join(desktopDir, 'src')
const outFile = join(desktopDir, 'src', 'lib', 'intl', 'keys.gen.ts')

/** Source extensions to scan for key references. */
const SCANNED_EXTS = new Set(['.ts', '.svelte'])
/** Directories to skip when walking for key usages. */
const SKIP_DIRS = new Set(['node_modules', 'target', '.svelte-kit'])
/**
 * Files to skip in the usage scan. The `$lib/intl` runtime files define and
 * document the accessors (`t('key')` appears in their JSDoc, not as a call
 * site), and test files deliberately reference unknown keys to exercise the
 * fallback. Counting either would produce phantom "missing key" failures.
 */
const SKIP_FILES = new Set(['messages.svelte.ts', 'Trans.svelte'])

/**
 * Whether a filename should be excluded from the usage scan.
 * @param {string} name
 * @returns {boolean}
 */
function isScanExcluded(name) {
  return SKIP_FILES.has(name) || name.endsWith('.test.ts') || name.endsWith('.test.js')
}

/** Reads and parses every `en/*.json` catalog into a filename → JSON map. */
function readCatalogFiles() {
  /** @type {Record<string, Record<string, unknown>>} */
  const files = {}
  for (const file of readdirSync(messagesDir)) {
    if (!file.endsWith('.json')) continue
    files[file] = JSON.parse(readFileSync(join(messagesDir, file), 'utf8'))
  }
  return files
}

/**
 * Recursively collects, across the frontend source tree: keys referenced via
 * `t()`/`tString()`/`getMessage()`/`<Trans>`/`*Key:` props (into `acc`, drives
 * missing detection) and catalog keys whose literal appears anywhere (into
 * `mentioned`, suppresses false dead warnings for indirection). The generated
 * `keys.gen.ts` is skipped (it lists the catalog keys, not usages).
 * @param {string} dir
 * @param {Set<string>} acc keys from direct references
 * @param {Set<string>} mentioned catalog keys whose literal appears in source
 * @param {string[]} catalogKeys the catalog key list to scan for mentions
 */
function scanUsedKeys(dir, acc, mentioned, catalogKeys) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) scanUsedKeys(join(dir, entry.name), acc, mentioned, catalogKeys)
      continue
    }
    if (!SCANNED_EXTS.has(extname(entry.name))) continue
    if (entry.name === 'keys.gen.ts' || isScanExcluded(entry.name)) continue
    const source = readFileSync(join(dir, entry.name), 'utf8')
    for (const key of extractUsedKeys(source)) acc.add(key)
    for (const key of findCatalogKeyMentions(source, catalogKeys)) mentioned.add(key)
  }
}

const catalogKeys = collectCatalogKeys(readCatalogFiles())
writeFileSync(outFile, emitKeysModule(catalogKeys))

/** @type {Set<string>} */
const usedKeys = new Set()
/** @type {Set<string>} Catalog keys whose literal text appears in source, for dead-key suppression. */
const literalKeys = new Set()
scanUsedKeys(srcDir, usedKeys, literalKeys, catalogKeys)
const { missing, dead } = diffKeys({ catalogKeys, usedKeys, literalKeys })

console.log(`Wrote ${String(catalogKeys.length)} message keys to keys.gen.ts`)

if (dead.length > 0) {
  console.warn(
    `\nWarning: ${String(dead.length)} catalog key(s) not referenced in code (dead keys).\n` +
      `These may be dynamically referenced; verify before deleting:\n` +
      dead.map((k) => `  - ${k}`).join('\n'),
  )
}

if (missing.length > 0) {
  console.error(
    `\nFailure: ${String(missing.length)} key(s) referenced in code but missing from the catalog:\n` +
      missing.map((k) => `  - ${k}`).join('\n') +
      `\nAdd them to a \`messages/en/*.json\` file (the key prefix maps to the filename).`,
  )
  process.exit(1)
}
