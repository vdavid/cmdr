/**
 * Drift guard for the two English overlays, `en-GB` and `en-AU`.
 *
 * These two catalogs are written BY HAND and are 96% duplicates of each other,
 * because `en-AU` inherits from `en` and never from `en-GB` (`inheritableAncestors`
 * walks a tag's own ancestors, and `en-GB` is not one of `en-AU`'s). So a shared
 * British form has to be typed into both, and nothing else in the pipeline notices
 * when someone edits one and forgets the other: coverage, parity, and stale each
 * compare a catalog against `en`, never against its sibling.
 *
 * This file is that missing comparison. It pins the divergence set — the handful of
 * keys where Australian English is deliberately NOT British — so any OTHER
 * difference fails loudly as accidental drift. Adding a real AU-only fork means
 * adding it here on purpose, with its evidence in `docs/i18n/en-AU/style.md`.
 *
 * Rulings and sources for every fork asserted below: `docs/i18n/en-GB/style.md`
 * and `docs/i18n/en-AU/style.md`.
 */
import { describe, expect, it } from 'vitest'

import { loadCatalog, resolveLocaleSource, listLocales, sourceHash } from './i18n-catalog-lib.ts'

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
  // `en-GB` forks the adverb to "go forwards"; `en-AU` keeps the American form, so
  // this key is absent from `en-AU` entirely and inherits base `en`.
  'commands.navForward.label',
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
        const stamped = cat.metadata[key]?.sourceHash
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
      expect(nouns.map(([k]) => k), tag).toEqual([])
    }
  })
})
