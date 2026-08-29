/**
 * Drift guard for the two English overlays, `en-GB` and `en-AU`.
 *
 * These two catalogs are written BY HAND and agree on 149 of the 160 keys either
 * one forks,
 * because `en-AU` inherits from `en` and never from `en-GB` (`inheritableAncestors`
 * walks a tag's own ancestors, and `en-GB` is not one of `en-AU`'s). So a shared
 * British form has to be typed into both, and nothing else in the pipeline notices
 * when someone edits one and forgets the other: coverage, parity, and stale each
 * compare a catalog against `en`, never against its sibling.
 *
 * This file is that missing comparison, in three parts:
 *
 *  1. The divergence set: the handful of keys where Australian English is
 *     deliberately NOT British, so any OTHER difference fails loudly as accidental
 *     drift. Adding a real AU-only fork means adding it here on purpose, with its
 *     evidence in `docs/i18n/en-AU/style.md`.
 *  2. A VOCABULARY sweep (`FORKED_TERMS`). Coverage, parity, and stale all compare
 *     key SETS, so none of them notices that "colour" forked in one area file while
 *     "color" survived in another; a half-forked vocabulary makes the app contradict
 *     itself. This sweeps every base-`en` value for each forked term and fails on any
 *     key the overlays skipped, plus any American form left inside a forked value.
 *  3. Renderability: placeholder/tag/category parity with base `en` (an identifier is
 *     not copy), and `*Aria`-contains-its-visible-label (WCAG 2.5.3), which breaks the
 *     moment a label forks and its aria sibling doesn't.
 *
 * Rulings and sources for every fork asserted below: `docs/i18n/en-GB/style.md`
 * and `docs/i18n/en-AU/style.md`.
 */
import { describe, expect, it } from 'vitest'

import {
  loadCatalog,
  isRawKey,
  listLocales,
  parseMessage,
  resolveLocaleSource,
  sourceHash,
  visibleLiterals,
} from './i18n-catalog-lib.ts'

const en = loadCatalog('en')
const gb = loadCatalog('en-GB')
const au = loadCatalog('en-AU')

/**
 * Keys where `en-AU` deliberately says something other than `en-GB`. Anything else
 * differing between the two is drift. See `docs/i18n/en-AU/style.md` § Where AU
 * diverges from GB.
 */
const AU_DIVERGES_FROM_GB: readonly string[] = [
  // Australian Finder's Edit menu reads "Unselect All" where the British one reads
  // "Deselect All" (`Finder/MenuBar.json:300488.title`).
  'commands.selectionToggleAndDown.description',
  'commands.selectionDeselectAll.label',
  'commands.selectionDeselectFiles.label',
  'commands.selectionDeselectFiles.description',
  'menu.select.deselectAll',
  'menu.select.deselectFiles',
  'settings.selection.recentSelections.maxCount.description',
  // The dialog those menu items open. Its title and primary button carry the same
  // verb, so leaving them out is what made `en-AU` read "Unselect files…" in the
  // menu bar and "Deselect files" in the window it opened. The positive verb does
  // NOT fork: `Select` and `Select All` are identical in `en-GB` and `en-AU`.
  'selection.dialog.title.remove',
  'selection.action.deselect.label',
  'selection.action.deselect.tooltip',
  // `en-GB` forks the adverb to "go forwards"; `en-AU` keeps the American form, so
  // this key is absent from `en-AU` entirely and inherits base `en`.
  'commands.navForward.label',
]

/**
 * The text a reader actually SEES, with every identifier removed: no placeholder
 * names, no `<tag>` names, no `plural`/`select` category labels. A vocabulary sweep
 * has to match words against this and never against the raw value, because
 * `settings.appearance.tintTriggerAria`'s only "color" is the `{colorName}`
 * placeholder NAME and `queue.row.label`'s only "trash" is the operation-kind
 * selector Rust sends over IPC. Forking either would break rendering.
 *
 * The RAW families (`errors.*`, native `menu.*`) never reach the ICU engine, so
 * they get the same treatment by hand: strip `{token}` spans, keep the rest.
 */
function visibleTextOf(key: string, value: string): string {
  if (isRawKey(key)) return value.replaceAll(/\{[^{}]*\}/g, ' ')
  return visibleLiterals(value) ?? value.replaceAll(/\{[^{}]*\}/g, ' ')
}

/**
 * Every American form these overlays fork, with the locales that fork it. Mirrors
 * `docs/i18n/en-GB/glossary.md` and `docs/i18n/en-AU/glossary.md`; each pattern
 * covers the whole inflection family, because a HALF-forked vocabulary is the
 * failure this file exists to catch: "colour" in one file and "color" in another
 * makes the app contradict itself, and no other check compares a catalog's
 * vocabulary against English at all.
 */
const FORKED_TERMS: readonly { term: string; pattern: RegExp; locales: readonly string[] }[] = [
  { term: 'color', pattern: /\bcolor(s|ed|ing|ful|less)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'favorite', pattern: /\bfavorite(s|d)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'behavior', pattern: /\bbehavior(s|al|ally)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'organize', pattern: /\borganiz(e|es|ed|ing|ation|ations|ational|er|ers)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'customize', pattern: /\bcustomiz(e|es|ed|ing|ation|ations|able)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'recognize', pattern: /\brecogniz(e|es|ed|ing|able)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'synchronize', pattern: /\bsynchroniz(e|es|ed|ing|ation)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'minimize', pattern: /\bminimiz(e|es|ed|ing|ation)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'virtualize', pattern: /\bvirtualiz(e|es|ed|ing|ation)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'prioritize', pattern: /\bprioritiz(e|es|ed|ing|ation)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'canceled', pattern: /\bcancel(ed|ing|ation|ations)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'license (noun)', pattern: /\blicens(e|es|ed)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'trash', pattern: /\btrash(es|ed|ing)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'gray', pattern: /\bgray(s|ed|ing|scale|ish)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'percent', pattern: /\bpercent(age|ages)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'toward', pattern: /\btoward\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'gotten', pattern: /\bgotten\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'aging', pattern: /\baging\b/i, locales: ['en-GB', 'en-AU'] },
  // AU-only: Australian Finder says "Unselect All" where the British one keeps "Deselect All".
  { term: 'deselect', pattern: /\bdeselect(s|ed|ing|ion|ions)?\b/i, locales: ['en-AU'] },
  // GB-only: the adverbial `-s` on the VERB phrase. The Go-menu noun ("Forward",
  // `menu.go.forward`) stays put in both, matching `Finder/MenuBar.json:249.title`.
  { term: 'go forward', pattern: /\b(go|goes|going|went)\s+forward\b/i, locales: ['en-GB'] },

  // Families base `en` doesn't use TODAY. Listed anyway, so the guard is armed
  // BEFORE the copy that introduces one: a term only reaches this list after
  // someone notices a half-fork, which is the failure the list exists to
  // prevent. Zero matches costs one regex per value and buys the first
  // occurrence being caught on the commit that adds it.
  //
  // Each pattern matches the AMERICAN form only. A British form here would make
  // the correctly-forked overlay value read as a leftover: `catalogue` must not
  // match, or `en-GB` fails the sweep for spelling it right.
  { term: 'authorize', pattern: /\bauthoriz(e|es|ed|ing|ation|ations)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'initialize', pattern: /\binitializ(e|es|ed|ing|ation)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'analyze', pattern: /\banalyz(e|es|ed|ing)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'center', pattern: /\bcenter(s|ed|ing)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'defense', pattern: /\bdefens(e|es|ive)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'catalog', pattern: /\bcatalog(s|ed|ing)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'installment', pattern: /\binstallment(s)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'judgment', pattern: /\bjudgment(s)?\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'labeled', pattern: /\blabel(ed|ing)\b/i, locales: ['en-GB', 'en-AU'] },
  { term: 'traveled', pattern: /\btravel(ed|ing|er|ers)\b/i, locales: ['en-GB', 'en-AU'] },
]

describe('en-GB and en-AU are overlays of en, not full translations', () => {
  it('both resolve as overlays that override en', () => {
    const shipped = listLocales()
    expect(resolveLocaleSource('en-GB', shipped)).toEqual({ overrides: 'en', isOverlay: true })
    expect(resolveLocaleSource('en-AU', shipped)).toEqual({ overrides: 'en', isOverlay: true })
  })

  it('each stays a small fork, nowhere near a full mirror of en', () => {
    const enKeys = Object.keys(en.messages).length
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      const n = Object.keys(cat.messages).length
      expect(n, `${tag} is empty`).toBeGreaterThan(0)
      // A tenth of the catalog is generous headroom; a full mirror would be 100%.
      expect(n / enKeys, `${tag} has grown into a near-copy of en`).toBeLessThan(0.1)
    }
  })
})

describe('every overlay key genuinely forks and is anchored to en', () => {
  it('invents no key that base en does not define', () => {
    const strays: string[] = []
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      for (const key of Object.keys(cat.messages)) {
        if (!(key in en.messages)) strays.push(`${tag}: ${key}`)
      }
    }
    // Nothing would ever render an invented key.
    expect(strays).toEqual([])
  })

  it('never repeats base en verbatim (that key would be dead weight)', () => {
    const dead: string[] = []
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      for (const [key, value] of Object.entries(cat.messages)) {
        if (en.messages[key] === value) dead.push(`${tag}: ${key}`)
      }
    }
    expect(dead).toEqual([])
  })

  it('stamps every @key.sourceHash from the en value it overrides', () => {
    const wrong: string[] = []
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      for (const key of Object.keys(cat.messages)) {
        const stamped = (cat.metadata[key] as { sourceHash?: string } | undefined)?.sourceHash
        const expected = sourceHash(en.messages[key])
        if (stamped !== expected) wrong.push(`${tag}: ${key} (stamped ${String(stamped)}, want ${expected})`)
      }
    }
    // A wrong hash silently disarms the staleness net for that key forever.
    expect(wrong).toEqual([])
  })
})

describe('en-AU tracks en-GB except where it deliberately does not', () => {
  it('agrees with en-GB on every key that is not a recorded divergence', () => {
    const divergences = new Set(AU_DIVERGES_FROM_GB)
    const drift: string[] = []
    for (const key of new Set([...Object.keys(gb.messages), ...Object.keys(au.messages)])) {
      if (divergences.has(key)) continue
      if (gb.messages[key] !== au.messages[key]) {
        drift.push(`${key}: en-GB ${JSON.stringify(gb.messages[key])} vs en-AU ${JSON.stringify(au.messages[key])}`)
      }
    }
    expect(drift).toEqual([])
  })

  it('actually diverges on every key listed as a divergence', () => {
    // Pre-fix this would have passed wrongly: a stale entry left here after the two
    // catalogs were reconciled would quietly widen the allowance above.
    const stale = AU_DIVERGES_FROM_GB.filter((key) => gb.messages[key] === au.messages[key])
    expect(stale).toEqual([])
  })

  it('keeps en-GB-only keys out of en-AU, and vice versa, only where recorded', () => {
    expect(gb.messages['commands.navForward.label']).toBe('Go forwards')
    expect(au.messages).not.toHaveProperty('commands.navForward.label')
    expect(au.messages['menu.select.deselectAll']).toBe('Unselect all')
    expect(gb.messages['menu.select.deselectAll']).toBeUndefined()
  })
})

describe('the forked vocabulary is whole, not half-applied', () => {
  it('leaves no base-en key carrying a forked term without an overlay entry', () => {
    // The headline guard. A term that forks in one area file and survives in
    // another makes the app contradict itself, and coverage/parity/stale all
    // compare KEY SETS, so none of them can see it.
    const stragglers: string[] = []
    for (const [key, value] of Object.entries(en.messages)) {
      const visible = visibleTextOf(key, value)
      for (const { term, pattern, locales } of FORKED_TERMS) {
        if (!pattern.test(visible)) continue
        for (const tag of locales) {
          const cat = tag === 'en-GB' ? gb : au
          if (!(key in cat.messages)) stragglers.push(`${tag}: ${key} still says "${term}"`)
        }
      }
    }
    expect(stragglers).toEqual([])
  })

  it('leaves no American form inside an overlay value it was supposed to replace', () => {
    // The other half: the key IS forked, but one occurrence in it was missed.
    const leftovers: string[] = []
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      for (const [key, value] of Object.entries(cat.messages)) {
        const visible = visibleTextOf(key, value)
        for (const { term, pattern, locales } of FORKED_TERMS) {
          if (!locales.includes(tag)) continue
          if (pattern.test(visible)) leftovers.push(`${tag}: ${key} still says "${term}"`)
        }
      }
    }
    expect(leftovers).toEqual([])
  })
})

describe('every overlay value stays renderable where base en is', () => {
  it('matches base en on placeholder names, tag names, and select/plural categories', () => {
    // An identifier is not copy. Rename one and the branch goes unreachable or the
    // substitution silently renders the literal token, in a locale nobody on the
    // team reads back.
    const mismatches: string[] = []
    const sorted = (set: Iterable<string>): string[] => [...set].sort()
    const categoriesOf = (map: ReadonlyMap<string, Set<string>>): string[] =>
      [...map].flatMap(([arg, cats]) => [...cats].map((c) => `${arg}:${c}`)).sort()

    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      for (const [key, value] of Object.entries(cat.messages)) {
        const source = en.messages[key]
        if (isRawKey(key)) {
          // The raw families never reach ICU; their `{token}` set is the contract.
          const tokensOf = (v: string): string[] => sorted(new Set(v.match(/\{[^{}]*\}/g) ?? []))
          if (String(tokensOf(value)) !== String(tokensOf(source))) {
            mismatches.push(`${tag}: ${key} raw tokens ${String(tokensOf(value))} vs en ${String(tokensOf(source))}`)
          }
          continue
        }
        const want = parseMessage(source)
        const got = parseMessage(value)
        if (!got.ok) {
          mismatches.push(`${tag}: ${key} is not valid ICU (${got.error ?? ''})`)
          continue
        }
        if (String(sorted(got.placeholders)) !== String(sorted(want.placeholders))) {
          mismatches.push(
            `${tag}: ${key} placeholders ${String(sorted(got.placeholders))} vs en ${String(sorted(want.placeholders))}`,
          )
        }
        if (String(sorted(got.tags)) !== String(sorted(want.tags))) {
          mismatches.push(`${tag}: ${key} tags ${String(sorted(got.tags))} vs en ${String(sorted(want.tags))}`)
        }
        if (String(categoriesOf(got.selectCategories)) !== String(categoriesOf(want.selectCategories))) {
          mismatches.push(
            `${tag}: ${key} select categories ${String(categoriesOf(got.selectCategories))} vs en ${String(categoriesOf(want.selectCategories))}`,
          )
        }
      }
    }
    expect(mismatches).toEqual([])
  })

  it('keeps every aria name containing the visible label it names (WCAG 2.5.3)', () => {
    // If a label forks and its aria sibling doesn't, a voice-control user says the
    // British words on screen and nothing matches. Pairs are discovered, not listed:
    // any aria value that contains a sibling's value verbatim in base en must still
    // contain it after the overlay resolves.
    const broken: string[] = []
    const family = (key: string): string => key.split('.').slice(0, 2).join('.')
    const ariaKeys = Object.keys(en.messages).filter((k) => /aria/i.test(k))
    const labelKeys = Object.entries(en.messages).filter(([k, v]) => !/aria/i.test(k) && v.length >= 4)

    for (const ariaKey of ariaKeys) {
      for (const [labelKey, labelValue] of labelKeys) {
        if (family(labelKey) !== family(ariaKey)) continue
        if (!en.messages[ariaKey].includes(labelValue)) continue
        for (const [tag, cat] of [
          ['en-GB', gb],
          ['en-AU', au],
        ] as const) {
          const aria = cat.messages[ariaKey] ?? en.messages[ariaKey]
          const label = cat.messages[labelKey] ?? labelValue
          if (!aria.includes(label)) broken.push(`${tag}: ${ariaKey} no longer contains ${labelKey} ("${label}")`)
        }
      }
    }
    expect(broken).toEqual([])
  })
})

describe('the rulings the catalogs are supposed to encode', () => {
  it('calls the destination the Bin, capitalised, in both', () => {
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      expect(cat.messages['fileOperations.delete.trashSwitch'], tag).toBe('Move to Bin')
      expect(cat.messages['errors.mutation.trashRefused'], tag).toBe("macOS wouldn't move this to the Bin.")
      const lowercased = Object.entries(cat.messages).filter(([, v]) => / bin\b/.test(v))
      // "bin" survives only as the verb ("Counting items to bin...", "and bin old file").
      expect(lowercased.map(([k]) => k).sort(), tag).toEqual([
        'fileExplorer.renameConflict.overwriteTrash',
        'fileOperations.transferProgress.scanTitleTrash',
      ])
    }
  })

  it('leaves no American "trash" in any overlay COPY', () => {
    // `trash` survives in exactly two values, and only as an ICU `select` SELECTOR
    // bound to the operation-kind discriminator Rust sends across IPC. A selector
    // name is an identifier, so it must match base `en` byte for byte; translating
    // it would make the branch unreachable and the message fall to `other`.
    const icuSelectorOnly = new Set(['queue.row.label', 'queue.failureToast.title'])
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      const leftovers = Object.entries(cat.messages).filter(([, v]) => /trash/i.test(v))
      expect(leftovers.map(([k]) => k).sort(), tag).toEqual([...icuSelectorOnly].sort())
      for (const [key, value] of leftovers) {
        // The only occurrence is the selector itself, never prose.
        expect(value.match(/trash/gi)?.length, `${tag}: ${key}`).toBe(1)
        expect(value, `${tag}: ${key}`).toContain(' trash {')
        expect(en.messages[key], `${tag}: ${key}`).toContain(' trash {')
      }
    }
  })

  it('spells the noun licence and leaves the verb-derived licensing alone', () => {
    for (const [tag, cat] of [
      ['en-GB', gb],
      ['en-AU', au],
    ] as const) {
      expect(cat.messages['licensing.dialog.labelKey'], tag).toBe('Licence key')
      expect(cat.messages['menu.app.licenseEnter'], tag).toBe('Enter licence key…')
      const nouns = Object.entries(cat.messages).filter(([, v]) => /\blicens(e|es)\b/i.test(v))
      expect(
        nouns.map(([k]) => k),
        tag,
      ).toEqual([])
    }
  })
})
