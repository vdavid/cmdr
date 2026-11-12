#!/usr/bin/env node
/**
 * Writes `src/lib/intl/__fixtures__/macos-copy-baseline.json`, the frozen macOS copy
 * that `src/lib/intl/macos-copy-unchanged.test.ts` holds the catalogs to.
 *
 * The keys are every `strings[].keys` entry of `docs/notes/windows-port/copy-proposal.json`
 * (the platform-copy rows) plus `KEYS_BEYOND_PROPOSAL`, resolved for every shipped
 * locale (`i18n-macos-copy-baseline-lib.ts`).
 *
 * Two modes, neither of which ever rewrites a frozen entry:
 *  - no flag: the first write. Refuses when the file exists.
 *  - `--add-missing`: freezes keys the file lacks, from today's catalogs, and leaves
 *    every existing entry byte-identical. Run it only before the added keys' catalog
 *    values change: a key frozen after its edit proves nothing.
 * ❌ Never re-run either mode to make the test pass, and never delete an entry to re-add it.
 *
 * Run from `apps/desktop/`: `node scripts/i18n-macos-copy-baseline.ts [--add-missing]`.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import {
  addMissingEntries,
  BASELINE_RELATIVE_PATH,
  baselineLocales,
  desktopPath,
  resolveLocales,
  type MacosCopyBaseline,
} from './i18n-macos-copy-baseline-lib.ts'

const PROPOSAL_PATH = desktopPath('../../docs/notes/windows-port/copy-proposal.json')

const README =
  'Frozen macOS copy: the resolved value a Mac reader of each locale saw for every platform-copy key, before any value held a platform block. Written by scripts/i18n-macos-copy-baseline.ts (first write, then --add-missing, which never rewrites an entry); held by src/lib/intl/macos-copy-unchanged.test.ts. Never edit or regenerate it to make that test pass.'

/**
 * Keys the platform-copy work turns into blocks that the proposal's `strings` never
 * listed (`docs/specs/platform-copy-plan.md` § "Copy resolution"): the overturned
 * `reviewedNoChange` rows (Windows "user folder", "recycle bin" keyword), the Quit
 * narration keys, the four permanent-delete siblings (`sourcesGone.continue` is
 * already a proposal row), the unscanned `{system_settings}` value, and the
 * sentences that say "this computer" on macOS and Linux and "this PC" on Windows, and
 * the switches Windows words "Turn on" where macOS and Linux say "Enable" (#1121).
 */
const KEYS_BEYOND_PROPOSAL: readonly string[] = [
  'askCmdr.tool.quit.doing',
  'commands.navGoHome.label',
  'errors.git.permissionDenied.suggestion',
  'errors.listing.serverRefused.explanation',
  'errors.mount.hostUnreachable',
  'errors.mount.mountMissing',
  'errors.mount.unsupportedProtocol',
  'errors.shareList.hostUnreachable',
  'errors.write.homeFolder.message.holdsHome',
  'errors.write.homeFolder.message.isHome',
  'errors.write.homeFolder.title',
  'errors.write.homeFolderMove.title',
  'errors.write.homeFolderRename.title',
  'fileExplorer.errorPane.goHome',
  'fileExplorer.unreachable.openHome',
  'fileOperations.delete.confirmDelete',
  'indexing.phase.home',
  'main.quit.countdown',
  'main.quit.countdownAria',
  'settings.developer.mcpEnabled.label',
  'settings.fileOperations.skipConfirmation.keywords',
  'settings.network.enabled.label',
  'settings.summary.smbNetworkShares',
  'stateRecovery.appStatus.title',
  'stateRecovery.credentials.body',
  'suggestedOps.action.delete',
  'suggestedOps.approve.delete',
]

interface ProposalRow {
  key: string
  keys?: string[]
}

function baselineKeys(): string[] {
  const proposal = JSON.parse(readFileSync(PROPOSAL_PATH, 'utf8')) as { strings: ProposalRow[] }
  return [...new Set([...proposal.strings.flatMap((row) => row.keys ?? [row.key]), ...KEYS_BEYOND_PROPOSAL])].sort()
}

function write(out: string, baseline: MacosCopyBaseline): void {
  writeFileSync(out, `${JSON.stringify(baseline, null, 2)}\n`)
}

function addMissing(out: string): void {
  if (!existsSync(out)) {
    console.error(`${BASELINE_RELATIVE_PATH} doesn't exist yet. Run without --add-missing for the first write.`)
    process.exit(1)
  }
  const keys = baselineKeys()
  const existing = JSON.parse(readFileSync(out, 'utf8')) as MacosCopyBaseline
  const { baseline, added } = addMissingEntries(existing, resolveLocales(keys), keys, README)
  write(out, baseline)
  console.log(`Added ${String(added)} values; every existing entry is unchanged.`)
}

function firstWrite(out: string): void {
  if (existsSync(out)) {
    console.error(`${BASELINE_RELATIVE_PATH} exists. The baseline is frozen; this mode never overwrites it.`)
    process.exit(1)
  }
  const keys = baselineKeys()
  const empty: MacosCopyBaseline = {
    readme: README,
    locales: Object.fromEntries(baselineLocales().map((locale) => [locale, {}])),
  }
  const { baseline, added } = addMissingEntries(empty, resolveLocales(keys), keys, README)
  write(out, baseline)
  console.log(
    `Wrote ${String(added)} values (${String(keys.length)} keys × ${String(Object.keys(baseline.locales).length)} locales).`,
  )
}

const out = desktopPath(BASELINE_RELATIVE_PATH)
if (process.argv.includes('--add-missing')) addMissing(out)
else firstWrite(out)
