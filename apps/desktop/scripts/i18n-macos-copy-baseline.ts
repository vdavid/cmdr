#!/usr/bin/env node
/**
 * One-shot: writes `src/lib/intl/__fixtures__/macos-copy-baseline.json`, the frozen
 * macOS copy that `src/lib/intl/macos-copy-unchanged.test.ts` holds the catalogs to.
 *
 * The keys are every `strings[].keys` entry of `docs/notes/windows-port/copy-proposal.json`
 * (the platform-copy rows), resolved for every shipped locale (`i18n-macos-copy-baseline-lib.ts`).
 * It ran once, on a tree where no catalog value held a platform block, and refuses to
 * overwrite the file: a baseline regenerated after a catalog edit proves nothing. ❌ Never
 * re-run it to make the test pass.
 *
 * Run from `apps/desktop/`: `node scripts/i18n-macos-copy-baseline.ts`.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import {
  BASELINE_RELATIVE_PATH,
  baselineLocales,
  desktopPath,
  resolveLocales,
  type MacosCopyBaseline,
} from './i18n-macos-copy-baseline-lib.ts'

const PROPOSAL_PATH = desktopPath('../../docs/notes/windows-port/copy-proposal.json')

interface ProposalRow {
  key: string
  keys?: string[]
}

function main(): void {
  const out = desktopPath(BASELINE_RELATIVE_PATH)
  if (existsSync(out)) {
    console.error(`${BASELINE_RELATIVE_PATH} exists. The baseline is frozen; this script never overwrites it.`)
    process.exit(1)
  }

  const proposal = JSON.parse(readFileSync(PROPOSAL_PATH, 'utf8')) as { strings: ProposalRow[] }
  const keys = [...new Set(proposal.strings.flatMap((row) => row.keys ?? [row.key]))].sort()

  const resolved = resolveLocales(keys)
  const locales: Record<string, Record<string, string>> = {}
  let count = 0
  for (const locale of baselineLocales()) {
    const values: Record<string, string> = {}
    for (const key of keys) {
      const entry = resolved[locale]?.[key]
      if (!entry) throw new Error(`${locale}: no catalog in the chain defines ${key}`)
      if (entry.value.startsWith('{platform, select,')) {
        throw new Error(`${locale} ${key} already holds a platform block; the baseline must predate them`)
      }
      values[key] = entry.value
      count++
    }
    locales[locale] = values
  }

  const baseline: MacosCopyBaseline = {
    readme:
      'Frozen macOS copy: the resolved value a Mac reader of each locale saw for every platform-copy key, before any value held a platform block. Written once by scripts/i18n-macos-copy-baseline.ts; held by src/lib/intl/macos-copy-unchanged.test.ts. Never edit or regenerate it to make that test pass.',
    locales,
  }
  writeFileSync(out, `${JSON.stringify(baseline, null, 2)}\n`)
  console.log(
    `Wrote ${String(count)} values (${String(keys.length)} keys × ${String(Object.keys(locales).length)} locales).`,
  )
}

main()
