import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { _setLocaleForTests } from '$lib/intl/locale'
import { costRequestFor, roundsToZero, visibleCostLines } from './s3-cost-line'

describe('roundsToZero', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('is true below half a cent and false from half a cent up', () => {
    expect(roundsToZero(0, 'USD')).toBe(true)
    expect(roundsToZero(0.004, 'USD')).toBe(true)
    expect(roundsToZero(0.005, 'USD')).toBe(false)
  })

  it('uses the currency’s own minor unit', () => {
    // The yen has no minor unit, so 0.4 rounds to zero and 0.6 doesn't.
    expect(roundsToZero(0.4, 'JPY')).toBe(true)
    expect(roundsToZero(0.6, 'JPY')).toBe(false)
  })
})

describe('visibleCostLines', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('hides an estimate that rounds to zero and rounds the rest through the locale layer', () => {
    _setLocaleForTests('en-US')
    expect(
      visibleCostLines([
        { amount: 0.004, currency: 'USD', providerLabel: 'AWS' },
        { amount: 0.005, currency: 'USD', providerLabel: 'Wasabi' },
      ]),
    ).toEqual([{ providerLabel: 'Wasabi', amountText: '$0.01' }])
  })

  it('shows two providers in two currencies, each in its own', () => {
    _setLocaleForTests('en-US')
    expect(
      visibleCostLines([
        { amount: 0.42, currency: 'USD', providerLabel: 'AWS' },
        { amount: 1.5, currency: 'EUR', providerLabel: 'Hetzner' },
      ]),
    ).toEqual([
      { providerLabel: 'AWS', amountText: '$0.42' },
      { providerLabel: 'Hetzner', amountText: '€1.50' },
    ])
  })

  it('formats in the OS formatting locale', () => {
    _setLocaleForTests('de-DE')
    expect(visibleCostLines([{ amount: 1.5, currency: 'EUR', providerLabel: 'Hetzner' }])).toEqual([
      { providerLabel: 'Hetzner', amountText: '1,50 €' },
    ])
  })
})

describe('costRequestFor', () => {
  const settled = { scanComplete: true, previewId: 'p1', sourceVolumeId: 'src', destinationVolumeId: 'dst' }

  it('asks for a copy or a move once the scan settled with a preview id', () => {
    expect(costRequestFor({ ...settled, operation: 'copy' })).toEqual({
      operation: 'copy',
      previewId: 'p1',
      sourceVolumeId: 'src',
      destinationVolumeId: 'dst',
    })
    expect(costRequestFor({ ...settled, operation: 'move' })?.operation).toBe('move')
  })

  it('carries what the conflict check found, so overwrites are priced', () => {
    const clashes = {
      resolution: 'overwrite' as const,
      clashes: [{ sourceSize: 10, destSize: 5, sourceModified: 200, destModified: 100 }],
    }
    expect(costRequestFor({ ...settled, operation: 'copy', clashes })?.clashes).toEqual(clashes)
    expect(costRequestFor({ ...settled, operation: 'copy', clashes: null })).not.toHaveProperty('clashes')
  })

  it('asks for a delete with no destination', () => {
    expect(costRequestFor({ ...settled, operation: 'delete', destinationVolumeId: null })).toEqual({
      operation: 'delete',
      previewId: 'p1',
      sourceVolumeId: 'src',
      destinationVolumeId: null,
    })
  })

  it('doesn’t ask yet while the scan runs or before the preview id is known', () => {
    expect(costRequestFor({ ...settled, operation: 'copy', scanComplete: false })).toBeNull()
    expect(costRequestFor({ ...settled, operation: 'copy', previewId: null })).toBeNull()
  })

  it('never asks for an operation that isn’t priced, like compress', () => {
    expect(costRequestFor({ ...settled, operation: 'compress' })).toBeNull()
    expect(costRequestFor({ ...settled, operation: 'trash' })).toBeNull()
  })
})
