/**
 * Tests for the platform-copy migration: the catalog rewrite plan
 * (`apply-platform-copy-lib.ts`), the scan and lint (`platform-copy-lint.ts`), and
 * the proving, searching, and restamping around them (`apply-platform-copy.ts`).
 *
 * Each case builds a throwaway `messages/` root (and `docs/i18n` root where a
 * rename is the point), plans or applies rows against it, and reads the result
 * back. The rule under test in most of them: macOS text never moves, and a
 * translation is carried into another branch only when it's a current
 * translation of that branch's unchanged English.
 */
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { blankLineKeys, planPlatformCopy, serializeCatalog } from './apply-platform-copy-lib.ts'
import type { CopyPlan } from './apply-platform-copy-lib.ts'
import {
  filesAssertingCopy,
  lintRows,
  literalRuns,
  restampEnglishOverlays,
  selectRows,
  verifyPlan,
  writePlan,
} from './apply-platform-copy.ts'
import { listLocales, loadCatalog, sourceHash } from './i18n-catalog-lib.ts'
import { COLLAPSED_PAIRS } from './i18n-macos-copy-baseline-lib.ts'
import { lintRow, scanEnglish } from './platform-copy-lint.ts'
import { isApplyRow, readCopyFinal } from './platform-copy-rows.ts'
import type { ApplyRow, CopyFinal, CopyRow } from './platform-copy-rows.ts'

let root: string
let docs: string

beforeEach(() => {
  const base = mkdtempSync(join(tmpdir(), 'cmdr-apply-copy-'))
  root = join(base, 'messages')
  docs = join(base, 'docs-i18n')
  mkdirSync(root, { recursive: true })
  mkdirSync(docs, { recursive: true })
})

afterEach(() => {
  rmSync(dirname(root), { recursive: true, force: true })
})

type Json = Record<string, unknown>

function writeCatalog(locale: string, file: string, content: Json): void {
  mkdirSync(join(root, locale), { recursive: true })
  writeFileSync(join(root, locale, file), `${JSON.stringify(content, null, 2)}\n`, 'utf8')
}

function readCatalog(locale: string, file: string): Json {
  return JSON.parse(readFileSync(join(root, locale, file), 'utf8')) as Json
}

/** A translated entry with its hash: `key`, its text, and the English it was made from. */
function entry(key: string, text: string, madeFrom: string): Json {
  return { [key]: text, [`@${key}`]: { sourceHash: sourceHash(madeFrom) } }
}

function applyRow(key: string, branches: ApplyRow['branches'], extra: Partial<CopyRow> = {}): ApplyRow {
  return {
    key,
    file: `${key.split('.')[0]}.json`,
    source: 'catalog',
    mechanism: 'select',
    status: 'apply',
    branches,
    sources: { windows: 'test', other: 'test' },
    ...extra,
  } as ApplyRow
}

function plan(rows: ApplyRow[]): CopyPlan {
  return planPlatformCopy(rows, { messagesRoot: root, docsRoot: docs, termAllowlistPath: null })
}

function apply(rows: ApplyRow[]): CopyPlan {
  const planned = plan(rows)
  writePlan(planned)
  return planned
}

function reportOf(planned: CopyPlan, locale: string): CopyPlan['locales'][number] {
  const report = planned.locales.find((candidate) => candidate.locale === locale)
  if (!report) throw new Error(`no report for ${locale}`)
  return report
}

const FINDER = 'Open in Finder'
const EXPLORER = 'Open in File Explorer'
const FILE_MANAGER = 'Open in your file manager'

describe('a row whose three branches differ', () => {
  beforeEach(() => {
    writeCatalog('en', 'area.json', { 'area.open': FINDER, '@area.open': { description: 'A button.' } })
    writeCatalog('de', 'area.json', entry('area.open', 'Im Finder öffnen', FINDER))
  })

  it('writes the English block and keeps its @key block', () => {
    apply([applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })])

    expect(readCatalog('en', 'area.json')).toEqual({
      'area.open': `{platform, select, macos {${FINDER}} windows {${EXPLORER}} other {${FILE_MANAGER}}}`,
      '@area.open': { description: 'A button.' },
    })
  })

  it('gives a locale the same block: its translation for macOS, English everywhere it has none', () => {
    const planned = apply([applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })])

    const de = readCatalog('de', 'area.json')
    expect(de['area.open']).toBe(
      `{platform, select, macos {Im Finder öffnen} windows {${EXPLORER}} other {${FILE_MANAGER}}}`,
    )
    expect(reportOf(planned, 'de')).toMatchObject({ carried: 1, english: 2 })
  })

  it('leaves the stored hash alone, so the stale check lists the key', () => {
    apply([applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })])

    expect(readCatalog('de', 'area.json')['@area.open']).toEqual({ sourceHash: sourceHash(FINDER) })
  })

  it('is a no-op the second time', () => {
    const rows = [applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })]
    apply(rows)

    const again = plan(rows)

    expect(again.alreadyApplied).toEqual(['area.open'])
    expect(again.catalogFiles.size).toBe(0)
  })

  it('passes the shape, parity, and coverage checks, and leaves the key sync nothing to do', () => {
    const planned = plan([applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })])

    expect(verifyPlan(planned, root, null)).toEqual([])
  })
})

describe('a row that changes one OS', () => {
  beforeEach(() => {
    writeCatalog('en', 'area.json', { 'area.quit': 'Quit now', 'area.hidden': 'Shows files hidden by macOS.' })
    writeCatalog('de', 'area.json', {
      ...entry('area.quit', 'Jetzt beenden', 'Quit now'),
      ...entry('area.hidden', 'Zeigt von macOS ausgeblendete Dateien.', 'Shows files hidden by macOS.'),
    })
  })

  it('a Windows-only change is a {windows, other} block whose other keeps the translation', () => {
    apply([
      applyRow('area.quit', { macos: 'Quit now', windows: 'Exit now', other: 'Quit now' }, { register: 'action' }),
    ])

    expect(readCatalog('en', 'area.json')['area.quit']).toBe('{platform, select, windows {Exit now} other {Quit now}}')
    expect(readCatalog('de', 'area.json')['area.quit']).toBe(
      '{platform, select, windows {Exit now} other {Jetzt beenden}}',
    )
  })

  it('a Linux-only change keeps macos and windows as two branches, both carrying the translation', () => {
    const macos = 'Shows files hidden by macOS.'
    const planned = apply([applyRow('area.hidden', { macos, windows: macos, other: 'Shows dotfiles.' })])

    expect(readCatalog('en', 'area.json')['area.hidden']).toBe(
      `{platform, select, macos {${macos}} windows {${macos}} other {Shows dotfiles.}}`,
    )
    const german = 'Zeigt von macOS ausgeblendete Dateien.'
    expect(readCatalog('de', 'area.json')['area.hidden']).toBe(
      `{platform, select, macos {${german}} windows {${german}} other {Shows dotfiles.}}`,
    )
    expect(reportOf(planned, 'de')).toMatchObject({ carried: 2, english: 1 })
  })

  it('a change that folds Windows into Linux leaves macOS its own branch', () => {
    apply([
      applyRow('area.quit', { macos: 'Quit now', windows: 'Close now', other: 'Close now' }, { register: 'action' }),
    ])

    expect(readCatalog('de', 'area.json')['area.quit']).toBe(
      '{platform, select, macos {Jetzt beenden} other {Close now}}',
    )
  })
})

describe('a raw row', () => {
  it('keeps {system_settings} in the macos branch only, in every locale', () => {
    const macos = 'Open {system_settings} > {privacy_and_security}.'
    writeCatalog('en', 'errors.json', { 'errors.git.denied': macos })
    writeCatalog(
      'de',
      'errors.json',
      entry('errors.git.denied', 'Öffne {system_settings} > {privacy_and_security}.', macos),
    )
    const row = applyRow('errors.git.denied', {
      macos,
      windows: 'Open Settings > Privacy & security.',
      other: 'Check the folder’s permissions.',
    })

    const planned = apply([row])

    expect(readCatalog('de', 'errors.json')['errors.git.denied']).toBe(
      '{platform, select, macos {Öffne {system_settings} > {privacy_and_security}.} windows {Open Settings > Privacy & security.} other {Check the folder’s permissions.}}',
    )
    expect(planned.refused).toEqual([])
  })
})

describe('a plural toast', () => {
  it('folds to {windows, other}, and a Russian other keeps few and many', () => {
    const english = '{count, plural, one {Moved # file to Trash} other {Moved # files to Trash}}'
    const russian =
      '{count, plural, one {# файл перемещён в Корзину} few {# файла перемещено в Корзину} many {# файлов перемещено в Корзину} other {# файла перемещено в Корзину}}'
    const windows = '{count, plural, one {Moved # file to the Recycle Bin} other {Moved # files to the Recycle Bin}}'
    writeCatalog('en', 'area.json', { 'area.toast': english })
    writeCatalog('ru', 'area.json', entry('area.toast', russian, english))
    const row = applyRow('area.toast', { macos: english, windows, other: english }, { register: 'sentence' })

    const planned = plan([row])
    writePlan(planned)

    expect(readCatalog('ru', 'area.json')['area.toast']).toBe(
      `{platform, select, windows {${windows}} other {${russian}}}`,
    )
    expect(verifyPlan(planned, root, null)).toEqual([])
  })
})

describe('a key that was already stale', () => {
  it('puts the translation in the macOS-read branch only, and lists the key', () => {
    const macos = 'Shows files hidden by macOS.'
    writeCatalog('en', 'area.json', { 'area.hidden': macos })
    writeCatalog('de', 'area.json', entry('area.hidden', 'Zeigt versteckte Dateien.', 'Shows hidden files.'))

    const planned = apply([applyRow('area.hidden', { macos, windows: macos, other: 'Shows dotfiles.' })])

    expect(readCatalog('de', 'area.json')['area.hidden']).toBe(
      `{platform, select, macos {Zeigt versteckte Dateien.} windows {${macos}} other {Shows dotfiles.}}`,
    )
    expect(reportOf(planned, 'de').preStale).toEqual(['area.hidden'])
    expect(readCatalog('de', 'area.json')['@area.hidden']).toEqual({ sourceHash: sourceHash('Shows hidden files.') })
  })
})

describe('a row resolved again after it was applied', () => {
  it('keeps every branch whose English did not change, and takes the new English for the one that did', () => {
    writeCatalog('en', 'area.json', { 'area.open': FINDER })
    writeCatalog('de', 'area.json', entry('area.open', 'Im Finder öffnen', FINDER))
    apply([applyRow('area.open', { macos: FINDER, windows: EXPLORER, other: FILE_MANAGER })])

    apply([applyRow('area.open', { macos: FINDER, windows: 'Show in File Explorer', other: FILE_MANAGER })])

    expect(readCatalog('de', 'area.json')['area.open']).toBe(
      `{platform, select, macos {Im Finder öffnen} windows {Show in File Explorer} other {${FILE_MANAGER}}}`,
    )
  })
})

const NUDGE_MAC = 'Open Cmdr > Onboarding… to review them.'
const NUDGE_OTHER = 'Open the command palette and run Onboarding… to review them.'
const nudgeRow = applyRow(
  'main.upgradeNudge',
  { macos: NUDGE_MAC, windows: NUDGE_OTHER, other: NUDGE_OTHER },
  { collapseFrom: { mac: 'main.upgradeNudge.mac', other: 'main.upgradeNudge.other' } },
)

describe('a pair collapse', () => {
  beforeEach(() => {
    writeCatalog('en', 'main.json', {
      'main.first': 'First',
      'main.upgradeNudge.mac': NUDGE_MAC,
      '@main.upgradeNudge.mac': { description: 'macOS toast.', screenshot: 'nudge.png' },
      'main.upgradeNudge.other': NUDGE_OTHER,
      '@main.upgradeNudge.other': { description: 'Other toast.' },
      'main.last': 'Last',
    })
    writeCatalog('de', 'main.json', {
      ...entry('main.first', 'Erstes', 'First'),
      ...entry('main.upgradeNudge.mac', 'Öffne Cmdr > Einführung…', NUDGE_MAC),
      ...entry('main.upgradeNudge.other', 'Öffne die Befehlspalette…', NUDGE_OTHER),
      ...entry('main.last', 'Letztes', 'Last'),
    })
    writeCatalog('fr', 'main.json', {
      ...entry('main.first', 'Premier', 'First'),
      ...entry('main.upgradeNudge.mac', 'Ouvrez Cmdr > Introduction…', NUDGE_MAC),
      ...entry('main.last', 'Dernier', 'Last'),
    })
  })

  it('puts the survivor where the .mac key stood, with the .mac key’s @key block, and removes both old keys', () => {
    apply([nudgeRow])

    const en = readCatalog('en', 'main.json')
    expect(Object.keys(en)).toEqual(['main.first', 'main.upgradeNudge', '@main.upgradeNudge', 'main.last'])
    expect(en['main.upgradeNudge']).toBe(`{platform, select, macos {${NUDGE_MAC}} other {${NUDGE_OTHER}}}`)
    expect(en['@main.upgradeNudge']).toEqual({ description: 'macOS toast.', screenshot: 'nudge.png' })
  })

  it('folds a locale’s two translations into one block, hashed as the .mac key was', () => {
    apply([nudgeRow])

    const de = readCatalog('de', 'main.json')
    expect(Object.keys(de)).toEqual([
      'main.first',
      '@main.first',
      'main.upgradeNudge',
      '@main.upgradeNudge',
      'main.last',
      '@main.last',
    ])
    expect(de['main.upgradeNudge']).toBe(
      '{platform, select, macos {Öffne Cmdr > Einführung…} other {Öffne die Befehlspalette…}}',
    )
    expect(de['@main.upgradeNudge']).toEqual({ sourceHash: sourceHash(NUDGE_MAC) })
  })

  it('writes English into the other branch of a locale that never had the .other key', () => {
    const planned = apply([nudgeRow])

    expect(readCatalog('fr', 'main.json')['main.upgradeNudge']).toBe(
      `{platform, select, macos {Ouvrez Cmdr > Introduction…} other {${NUDGE_OTHER}}}`,
    )
    expect(reportOf(planned, 'fr').missingOther).toEqual(['main.upgradeNudge'])
  })

  it('writes English where the .other English changed, and keeps the translation where it did not', () => {
    const reworded = {
      ...nudgeRow,
      branches: { ...nudgeRow.branches, windows: 'Run Onboarding… from the command palette.' },
    }

    apply([reworded])

    expect(readCatalog('de', 'main.json')['main.upgradeNudge']).toBe(
      '{platform, select, macos {Öffne Cmdr > Einführung…} windows {Run Onboarding… from the command palette.} other {Öffne die Befehlspalette…}}',
    )
  })

  it('leaves the key sync nothing to do: every locale keeps en’s key order', () => {
    writeCatalog('fr', 'main.json', {
      ...entry('main.first', 'Premier', 'First'),
      ...entry('main.upgradeNudge.mac', 'Ouvrez Cmdr > Introduction…', NUDGE_MAC),
      ...entry('main.upgradeNudge.other', 'Ouvrez la palette…', NUDGE_OTHER),
      ...entry('main.last', 'Dernier', 'Last'),
    })

    expect(verifyPlan(plan([nudgeRow]), root, null)).toEqual([])
  })

  it('refuses a pair whose survivor the frozen macOS copy doesn’t know', () => {
    const renamed = { ...nudgeRow, key: 'main.onboardingNudge' }

    const planned = plan([renamed])

    expect(planned.refused.map(({ reason }) => reason).join()).toMatch(/COLLAPSED_PAIRS/)
    expect(planned.catalogFiles.size).toBe(0)
  })
})

describe('a pair whose .other key feeds two survivors', () => {
  it('fills both survivors’ other branch from it and removes it once', () => {
    const keyring = 'Unlock your keyring and try again.'
    writeCatalog('en', 'ai.json', {
      'ai.secretError.keychainBody': 'Open Keychain Access.',
      'ai.secretError.keyringBody': keyring,
      'ai.secretError.removeKeychainBody': 'Delete it in Keychain Access.',
    })
    writeCatalog('de', 'ai.json', {
      ...entry('ai.secretError.keychainBody', 'Öffne die Schlüsselbundverwaltung.', 'Open Keychain Access.'),
      ...entry('ai.secretError.keyringBody', 'Entsperre deinen Schlüsselbund.', keyring),
      ...entry('ai.secretError.removeKeychainBody', 'Lösche ihn dort.', 'Delete it in Keychain Access.'),
    })
    const rows = [
      applyRow(
        'ai.secretError.body',
        { macos: 'Open Keychain Access.', windows: 'Open Credential Manager.', other: keyring },
        { collapseFrom: { mac: 'ai.secretError.keychainBody', other: 'ai.secretError.keyringBody' } },
      ),
      applyRow(
        'ai.secretError.removeBody',
        { macos: 'Delete it in Keychain Access.', windows: 'Remove it in Credential Manager.', other: keyring },
        { collapseFrom: { mac: 'ai.secretError.removeKeychainBody', other: 'ai.secretError.keyringBody' } },
      ),
    ]

    apply(rows)

    const de = readCatalog('de', 'ai.json')
    expect(Object.keys(de).filter((key) => !key.startsWith('@'))).toEqual([
      'ai.secretError.body',
      'ai.secretError.removeBody',
    ])
    expect(de['ai.secretError.body']).toBe(
      '{platform, select, macos {Öffne die Schlüsselbundverwaltung.} windows {Open Credential Manager.} other {Entsperre deinen Schlüsselbund.}}',
    )
    expect(de['ai.secretError.removeBody']).toBe(
      '{platform, select, macos {Lösche ihn dort.} windows {Remove it in Credential Manager.} other {Entsperre deinen Schlüsselbund.}}',
    )
  })
})

describe('a collapsed pair’s keys outside the catalogs', () => {
  beforeEach(() => {
    writeCatalog('en', 'main.json', { 'main.upgradeNudge.mac': NUDGE_MAC, 'main.upgradeNudge.other': NUDGE_OTHER })
    writeCatalog('de', 'main.json', {
      ...entry('main.upgradeNudge.mac', 'Öffne Cmdr > Einführung…', NUDGE_MAC),
      ...entry('main.upgradeNudge.other', 'Öffne die Befehlspalette…', NUDGE_OTHER),
    })
    mkdirSync(join(docs, 'de'), { recursive: true })
    mkdirSync(join(root, 'screenshots'), { recursive: true })
  })

  it('renames them in a termbase’s exceptions, merging the pair’s two reasons on the survivor', () => {
    writeFileSync(
      join(docs, 'de', 'terms.json'),
      JSON.stringify({
        onboarding: {
          chosen: 'Einführung',
          sources: 'catalog (main.upgradeNudge.mac)',
          exceptions: {
            'main.upgradeNudge.mac': 'Names the menu item.',
            'main.upgradeNudge.other': 'Same as `main.upgradeNudge.mac`.',
            'main.unrelated': 'Stays.',
          },
        },
      }),
    )

    apply([nudgeRow])

    const terms = JSON.parse(readFileSync(join(docs, 'de', 'terms.json'), 'utf8')) as Record<string, Json>
    expect(terms.onboarding.sources).toBe('catalog (main.upgradeNudge)')
    expect(terms.onboarding.exceptions).toEqual({
      'main.upgradeNudge': 'Names the menu item. Off macOS: Same as `main.upgradeNudge`.',
      'main.unrelated': 'Stays.',
    })
  })

  it('renames a citation in a guide, and never the head of a longer key', () => {
    const guide = join(docs, 'de', 'decisions.md')
    writeFileSync(
      guide,
      '## Menu paths (`main.upgradeNudge.mac`, `main.upgradeNudge.other`, `main.upgradeNudge.mac.label`)\n',
    )

    apply([nudgeRow])

    expect(readFileSync(guide, 'utf8')).toBe(
      '## Menu paths (`main.upgradeNudge`, `main.upgradeNudge`, `main.upgradeNudge.mac.label`)\n',
    )
  })

  it('moves the capture report’s surfaces to the survivor, once each, still sorted', () => {
    const report = join(root, 'screenshots', 'capture-report.json')
    writeFileSync(
      report,
      JSON.stringify({
        toast: {
          screenshot: 'nudge.png',
          keys: ['main.a', 'main.upgradeNudge.mac', 'main.upgradeNudge.other', 'main.z'],
        },
        other: { screenshot: 'other.png', keys: ['main.a'] },
      }),
    )

    const planned = apply([nudgeRow])

    expect(JSON.parse(readFileSync(report, 'utf8'))).toEqual({
      toast: { screenshot: 'nudge.png', keys: ['main.a', 'main.upgradeNudge', 'main.z'] },
      other: { screenshot: 'other.png', keys: ['main.a'] },
    })
    expect([...planned.renames.keys()]).toEqual(['main.upgradeNudge.mac', 'main.upgradeNudge.other'])
  })

  it('lists a guide that still names an old key by its last segment alone', () => {
    writeCatalog('en', 'ai.json', {
      'ai.secretError.keychainTitle': 'Keychain said no',
      'ai.secretError.keyringTitle': 'Keyring said no',
    })
    writeCatalog('de', 'ai.json', {
      ...entry('ai.secretError.keychainTitle', 'Schlüsselbund', 'Keychain said no'),
      ...entry('ai.secretError.keyringTitle', 'Schlüsselring', 'Keyring said no'),
    })
    const guide = join(docs, 'de', 'decisions.md')
    writeFileSync(
      guide,
      'Intro.\n\nKeychain (`ai.secretError.keychainTitle`/`.keyringTitle`), unlike `settings.x.keyringTitle`.\n',
    )
    const row = applyRow(
      'ai.secretError.title',
      { macos: 'Keychain said no', windows: 'Credential Manager said no', other: 'Keyring said no' },
      { collapseFrom: { mac: 'ai.secretError.keychainTitle', other: 'ai.secretError.keyringTitle' } },
    )

    const planned = apply([row])

    expect(readFileSync(guide, 'utf8')).toContain('(`ai.secretError.title`/`.keyringTitle`)')
    expect(planned.leftoverMentions).toEqual([`${guide}: keyringTitle`])
  })
})

describe('es-419, a translated overlay', () => {
  const english = 'Add to Finder favorites'
  const spain = 'Añadir a favoritos del Finder'
  const latam = 'Agregar a favoritos del Finder'
  const row = applyRow('area.add', { macos: english, windows: english, other: 'Add to bookmarks' })

  beforeEach(() => {
    writeCatalog('en', 'area.json', { 'area.add': english })
    writeCatalog('es', 'area.json', entry('area.add', spain, english))
  })

  it('carries its own fork into every branch whose English is unchanged, never the Spain-only form es carries', () => {
    writeCatalog('es-419', 'area.json', entry('area.add', latam, spain))

    apply([row])

    expect(readCatalog('es', 'area.json')['area.add']).toBe(
      `{platform, select, macos {${spain}} windows {${spain}} other {Add to bookmarks}}`,
    )
    const fork = readCatalog('es-419', 'area.json')
    expect(fork['area.add']).toBe(`{platform, select, macos {${latam}} windows {${latam}} other {Add to bookmarks}}`)
    expect(fork['@area.add']).toEqual({ sourceHash: sourceHash(spain) })
  })

  it('takes English, never es’s translation, for a branch its stale fork can’t vouch for', () => {
    writeCatalog('es-419', 'area.json', entry('area.add', latam, 'An older Spanish value'))

    apply([row])

    const value = readCatalog('es-419', 'area.json')['area.add'] as string
    expect(value).toBe(`{platform, select, macos {${latam}} windows {${english}} other {Add to bookmarks}}`)
    expect(value).not.toContain('Añadir')
  })

  it('leaves a key it doesn’t fork alone', () => {
    writeCatalog('es-419', 'area.json', entry('area.unrelated', 'Otra cosa', 'Otra cosa de España'))
    const before = readFileSync(join(root, 'es-419', 'area.json'), 'utf8')

    apply([row])

    expect(readFileSync(join(root, 'es-419', 'area.json'), 'utf8')).toBe(before)
  })

  describe('with a pair it forks one side of', () => {
    const esMac = 'Abre Cmdr > Introducción…'
    const esOther = 'Abre la paleta…'
    const latamOther = 'Abre la paleta y ejecuta Introducción…'

    beforeEach(() => {
      writeCatalog('en', 'main.json', { 'main.upgradeNudge.mac': NUDGE_MAC, 'main.upgradeNudge.other': NUDGE_OTHER })
      writeCatalog('es', 'main.json', {
        ...entry('main.upgradeNudge.mac', esMac, NUDGE_MAC),
        ...entry('main.upgradeNudge.other', esOther, NUDGE_OTHER),
      })
      writeCatalog('es-419', 'main.json', entry('main.upgradeNudge.other', latamOther, esOther))
    })

    it('reads the side it never forked from es, since that is what its readers saw', () => {
      apply([nudgeRow])

      const fork = readCatalog('es-419', 'main.json')
      expect(fork).toEqual({
        'main.upgradeNudge': `{platform, select, macos {${esMac}} other {${latamOther}}}`,
        '@main.upgradeNudge': { sourceHash: sourceHash(esOther) },
      })
    })

    it('drops the fork when its side’s English changed, since nothing of it is left', () => {
      const reworded = {
        ...nudgeRow,
        branches: { macos: NUDGE_MAC, windows: 'Run Onboarding….', other: 'Run Onboarding….' },
      }

      const planned = apply([reworded])

      expect(readCatalog('es-419', 'main.json')).toEqual({})
      expect(reportOf(planned, 'es-419').droppedForks).toEqual(['main.upgradeNudge'])
    })
  })
})

describe('en-GB and en-AU, the English overlays', () => {
  const row = applyRow(
    'area.trash',
    { macos: 'Move to Trash', windows: 'Delete', other: 'Move to Trash' },
    { register: 'label' },
  )

  beforeEach(() => {
    writeCatalog('en', 'area.json', { 'area.trash': 'Move to Trash', 'area.color': 'Color' })
    writeCatalog('de', 'area.json', {
      ...entry('area.trash', 'In den Papierkorb legen', 'Move to Trash'),
      ...entry('area.color', 'Farbe', 'Color'),
    })
    writeCatalog('en-GB', 'area.json', entry('area.trash', 'Move to Bin', 'Move to Trash'))
    writeCatalog('en-AU', 'area.json', entry('area.color', 'Colour', 'Color'))
  })

  it('keep the fork’s word in the branch macOS and Linux read, and take en’s new Windows branch', () => {
    const planned = apply([row])

    expect(readCatalog('en-GB', 'area.json')['area.trash']).toBe(
      '{platform, select, windows {Delete} other {Move to Bin}}',
    )
    expect(reportOf(planned, 'en-GB').spellingPass).toEqual(['area.trash'])
    expect(readCatalog('en-AU', 'area.json')).toEqual(entry('area.color', 'Colour', 'Color'))
  })

  it('are restamped alone: a full locale’s hash stays, so it stays on the translators’ list', () => {
    apply([row])
    const english = readCatalog('en', 'area.json')['area.trash'] as string

    const restamped = restampEnglishOverlays(['area.trash', 'area.color'], root)

    expect(restamped).toEqual({ 'en-GB': ['area.trash'], 'en-AU': [] })
    expect(readCatalog('en-GB', 'area.json')['@area.trash']).toEqual({ sourceHash: sourceHash(english) })
    expect(readCatalog('de', 'area.json')['@area.trash']).toEqual({ sourceHash: sourceHash('Move to Trash') })
  })
})

describe('macOS text', () => {
  beforeEach(() => {
    writeCatalog('en', 'area.json', { 'area.open': FINDER })
    writeCatalog('de', 'area.json', entry('area.open', 'Im Finder öffnen', FINDER))
  })

  it('refuses a row that would move it, and writes nothing for that row', () => {
    const planned = plan([applyRow('area.open', { macos: 'Show in Finder', windows: EXPLORER, other: FILE_MANAGER })])

    expect(planned.refused.map(({ reason }) => reason).join()).toMatch(/macOS text would move/)
    expect(planned.catalogFiles.size).toBe(0)
  })

  it('refuses a {windows, other} row whose other isn’t what macOS reads today', () => {
    const planned = plan([applyRow('area.open', { macos: 'Open it', windows: EXPLORER, other: 'Open it' })])

    expect(planned.refused.length).toBe(1)
  })

  it('moves for an accepted English change, while a translation of it stays put', () => {
    const old = 'Check that your system keyring is available, then try again. If it persists, restart the app.'
    const accepted = 'Check that your Keychain is available, then try again. If it persists, restart the app.'
    writeCatalog('en', 'ai.json', { 'ai.secretError.genericBody': old })
    writeCatalog('de', 'ai.json', entry('ai.secretError.genericBody', 'Prüfe deinen Schlüsselbund.', old))
    const row = applyRow(
      'ai.secretError.genericBody',
      { macos: accepted, windows: 'Check that Credential Manager is available.', other: old },
      { macosChanged: true },
    )

    const planned = apply([row])

    expect(planned.refused).toEqual([])
    expect(readCatalog('en', 'ai.json')['ai.secretError.genericBody']).toContain(`macos {${accepted}}`)
    expect(readCatalog('de', 'ai.json')['ai.secretError.genericBody']).toBe(
      '{platform, select, macos {Prüfe deinen Schlüsselbund.} windows {Check that Credential Manager is available.} other {Prüfe deinen Schlüsselbund.}}',
    )
  })

  it('refuses a row whose key English doesn’t hold', () => {
    const planned = plan([applyRow('area.missing', { macos: 'A', windows: 'B', other: 'C' })])

    expect(planned.refused.map(({ reason }) => reason).join()).toMatch(/lacks area\.missing/)
  })
})

describe('the lint', () => {
  const english = { 'area.tip': 'In Terminal, run `ls -la`. Check it (e.g. with chmod).' }
  const windowsRow = (windows: string, extra: Partial<CopyRow> = {}): ApplyRow =>
    applyRow('area.tip', { macos: english['area.tip'], windows, other: english['area.tip'] }, extra)

  it('blocks a Windows branch that says Terminal', () => {
    const hits = lintRows([windowsRow('In Terminal, run `dir`.')], english)

    expect(hits.map(({ branch, rule }) => `${branch}: ${rule}`)).toEqual([
      'windows: “Terminal” on Windows: “PowerShell”',
    ])
  })

  it('lets a hit through when the row gives the branch an exception with a reason', () => {
    const excepted = windowsRow('In Terminal, run `dir`.', {
      lintExceptions: [{ branch: 'windows', reason: 'Names the Windows Terminal app itself.' }],
    })

    expect(lintRows([excepted], english)).toEqual([])
  })

  it('holds a Windows or Linux branch to its own OS’s words, even one that still holds the macOS text', () => {
    const kept = applyRow('area.kept', { macos: 'Open Finder.', windows: 'Open Finder.', other: 'Open your files.' })
    expect(lintRow(kept, ['Open Finder.']).map(({ branch, rule }) => `${branch}: ${rule}`)).toEqual([
      'windows: macOS word “Finder”',
    ])

    const row = applyRow('area.x', {
      macos: 'Moved to Trash. Quit Cmdr.',
      windows: 'Moved to Trash. Quit Cmdr. Open {system_settings}. Press ⌘Q.',
      other: 'Moved to the Recycle Bin. Disconnect the share in Finder on this Mac.',
    })

    const rules = lintRow(row, []).map(({ branch, rule }) => `${branch}: ${rule}`)

    expect(rules.filter((rule) => rule.startsWith('windows'))).toHaveLength(4)
    expect(rules.filter((rule) => rule.startsWith('other'))).toHaveLength(4)
  })

  it('applies the house style to a branch the row wrote: an unchanged branch keeps its old “e.g.”, a new one can’t have it', () => {
    const kept = windowsRow('In PowerShell, run `dir`.')
    const fresh = applyRow('area.fresh', {
      macos: 'Check it.',
      windows: 'Just check it, e.g. in Properties.',
      other: 'Check it.',
    })

    expect(lintRows([kept], english)).toEqual([])
    expect(lintRows([windowsRow('In PowerShell, run `dir` (e.g. here).')], english).map(({ rule }) => rule)).toEqual([
      'style: “for example”, not “e.g.”',
    ])
    expect(lintRows([fresh], { 'area.fresh': 'Check it.' }).map(({ rule }) => rule)).toEqual([
      'style: no “just”, “simple”, or “easy”',
      'style: “for example”, not “e.g.”',
    ])
  })

  it('wants Microsoft’s words in new Windows text: select, turn on, drive, and never PC beside computer', () => {
    const row = applyRow('area.win', {
      macos: 'Fine.',
      windows: 'Click OK to enable it. Check the disk. Right-click works. This PC is a computer. A disk image is fine.',
      other: 'Click OK to enable it. Check the disk.',
    })

    const hits = lintRow(row, ['Fine.'])

    expect(hits.filter(({ branch }) => branch === 'windows')).toHaveLength(4)
    expect(hits.filter(({ branch }) => branch === 'other')).toEqual([])
  })

  it('wants a curly apostrophe in prose, and leaves a straight one inside a code span', () => {
    const row = applyRow('area.q', { macos: 'Fine.', windows: "Don't run `echo 'x'`.", other: "Run `echo 'x'`." })

    expect(lintRow(row, ['Fine.']).map(({ branch }) => branch)).toEqual(['windows'])
  })
})

describe('the scan', () => {
  it('lists an English value with macOS vocabulary that no row and no gate covers', () => {
    const file: CopyFinal = {
      readme: '',
      gated: [{ keys: 'ai.local.*', capability: 'localAi' }],
      rows: [
        applyRow('area.judged', { macos: 'Open Finder', windows: 'Open File Explorer', other: 'Open your files' }),
      ],
    }
    const english = {
      'area.judged': 'Open Finder',
      'ai.local.note': 'Runs on this Mac.',
      'area.unseen': 'Empty the Trash from the Dock, or open {system_settings}.',
      'area.plain': 'Rename the folder.',
      'area.selector': '{kind, select, trash {Removing} other {Copying}}',
    }

    expect(scanEnglish(english, file)).toEqual([{ key: 'area.unseen', terms: ['Trash', 'Dock', '{system_settings}'] }])
  })

  it('counts a pair’s old keys as judged', () => {
    const file: CopyFinal = { readme: '', gated: [], rows: [nudgeRow] }

    expect(scanEnglish({ 'main.upgradeNudge.mac': 'On this Mac, open Cmdr.' }, file)).toEqual([])
  })
})

describe('selectRows', () => {
  const file: CopyFinal = {
    readme: '',
    gated: [],
    rows: [
      nudgeRow,
      applyRow('errors.a', { macos: 'A', windows: 'B', other: 'C' }),
      applyRow('ai.b', { macos: 'A', windows: 'B', other: 'C' }),
    ],
  }
  const none = { catalogs: [], keys: [], pairs: false, all: false }

  it('selects by catalog, by key (a pair’s old key too), and by --pairs', () => {
    expect(selectRows(file, { ...none, catalogs: ['errors'] }).map((row) => row.key)).toEqual(['errors.a'])
    expect(selectRows(file, { ...none, keys: ['main.upgradeNudge.mac', 'ai.b'] }).map((row) => row.key)).toEqual([
      'main.upgradeNudge',
      'ai.b',
    ])
    expect(selectRows(file, { ...none, pairs: true }).map((row) => row.key)).toEqual(['main.upgradeNudge'])
    expect(selectRows(file, none)).toEqual([])
  })
})

describe('the test-file search', () => {
  it('finds a file asserting a literal run of the old or new copy, or naming the key', () => {
    const base = dirname(root)
    mkdirSync(join(base, 'src', 'lib'), { recursive: true })
    writeFileSync(join(base, 'src', 'lib', 'asserts-old.test.ts'), "expect(text).toBe('Moved 3 files to Trash')")
    writeFileSync(
      join(base, 'src', 'lib', 'asserts-new.test.ts'),
      "expect(text).toContain('1 file to the Recycle Bin')",
    )
    writeFileSync(join(base, 'src', 'lib', 'names-key.ts'), "tString('area.toast', { count })")
    writeFileSync(join(base, 'src', 'lib', 'unrelated.ts'), "const label = 'Moved'")
    const texts = new Map([
      [
        'area.toast',
        ['Moved {count} files to Trash', '{count, plural, one {Moved # file to the Recycle Bin} other {…}}'],
      ],
    ])

    expect(filesAssertingCopy(texts, ['area.toast'], base, ['src'])).toEqual([
      join('src', 'lib', 'asserts-new.test.ts'),
      join('src', 'lib', 'asserts-old.test.ts'),
      join('src', 'lib', 'names-key.ts'),
    ])
  })

  it('splits copy at placeholders, tags, and line breaks, and drops short runs', () => {
    expect(literalRuns('area.x', 'Moved {count} files to the Recycle Bin. <link>Undo</link>')).toEqual([
      'files to the Recycle Bin.',
    ])
    expect(literalRuns('errors.x', 'Here’s what to try:\n- Open {system_settings} and check')).toEqual([
      'Here’s what to try:',
    ])
  })
})

describe('serializeCatalog', () => {
  it('reproduces a hand-kept file with blank lines between sections byte for byte', () => {
    const text = '{\n  "a.one": "One",\n  "@a.one": {\n    "description": "First."\n  },\n\n  "a.two": "Two"\n}\n'

    expect(serializeCatalog(JSON.parse(text) as Json, blankLineKeys(text))).toBe(text)
  })

  it('reproduces every shipped catalog file byte for byte', () => {
    const messages = join(import.meta.dirname, '..', 'src', 'lib', 'intl', 'messages')
    const differing: string[] = []
    for (const locale of listLocales()) {
      for (const name of readdirSync(join(messages, locale))) {
        const text = readFileSync(join(messages, locale, name), 'utf8')
        if (serializeCatalog(JSON.parse(text) as Json, blankLineKeys(text)) !== text)
          differing.push(`${locale}/${name}`)
      }
    }
    expect(differing).toEqual([])
  })
})

describe('the committed copy-final.json', () => {
  const file = readCopyFinal()

  it('folds exactly the pairs the frozen macOS copy knows, into the same survivors', () => {
    const fromRows = file.rows
      .flatMap((row) => (row.collapseFrom ? [{ ...row.collapseFrom, survivor: row.key }] : []))
      .sort((a, b) => a.survivor.localeCompare(b.survivor))

    expect(fromRows).toEqual([...COLLAPSED_PAIRS].sort((a, b) => a.survivor.localeCompare(b.survivor)))
  })

  it('has no blocking lint hit on a resolved row', () => {
    expect(lintRows(file.rows.filter(isApplyRow), loadCatalog('en').messages)).toEqual([])
  })
})
