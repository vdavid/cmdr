/**
 * The frozen macOS copy baseline: what a Mac reader of each shipped locale sees
 * for every key the platform-copy work touches, captured once before any value
 * learned a `{platform, select, …}` block. `src/lib/intl/macos-copy-unchanged.test.ts`
 * holds every later catalog edit to it; `i18n-macos-copy-baseline.ts` wrote it.
 *
 * "What a reader sees" is the RESOLVED value: the locale's own catalog, then the
 * catalogs it inherits from (`inheritableAncestors`, the runtime's rule), then
 * `en`. So an overlay (`en-GB`) contributes every key, forked or inherited, and a
 * fork that vanishes or a base edit that leaks through both show up.
 */
import { join } from 'node:path'
import { IntlMessageFormat } from 'intl-messageformat'
import { inheritableAncestors } from '../src/lib/intl/locale-inheritance.ts'
import { pickPlatformBranch } from '../src/lib/intl/platform-select.ts'
import { BASE_LOCALE, GENERATED_LOCALES, isRawKey, listLocales, loadCatalog } from './i18n-catalog-lib.ts'

/** The baseline file, relative to `apps/desktop/`. */
export const BASELINE_RELATIVE_PATH = 'src/lib/intl/__fixtures__/macos-copy-baseline.json'

/** The baseline's on-disk shape: locale → key → resolved value. */
export interface MacosCopyBaseline {
  readme: string
  locales: Record<string, Record<string, string>>
}

/** One resolved value and the catalog it came from (`en` when nothing more specific defines it). */
export interface ResolvedValue {
  value: string
  from: string
}

/** The shipped locales the baseline covers: every catalog dir except generated ones (`en-XA`). */
export function baselineLocales(messagesRoot?: string): string[] {
  return listLocales(messagesRoot).filter((locale) => !GENERATED_LOCALES.has(locale))
}

/** The catalogs `locale` reads, most specific first, ending in `en`. */
export function resolutionChain(locale: string, available: readonly string[]): string[] {
  const chain = [locale, ...inheritableAncestors(locale, available), BASE_LOCALE]
  return chain.filter((tag, index) => chain.indexOf(tag) === index)
}

/**
 * Resolves `keys` for every baseline locale from the catalogs on disk.
 * @returns locale → key → resolved value; a key no catalog in the chain defines is absent
 */
export function resolveLocales(
  keys: readonly string[],
  messagesRoot?: string,
): Partial<Record<string, Partial<Record<string, ResolvedValue>>>> {
  const available = baselineLocales(messagesRoot)
  const catalogs = new Map(available.map((locale) => [locale, loadCatalog(locale, messagesRoot).messages]))
  const result: Partial<Record<string, Partial<Record<string, ResolvedValue>>>> = {}
  for (const locale of available) {
    const chain = resolutionChain(locale, available)
    const resolved: Partial<Record<string, ResolvedValue>> = {}
    for (const key of keys) {
      for (const tag of chain) {
        const value = catalogs.get(tag)?.[key]
        if (value !== undefined) {
          resolved[key] = { value, from: tag }
          break
        }
      }
    }
    result[locale] = resolved
  }
  return result
}

/** Whether a catalog value is already a `{platform, select, …}` block (the baseline must predate them). */
function isPlatformBlock(value: string): boolean {
  return value.startsWith('{platform, select,')
}

/**
 * Freezes `keys` that `baseline` lacks, from `resolved`, and leaves every entry it
 * already holds exactly as it was, even where the catalog has moved since. Each
 * locale comes back sorted by key, like the first write, so the file diff is pure
 * additions. Only the baseline's own locales are touched.
 * @returns the grown baseline and how many (locale, key) values it gained
 * @throws when a missing key resolves nowhere or already holds a platform block
 */
export function addMissingEntries(
  baseline: MacosCopyBaseline,
  resolved: Partial<Record<string, Partial<Record<string, ResolvedValue>>>>,
  keys: readonly string[],
  readme: string,
): { baseline: MacosCopyBaseline; added: number } {
  const locales: Record<string, Record<string, string>> = {}
  let added = 0
  for (const [locale, existing] of Object.entries(baseline.locales)) {
    const values: Record<string, string> = { ...existing }
    for (const key of keys) {
      if (key in existing) continue
      const entry = resolved[locale]?.[key]
      if (!entry) throw new Error(`${locale}: no catalog in the chain defines ${key}`)
      if (isPlatformBlock(entry.value)) {
        throw new Error(`${locale} ${key} already holds a platform block; the baseline must predate them`)
      }
      values[key] = entry.value
      added++
    }
    locales[locale] = Object.fromEntries(
      Object.keys(values)
        .sort()
        .map((key) => [key, values[key]]),
    )
  }
  return { baseline: { readme, locales }, added }
}

/** The absolute path of a file under `apps/desktop/`, resolved from this script's dir. */
export function desktopPath(relative: string): string {
  return join(import.meta.dirname, '..', relative)
}

/** The two macOS English changes David accepted (`docs/notes/windows-port/copy-decisions.md`), key → new text. */
export const ACCEPTED_ENGLISH_CHANGES: Readonly<Partial<Record<string, string>>> = {
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
export const COLLAPSED_PAIRS: readonly { mac: string; other: string; survivor: string }[] = [
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

/**
 * Keys split off a frozen key for one case it used to word: the new key carries that case's
 * per-OS words, so a Mac must keep reading exactly what the old key froze. The mounted network
 * share's eject words (#1117) are the six today: a share says "Eject" on macOS like a drive, and
 * "Disconnect" / "Unmount" on Windows / Linux. Checked per locale: the new key's macOS branch
 * renders what `from` froze.
 */
export const SPLIT_KEYS: readonly { from: string; to: string }[] = [
  { from: 'menu.volume.eject', to: 'menu.volume.detachShare' },
  { from: 'menu.volume.ejectBusy', to: 'menu.volume.detachShareBusy' },
  { from: 'fileExplorer.pane.ejectFailedToast', to: 'fileExplorer.pane.detachShareFailedToast' },
  { from: 'fileExplorer.navigation.ejectVolumeAriaLabel', to: 'fileExplorer.navigation.detachShareAriaLabel' },
  { from: 'fileExplorer.navigation.ejectingVolumeAriaLabel', to: 'fileExplorer.navigation.detachingShareAriaLabel' },
  { from: 'fileExplorer.navigation.ejectBusyTooltip', to: 'fileExplorer.navigation.detachShareBusyTooltip' },
]

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

/** One locale's `SPLIT_KEYS` whose new key is missing or reads other macOS text than its old key froze. */
function splitKeyDrift(
  locale: string,
  expectedValues: Readonly<Record<string, string>>,
  actualValues: Partial<Record<string, ResolvedValue>>,
): string[] {
  const drift: string[] = []
  for (const split of SPLIT_KEYS) {
    if (!(split.from in expectedValues)) continue
    const expected = expectedValues[split.from]
    const resolved = actualValues[split.to]
    if (!resolved) {
      drift.push(`${locale} ${split.to} (split off ${split.from}): missing`)
      continue
    }
    const actual = macosBranch(resolved.value)
    if (!rendersSame(split.to, expected, actual)) {
      drift.push(
        `${locale} ${split.to} (split off ${split.from}): ${JSON.stringify(expected)} → ${JSON.stringify(actual)}`,
      )
    }
  }
  return drift
}

/**
 * Every (locale, key) whose macOS text moved from `baseline`, as `locale key: expected → actual` lines.
 * @param messagesRoot the catalogs to read; the shipped ones by default
 */
export function macosCopyDrift(baseline: MacosCopyBaseline, messagesRoot?: string): string[] {
  const survivors = COLLAPSED_PAIRS.map((pair) => pair.survivor)
  const splits = SPLIT_KEYS.map((split) => split.to)
  const allKeys = [
    ...new Set([...Object.values(baseline.locales).flatMap((values) => Object.keys(values)), ...survivors, ...splits]),
  ]
  const current = resolveLocales(allKeys, messagesRoot)
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

    drift.push(...splitKeyDrift(locale, expectedValues, actualValues))

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
