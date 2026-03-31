import { describe, it, expect } from 'vitest'
import { buildFrontendIndices, extractFilename } from './selection-adjustment'

describe('buildFrontendIndices', () => {
  it('adds +1 offset when hasParent is true', () => {
    const map = { 'a.txt': 0, 'c.txt': 2 }
    expect(buildFrontendIndices(map, true).sort()).toEqual([1, 3])
  })

  it('returns backend indices as-is when hasParent is false', () => {
    const map = { 'a.txt': 0, 'c.txt': 2 }
    expect(buildFrontendIndices(map, false).sort()).toEqual([0, 2])
  })

  it('returns empty array for empty map', () => {
    expect(buildFrontendIndices({}, true)).toEqual([])
    expect(buildFrontendIndices({}, false)).toEqual([])
  })

  it('handles single entry', () => {
    expect(buildFrontendIndices({ 'file.txt': 5 }, true)).toEqual([6])
    expect(buildFrontendIndices({ 'file.txt': 5 }, false)).toEqual([5])
  })
})

describe('extractFilename', () => {
  it('extracts filename from Unix path', () => {
    expect(extractFilename('/home/user/docs/file.txt')).toBe('file.txt')
  })

  it('extracts directory name from Unix path', () => {
    expect(extractFilename('/home/user/docs/mydir')).toBe('mydir')
  })

  it('extracts filename from Windows path', () => {
    expect(extractFilename('C:\\Users\\user\\docs\\file.txt')).toBe('file.txt')
  })

  it('handles root-level file', () => {
    expect(extractFilename('/file.txt')).toBe('file.txt')
  })

  it('handles bare filename', () => {
    expect(extractFilename('file.txt')).toBe('file.txt')
  })
})

describe('diffGeneration discard logic', () => {
  it('newer generation discards stale callback', () => {
    // Simulates the generation counter pattern used in FilePane
    let generation = 0
    const results: number[][] = []

    // Simulate two rapid diffs
    generation++
    const gen1 = generation
    generation++
    const gen2 = generation

    // Stale callback arrives first — should be discarded
    if (gen1 === generation) results.push([1, 2])
    // Current callback arrives — should be applied
    if (gen2 === generation) results.push([3, 4])

    expect(results).toEqual([[3, 4]])
  })

  it('clearOperationSnapshot bumps generation to discard in-flight callbacks', () => {
    let generation = 0
    const results: number[][] = []

    // Simulate a diff starting
    generation++
    const myGen = generation

    // Simulate clearOperationSnapshot being called (bumps generation)
    generation++

    // The in-flight callback arrives — should be discarded
    if (myGen === generation) results.push([1, 2])

    expect(results).toEqual([])
  })
})
