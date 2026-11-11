/**
 * macOS copy doesn't move: every shipped locale's macOS rendering of every
 * platform-copy key stays what `__fixtures__/macos-copy-baseline.json` froze.
 *
 * The baseline holds the RESOLVED value a Mac reader saw (locale, then the
 * catalogs it inherits from, then `en`), read from disk for all 17 catalogs, since
 * the runtime only loads `en` and one chain outside `test-setup.ts`. Its keys are
 * the platform-copy rows of `docs/notes/windows-port/copy-proposal.json`; its
 * writer is `scripts/i18n-macos-copy-baseline.ts`, which refuses to overwrite it.
 * ❌ Never edit or regenerate the baseline to make this pass: a red here means a
 * catalog edit changed what a Mac shows.
 *
 * Allowed differences, and only these:
 *  - `ACCEPTED_ENGLISH_CHANGES`: two English values David changed on purpose. Only
 *    where the value resolves from `en`; a translation of either stays put.
 *  - `COLLAPSED_PAIRS`: a `.mac`/`.other` key pair folded into one block-valued
 *    survivor key. Once the `.mac` key is gone, the survivor's macOS branch must
 *    equal the old `.mac` baseline value, and the two old keys are no longer read.
 *
 * ICU values compare ASTs (same rendering, whatever the spelling); the raw families
 * (`isRawKey`) compare text.
 */
import { readFileSync } from 'node:fs'
import { IntlMessageFormat } from 'intl-messageformat'
import { describe, expect, it } from 'vitest'

import { isRawKey } from '../../../scripts/i18n-catalog-lib.ts'
import { pickPlatformBranch } from './platform-select'
import {
  BASELINE_RELATIVE_PATH,
  desktopPath,
  resolveLocales,
  type MacosCopyBaseline,
} from '../../../scripts/i18n-macos-copy-baseline-lib.ts'

/** The two macOS English changes David accepted (`docs/notes/windows-port/copy-decisions.md`), key → new text. */
const ACCEPTED_ENGLISH_CHANGES: Readonly<Partial<Record<string, string>>> = {
  'ai.secretError.genericBody':
    'Check that your Keychain is available, then try again. If it persists, restart the app.',
  'goToPath.dialog.inputPlaceholder': 'Type or paste a path, for example ~/Documents',
}

/**
 * The 14 `.mac`/`.other` pairs and the key each folds into. The survivor names
 * the UI concept without the OS suffix; the two Show-in-Finder pairs survive as
 * `showInFileManager` (lead ruling). Renaming a survivor when the pairs collapse updates this map,
 * never the baseline.
 */
const COLLAPSED_PAIRS: readonly { mac: string; other: string; survivor: string }[] = [
  {
    mac: 'commands.fileShowInFinder.mac.label',
    other: 'commands.fileShowInFinder.other.label',
    survivor: 'commands.fileShowInFileManager.label',
  },
  {
    mac: 'commands.fileGetInfo.mac.label',
    other: 'commands.fileGetInfo.other.label',
    survivor: 'commands.fileGetInfo.label',
  },
  {
    mac: 'commands.fileQuickLook.mac.label',
    other: 'commands.fileQuickLook.other.label',
    survivor: 'commands.fileQuickLook.label',
  },
  {
    mac: 'errors.write.fileLocked.suggestion.mac',
    other: 'errors.write.fileLocked.suggestion.other',
    survivor: 'errors.write.fileLocked.suggestion',
  },
  {
    mac: 'errors.write.permissionDenied.suggestion.deleteMac',
    other: 'errors.write.permissionDenied.suggestion.deleteOther',
    survivor: 'errors.write.permissionDenied.suggestion.delete',
  },
  {
    mac: 'errors.write.permissionDenied.suggestion.needsAdminMac',
    other: 'errors.write.permissionDenied.suggestion.needsAdminOther',
    survivor: 'errors.write.permissionDenied.suggestion.needsAdmin',
  },
  {
    mac: 'errors.write.permissionDenied.suggestion.systemProtectedMac',
    other: 'errors.write.permissionDenied.suggestion.systemProtectedOther',
    survivor: 'errors.write.permissionDenied.suggestion.systemProtected',
  },
  {
    mac: 'settings.appearance.appColorHintLinkMac',
    other: 'settings.appearance.appColorHintLinkOther',
    survivor: 'settings.appearance.appColorHintLink',
  },
  { mac: 'ai.secretError.keychainTitle', other: 'ai.secretError.keyringTitle', survivor: 'ai.secretError.title' },
  { mac: 'ai.secretError.keychainBody', other: 'ai.secretError.keyringBody', survivor: 'ai.secretError.body' },
  {
    mac: 'ai.secretError.removeKeychainBody',
    other: 'ai.secretError.keyringBody',
    survivor: 'ai.secretError.removeBody',
  },
  { mac: 'mtp.connectedToast.bodyMac', other: 'mtp.connectedToast.bodyOther', survivor: 'mtp.connectedToast.body' },
  { mac: 'main.upgradeNudge.mac', other: 'main.upgradeNudge.other', survivor: 'main.upgradeNudge' },
  { mac: 'menu.file.showInFinder', other: 'menu.file.showInFileManager', survivor: 'menu.file.showInFileManager' },
]

const baseline = JSON.parse(readFileSync(desktopPath(BASELINE_RELATIVE_PATH), 'utf8')) as MacosCopyBaseline

/**
 * The text macOS reads from a catalog value: a block's `macos` branch, or `other`
 * where the block has none. A value that starts like a block but doesn't split
 * comes back whole, so it shows as drift.
 */
function macosBranch(value: string): string {
  return pickPlatformBranch(value, 'macos')
}

/** Whether two values render the same: ICU by AST, raw by text. */
function rendersSame(key: string, expected: string, actual: string): boolean {
  if (expected === actual) return true
  if (isRawKey(key)) return false
  try {
    const ast = (value: string): string => JSON.stringify(new IntlMessageFormat(value, 'en').getAst())
    return ast(expected) === ast(actual)
  } catch {
    return false
  }
}

/** Every (locale, key) whose macOS text moved, as `locale key: expected → actual` lines. */
function macosCopyDrift(): string[] {
  const survivors = COLLAPSED_PAIRS.map((pair) => pair.survivor)
  const allKeys = [
    ...new Set([...Object.values(baseline.locales).flatMap((values) => Object.keys(values)), ...survivors]),
  ]
  const current = resolveLocales(allKeys)
  const drift: string[] = []

  for (const [locale, expectedValues] of Object.entries(baseline.locales)) {
    const actualValues = current[locale]
    if (!actualValues) {
      drift.push(`${locale}: the locale no longer ships`)
      continue
    }
    const retired = new Set<string>()
    for (const pair of COLLAPSED_PAIRS) {
      if (actualValues[pair.mac]) continue
      retired.add(pair.mac).add(pair.other)
      const survivor = actualValues[pair.survivor]
      if (!survivor) {
        drift.push(`${locale} ${pair.mac}: gone, and its survivor ${pair.survivor} doesn't exist`)
        continue
      }
      const actual = macosBranch(survivor.value)
      if (!rendersSame(pair.survivor, expectedValues[pair.mac], actual)) {
        drift.push(
          `${locale} ${pair.survivor} (was ${pair.mac}): ${JSON.stringify(expectedValues[pair.mac])} → ${JSON.stringify(actual)}`,
        )
      }
    }

    for (const [key, expected] of Object.entries(expectedValues)) {
      if (retired.has(key)) continue
      const resolved = actualValues[key]
      if (!resolved) {
        drift.push(`${locale} ${key}: gone`)
        continue
      }
      const actual = macosBranch(resolved.value)
      if (rendersSame(key, expected, actual)) continue
      const accepted = ACCEPTED_ENGLISH_CHANGES[key]
      if (accepted !== undefined && resolved.from === 'en' && actual === accepted) continue
      drift.push(`${locale} ${key}: ${JSON.stringify(expected)} → ${JSON.stringify(actual)}`)
    }
  }
  return drift
}

describe('macOS copy stays byte-identical', () => {
  it('covers every shipped locale and the platform-copy keys', () => {
    expect(Object.keys(baseline.locales).length).toBe(17)
    expect(Object.keys(baseline.locales.en).length).toBeGreaterThan(400)
  })

  it('every locale renders the frozen macOS text for every key', () => {
    expect(macosCopyDrift()).toEqual([])
  })
})
