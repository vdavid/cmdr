import { describe, expect, it } from 'vitest'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'
import { sectionsOf, showsOnDisk, tallyRows } from './about-card-view'

function fixture(id: string) {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  return card
}

describe('sectionsOf', () => {
  it('keeps the plan’s order and leaves the removal section to its own block', () => {
    const sections = sectionsOf(fixture('folder')).map((view) => view.section)
    expect(sections).toEqual(['contents', 'nature', 'context', 'use', 'origin'])
  })

  it('collapses a section whose rows all settled as ordinary absences, and only once settled', () => {
    const settled = sectionsOf(fixture('plain-document'))
    expect(settled.find((view) => view.section === 'origin')?.collapsed).toBe(true)
    expect(settled.find((view) => view.section === 'nature')?.collapsed).toBe(false)

    const filling = sectionsOf({ ...fixture('plain-document'), settled: false })
    expect(filling.every((view) => !view.collapsed)).toBe(true)
  })

  it('names the reason a collapsed section shares, and says "nothing on record" for no record', () => {
    const unlisted = sectionsOf(fixture('not-indexed-folder'))
    expect(unlisted.find((view) => view.section === 'use')).toMatchObject({
      collapsed: true,
      collapsedWhy: 'notIndexed',
    })
    // What's inside never collapses: its class stands whatever the index knows.
    expect(unlisted.find((view) => view.section === 'contents')?.collapsed).toBe(false)
    expect(unlisted.find((view) => view.section === 'origin')).toMatchObject({ collapsed: true, collapsedWhy: null })
    expect(sectionsOf(fixture('plain-document')).find((view) => view.section === 'origin')?.collapsedWhy).toBeNull()
  })

  it('never collapses a section holding a Notable unknown', () => {
    expect(sectionsOf(fixture('notable-a')).some((view) => view.collapsed)).toBe(false)
  })

  it('shows a pending row as null until it lands', () => {
    const rows = sectionsOf(fixture('still-filling')).flatMap((view) => view.rows)
    expect(rows.find((row) => row.kind === 'lastOpened')?.fact).toBeNull()
    expect(rows.find((row) => row.kind === 'identity')?.fact).not.toBeNull()
  })
})

describe('showsOnDisk', () => {
  it('leaves out block rounding and keeps a real difference', () => {
    expect(showsOnDisk({ logicalSize: 18, physicalSize: 4096 })).toBe(false)
    expect(showsOnDisk({ logicalSize: 412_318_720, physicalSize: 412_319_744 })).toBe(false)
    expect(showsOnDisk({ logicalSize: 10_737_418_240, physicalSize: 52_428_800 })).toBe(true)
    expect(showsOnDisk({ logicalSize: null, physicalSize: 4096 })).toBe(false)
  })
})

describe('the gallery’s removal rows', () => {
  // The crate's rule (`cmdr-about::removal`): "Nothing Cmdr knows of made this" and "Cmdr can't tell" are fallbacks,
  // said only when no fact names a consequence. A fixture that puts one beside a real consequence (a recorded
  // download, uncommitted work) previews a card the backend never sends, and the two lines contradict each other.
  it('never sets a fallback beside a real consequence', () => {
    for (const [id, card] of Object.entries(aboutThisFixtures)) {
      const removal = card?.derived?.removal
      if (!removal) continue
      const all = [...removal.shown, ...removal.more]
      const fallbacks = all.filter((line) => line.type === 'userDocument' || line.type === 'cantTell')
      if (fallbacks.length > 0) expect(all, id).toHaveLength(1)
    }
  })
})

describe('a calm card', () => {
  it('shows a plain document with more known rows than unknown ones', () => {
    const tally = tallyRows(fixture('plain-document'))
    expect(tally.pending).toBe(0)
    expect(tally.known).toBeGreaterThan(tally.unknown)
  })
})
