import { describe, it, expect } from 'vitest'
import { straysIn } from './i18n-capture-frame.js'

describe('straysIn', () => {
  it('reports nothing when the toast layer is empty and nothing was staged', () => {
    expect(straysIn([], [])).toEqual([])
  })

  it('reports any toast on a surface that staged none', () => {
    // The real defect: the virtual MTP device announces itself on its own
    // schedule, mid-shot, over a dialog that never asked for a toast.
    expect(straysIn(['Connected to Virtual Pixel 9'], [])).toEqual(['Connected to Virtual Pixel 9'])
  })

  it('accepts exactly what the surface staged', () => {
    expect(straysIn(['Zoom increased to 110%'], ['Zoom increased to 110%'])).toEqual([])
  })

  it('counts duplicates rather than matching by presence', () => {
    // Two identical toasts when one was staged means a second one arrived.
    expect(straysIn(['Copy complete', 'Copy complete'], ['Copy complete'])).toEqual(['Copy complete'])
  })

  it('allows a staged toast to vanish, since auto-dismiss is not contamination', () => {
    expect(straysIn([], ['Copy complete'])).toEqual([])
  })
})
