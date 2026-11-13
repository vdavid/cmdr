/**
 * Tests for `pnpm i18n:apply` (`i18n-apply-lib.ts`): the translator's one write
 * path into a locale catalog. Each case builds a throwaway `messages/` root.
 *
 * The load-bearing promises: a batch with ONE bad value writes nothing; a written
 * key carries its source's current hash; untouched lines stay byte for byte; and
 * a per-OS entry that leaves `macos` out keeps the catalog's macOS text exactly.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parseValuesFile, planApply, uncoveredKeys, writePlan } from './i18n-apply-lib.ts'
import { sourceHash } from './i18n-catalog-lib.ts'
import { parseApplyArgs } from './i18n-apply.ts'
import { latestWorkDir, newWorkDir } from './i18n-repo-lib.ts'

let root: string

const write = (tag: string, area: string, obj: Record<string, unknown>) => {
  mkdirSync(join(root, tag), { recursive: true })
  writeFileSync(join(root, tag, area), JSON.stringify(obj, null, 2) + '\n', 'utf8')
}
const raw = (tag: string, area = 'files.json') => readFileSync(join(root, tag, area), 'utf8')
const read = (tag: string, area = 'files.json') => JSON.parse(raw(tag, area)) as Record<string, unknown>

const PER_OS = '{platform, select, macos {Move to Trash} windows {Delete} other {Move to Trash}}'

/** Plans and, when clean, writes. */
function apply(tag: string, values: Record<string, unknown>) {
  const plan = planApply({ tag, values: parseValuesFile(values), messagesRoot: root })
  if (plan.refused.length === 0) writePlan(plan, root)
  return plan
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'cmdr-i18n-apply-'))
  write('en', 'files.json', {
    'files.copy': 'Copy {count} files',
    '@files.copy': { description: 'A button.' },
    'files.trash': PER_OS,
    'files.title': 'Files',
  })
  write('de', 'files.json', {
    'files.copy': 'Copy {count} files',
    '@files.copy': { sourceHash: sourceHash('Copy {count} files') },
    'files.trash': '{platform, select, macos {In den Papierkorb legen} windows {Delete} other {Move to Trash}}',
    '@files.trash': { sourceHash: sourceHash('In den Papierkorb') },
    'files.title': 'Dateien',
    '@files.title': { sourceHash: sourceHash('Files'), reviewed: true },
  })
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

describe('planApply on a full translation', () => {
  it('writes the value, stamps the current English, and leaves untouched lines byte for byte', () => {
    const before = raw('de')
    const plan = apply('de', { 'files.copy': '{count} Dateien kopieren' })

    expect(plan.refused).toEqual([])
    const after = raw('de')
    expect(read('de')['files.copy']).toBe('{count} Dateien kopieren')
    expect(read('de')['@files.copy']).toEqual({ sourceHash: sourceHash('Copy {count} files') })
    const changed = after.split('\n').filter((line, i) => line !== before.split('\n')[i])
    expect(changed).toEqual(['  "files.copy": "{count} Dateien kopieren",'])
  })

  it('refuses the whole batch when one value breaks a mechanic, and writes nothing', () => {
    const before = raw('de')
    const plan = apply('de', { 'files.copy': '{count} Dateien kopieren', 'files.title': 'Dateien {extra}' })

    expect(plan.refused.map((r) => r.key)).toEqual(['files.title'])
    expect(plan.refused[0].reason).toContain('placeholders')
    expect(raw('de')).toBe(before)
  })

  it('refuses invalid ICU, a broken per-OS block, a key English lacks, and one the locale lacks', () => {
    write('en', 'more.json', { 'more.new': 'New' })
    const plan = planApply({
      tag: 'de',
      messagesRoot: root,
      values: parseValuesFile({
        'files.copy': '{count Dateien',
        'files.trash': 'Papierkorb',
        'files.nope': 'x',
        'more.new': 'Neu',
      }),
    })
    const reasons = Object.fromEntries(plan.refused.map((r) => [r.key, r.reason]))
    expect(reasons['files.copy']).toContain('invalid ICU')
    expect(reasons['files.trash']).toContain('a plain value, but its source is a block')
    expect(reasons['files.nope']).toContain('not an English key')
    expect(reasons['more.new']).toContain('sync-locale-keys.ts de')
  })

  it('assembles a per-OS entry, keeping each branch it leaves out exactly as the catalog has it', () => {
    const plan = apply('de', { 'files.trash': { windows: 'Löschen', other: 'In den Papierkorb legen' } })

    expect(plan.refused).toEqual([])
    expect(read('de')['files.trash']).toBe(
      '{platform, select, macos {In den Papierkorb legen} windows {Löschen} other {In den Papierkorb legen}}',
    )
    expect(plan.notes).toContain('files.trash: kept from the catalog: macos')
    // The English moved under it, so the hash moves and the key leaves the stale list.
    expect(read('de')['@files.trash']).toEqual({ sourceHash: sourceHash(PER_OS) })
  })

  it('refuses a value that moves the macOS text the baseline froze', () => {
    const baseline = { readme: '', locales: { de: { 'files.trash': 'In den Papierkorb legen' } } }
    const moved = planApply({
      tag: 'de',
      messagesRoot: root,
      baseline,
      values: parseValuesFile({ 'files.trash': { macos: 'Wegwerfen', windows: 'Löschen' } }),
    })
    expect(moved.refused[0].reason).toContain('macosChangeReason')

    const kept = planApply({
      tag: 'de',
      messagesRoot: root,
      baseline,
      values: parseValuesFile({ 'files.trash': { windows: 'Löschen', other: 'In den Papierkorb legen' } }),
    })
    expect(kept.refused).toEqual([])
  })

  it('records a reasoned change to frozen macOS text in the locale’s own accepted file, and drops it when reverted', () => {
    const baseline = { readme: '', locales: { de: { 'files.trash': 'In den Papierkorb legen' } } }
    const acceptedDir = join(root, 'accepted')
    const run = (values: Record<string, unknown>) => {
      const plan = planApply({ tag: 'de', messagesRoot: root, baseline, acceptedDir, values: parseValuesFile(values) })
      if (plan.refused.length === 0) writePlan(plan, root)
      return plan
    }
    const plan = run({
      'files.trash': { macos: 'In den Papierkorb werfen', windows: 'Löschen', other: 'In den Papierkorb legen' },
      '@files.trash': { macosChangeReason: 'legen read as placing, not discarding' },
    })
    expect(plan.refused).toEqual([])
    const accepted = () => JSON.parse(readFileSync(join(acceptedDir, 'de.json'), 'utf8')) as Record<string, unknown>
    expect(accepted()).toEqual({
      'files.trash': { text: 'In den Papierkorb werfen', reason: 'legen read as placing, not discarding' },
    })
    // Applying the accepted value again needs no second reason.
    expect(
      planApply({
        tag: 'de',
        messagesRoot: root,
        baseline,
        acceptedDir,
        values: parseValuesFile({ 'files.trash': { windows: 'Entfernen' } }),
      }).refused,
    ).toEqual([])
    // Back to the frozen text: the entry would now be a false requirement, so it goes.
    run({ 'files.trash': { macos: 'In den Papierkorb legen' } })
    expect(existsSync(join(acceptedDir, 'de.json'))).toBe(false)
  })

  it('refuses a macosChangeReason on a value whose macOS text doesn’t change', () => {
    const baseline = { readme: '', locales: { de: { 'files.trash': 'In den Papierkorb legen' } } }
    const plan = planApply({
      tag: 'de',
      messagesRoot: root,
      baseline,
      acceptedDir: join(root, 'accepted'),
      values: parseValuesFile({ 'files.trash': { windows: 'Löschen' }, '@files.trash': { macosChangeReason: 'x' } }),
    })
    expect(plan.refused[0].reason).toContain('macOS text doesn')
  })

  it('keeps a translation as is: stamps it against the new English without retyping it', () => {
    write('en', 'files.json', {
      'files.copy': 'Copy {count} files',
      'files.trash': PER_OS,
      'files.title': 'Your files',
    })
    const plan = apply('de', { 'files.title': { keep: true } })
    expect(plan.refused).toEqual([])
    expect(read('de')['files.title']).toBe('Dateien')
    expect(read('de')['@files.title']).toEqual({ sourceHash: sourceHash('Your files') })
  })

  it('refuses to keep a key with a branch still in English, or one an overlay doesn’t fork', () => {
    const english = planApply({
      tag: 'de',
      messagesRoot: root,
      values: parseValuesFile({ 'files.trash': { keep: true } }),
    })
    expect(english.refused[0].reason).toContain('still English')
    write('en-GB', 'files.json', {})
    const overlay = planApply({
      tag: 'en-GB',
      messagesRoot: root,
      values: parseValuesFile({ 'files.title': { keep: true } }),
    })
    expect(overlay.refused[0].reason).toContain('nothing to keep')
  })

  it('refuses a per-OS entry naming a branch the source lacks', () => {
    const plan = planApply({
      tag: 'de',
      messagesRoot: root,
      values: parseValuesFile({ 'files.copy': { windows: 'x' } }),
    })
    expect(plan.refused[0].reason).toContain('plain value')
  })

  it('writes a half-translated per-OS key but leaves its stamp, so it stays on the stale list until every branch is done', () => {
    const plan = apply('de', { 'files.trash': { windows: 'Löschen' } })
    expect(plan.notes).toContain("files.trash: still English in other: the key stays stale until they're done")
    expect(read('de')['files.trash']).toBe(
      '{platform, select, macos {In den Papierkorb legen} windows {Löschen} other {Move to Trash}}',
    )
    expect(read('de')['@files.trash']).toEqual({ sourceHash: sourceHash('In den Papierkorb') })
    expect(plan.writes[0].restamped).toBe(false)
  })

  it('stamps a per-OS key with an English branch when the file justifies it', () => {
    apply('de', {
      'files.trash': { windows: 'Löschen' },
      '@files.trash': { sameAsSourceJustification: 'Linux desktops keep the English label here.' },
    })
    expect(read('de')['@files.trash']).toEqual({
      sourceHash: sourceHash(PER_OS),
      sameAsSourceJustification: 'Linux desktops keep the English label here.',
    })
  })

  it('drops the source-bound vouches when the value changes, and records a justification from the file', () => {
    apply('de', { 'files.title': 'Files', '@files.title': { sameAsSourceJustification: 'Same word in German.' } })
    expect(read('de')['@files.title']).toEqual({
      sourceHash: sourceHash('Files'),
      sameAsSourceJustification: 'Same word in German.',
    })
  })

  it('reports an already-current key as unchanged and writes no file', () => {
    const plan = apply('de', { 'files.title': 'Dateien' })
    expect(plan.unchanged).toEqual(['files.title'])
    expect(plan.files.size).toBe(0)
  })

  it('refuses a file that is not in the canonical shape', () => {
    writeFileSync(join(root, 'de', 'files.json'), JSON.stringify(read('de')), 'utf8')
    const plan = planApply({ tag: 'de', messagesRoot: root, values: parseValuesFile({ 'files.title': 'Ordner' }) })
    expect(plan.refused[0].reason).toContain('canonical')
  })

  it('refuses null on a full translation', () => {
    const plan = planApply({ tag: 'de', messagesRoot: root, values: parseValuesFile({ 'files.title': null }) })
    expect(plan.refused[0].reason).toContain('full translation')
  })
})

describe('planApply on an overlay', () => {
  beforeEach(() => {
    write('en-GB', 'files.json', {
      'files.copy': 'Copy {count} files, mate',
      '@files.copy': { sourceHash: sourceHash('Copy {count} files') },
    })
  })

  it('adds a new fork in English order, stamped with the value it overrides', () => {
    apply('en-GB', { 'files.trash': '{platform, select, macos {Move to Bin} windows {Delete} other {Move to Bin}}' })

    expect(Object.keys(read('en-GB'))).toEqual(['files.copy', '@files.copy', 'files.trash', '@files.trash'])
    expect(read('en-GB')['@files.trash']).toEqual({ sourceHash: sourceHash(PER_OS) })
  })

  it('refuses a fork identical to what it overrides, and drops a fork given null', () => {
    const same = planApply({ tag: 'en-GB', messagesRoot: root, values: parseValuesFile({ 'files.title': 'Files' }) })
    expect(same.refused[0].reason).toContain('identical to what it overrides')

    apply('en-GB', { 'files.copy': null })
    expect(read('en-GB')).toEqual({})
  })
})

describe('the values file and coverage', () => {
  it('rejects a malformed entry and a stray @key', () => {
    expect(() => parseValuesFile({ a: 1 })).toThrow('must be a string')
    expect(() => parseValuesFile({ a: { linux: 'x' } })).toThrow('per-OS entry')
    expect(() => parseValuesFile({ '@a': { sameAsSourceJustification: 'x' } })).toThrow('gives no value')
  })

  it('finds the newest run directory of a language, and none for a language without one', () => {
    const work = join(root, 'work')
    const first = newWorkDir('de', { root: work, now: new Date(2026, 10, 13, 2, 0, 0) })
    const second = newWorkDir('de', { root: work, now: new Date(2026, 10, 13, 2, 0, 0) })
    const later = newWorkDir('de', { root: work, now: new Date(2026, 10, 13, 2, 0, 5) })
    expect(first).not.toBe(second)
    expect(latestWorkDir('de', { root: work })).toBe(later)
    expect(latestWorkDir('fr', { root: work })).toBeUndefined()
  })

  it('lists expected keys the batch leaves out', () => {
    expect(uncoveredKeys(['a', 'b', 'c'], parseValuesFile({ a: 'x', c: 'y' }))).toEqual(['b'])
  })

  it('parses the CLI flags and refuses several locales', () => {
    expect(parseApplyArgs(['--lang', 'de', '--values', 'v.json', '--dry-run'])).toMatchObject({
      lang: 'de',
      values: 'v.json',
      dryRun: true,
    })
    expect(() => parseApplyArgs(['--lang', 'de,fr', '--values', 'v.json'])).toThrow('one locale')
    expect(parseApplyArgs(['--lang', 'de']).values).toBeUndefined()
  })
})
