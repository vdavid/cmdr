import { describe, it, expect } from 'vitest'
import { loadMapGeometry } from './world-map-shapes.js'

describe('loadMapGeometry', () => {
  it('projects every mapped country into a path inside the viewBox, without Antarctica', async () => {
    const { shapes, width, height } = await loadMapGeometry()
    expect(width).toBe(960)
    // Equal Earth without Antarctica is a bit under half as tall as wide.
    expect(height).toBeGreaterThan(350)
    expect(height).toBeLessThan(480)
    expect(shapes.length).toBe(176)
    expect(shapes.find((s) => s.country === 'SE')?.d).toMatch(/^M/)
    expect(shapes.filter((s) => s.country === 'CY')).toHaveLength(2)
    expect(shapes.some((s) => s.id === '010')).toBe(false)
  })

  it('projects once and shares the result', async () => {
    expect(await loadMapGeometry()).toBe(await loadMapGeometry())
  })
})
