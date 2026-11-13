import { describe, expect, it } from 'vitest'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'
import { sectionsOf, tallyRows } from './about-card-view'

function fixture(id: string) {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  return card
}

describe('sectionsOf', () => {
  it('keeps the plan’s order and leaves the removal section to its own block', () => {
    const sections = sectionsOf(fixture('folder')).map((view) => view.section)
    expect(sections).toEqual(['nature', 'origin', 'use', 'context'])
  })

  it('collapses a section whose rows all settled as ordinary absences, and only once settled', () => {
    const settled = sectionsOf(fixture('plain-document'))
    expect(settled.find((view) => view.section === 'origin')?.collapsed).toBe(true)
    expect(settled.find((view) => view.section === 'nature')?.collapsed).toBe(false)

    const filling = sectionsOf({ ...fixture('plain-document'), settled: false })
    expect(filling.every((view) => !view.collapsed)).toBe(true)
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

describe('a calm card', () => {
  it('shows a plain document with more known rows than unknown ones', () => {
    const tally = tallyRows(fixture('plain-document'))
    expect(tally.pending).toBe(0)
    expect(tally.known).toBeGreaterThan(tally.unknown)
  })
})
