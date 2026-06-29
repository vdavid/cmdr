import { describe, it, expect } from 'vitest'
import type { TagRef } from '$lib/ipc/bindings'
import {
  tagDotsModel,
  tagClusterWidthPx,
  tagColorVar,
  coloredTagCount,
  isColoredTag,
  TAG_CLUSTER_GAP,
  TAG_DOT_SIZE,
  TAG_DOT_OVERLAP_OFFSET,
  TAG_CHIP_EXTRA,
} from './tag-dots-utils'

const tag = (name: string, color: number): TagRef => ({ name, color })

describe('tagDotsModel', () => {
  it('returns no dots for an empty tag list', () => {
    const m = tagDotsModel([])
    expect(m.dots).toEqual([])
    expect(m.overflowCount).toBe(0)
    expect(m.label).toBe('')
  })

  it('treats undefined tags as none', () => {
    expect(tagDotsModel(undefined).dots).toEqual([])
  })

  it('skips colourless (color 0) tags but keeps them in the label', () => {
    const m = tagDotsModel([tag('Work', 0), tag('Urgent', 6)])
    expect(m.dots).toEqual([{ color: 6 }])
    expect(m.overflowCount).toBe(0)
    // Label lists ALL tag names, including colourless ones.
    expect(m.label).toBe('Work, Urgent')
  })

  it('shows every dot when the colored count is 3 or fewer', () => {
    const m = tagDotsModel([tag('a', 6), tag('b', 2), tag('c', 4)])
    expect(m.dots).toEqual([{ color: 6 }, { color: 2 }, { color: 4 }])
    expect(m.overflowCount).toBe(0)
  })

  it('shows 2 dots + "+2" overflow for 4 colored tags', () => {
    const m = tagDotsModel([tag('a', 1), tag('b', 2), tag('c', 3), tag('d', 4)])
    expect(m.dots).toEqual([{ color: 1 }, { color: 2 }])
    expect(m.overflowCount).toBe(2)
  })

  it('shows 2 dots + "+3" overflow for 5 colored tags', () => {
    const m = tagDotsModel([tag('a', 1), tag('b', 2), tag('c', 3), tag('d', 4), tag('e', 5)])
    expect(m.dots).toEqual([{ color: 1 }, { color: 2 }])
    expect(m.overflowCount).toBe(3)
  })

  it('counts only colored tags toward overflow, not colourless ones', () => {
    const m = tagDotsModel([tag('a', 0), tag('b', 0), tag('c', 6), tag('d', 2)])
    expect(m.dots).toEqual([{ color: 6 }, { color: 2 }])
    expect(m.overflowCount).toBe(0)
    expect(m.label).toBe('a, b, c, d')
  })
})

describe('tagColorVar', () => {
  it('maps each color index 1-7 to its token', () => {
    expect(tagColorVar(1)).toBe('var(--color-tag-grey)')
    expect(tagColorVar(2)).toBe('var(--color-tag-green)')
    expect(tagColorVar(3)).toBe('var(--color-tag-purple)')
    expect(tagColorVar(4)).toBe('var(--color-tag-blue)')
    expect(tagColorVar(5)).toBe('var(--color-tag-yellow)')
    expect(tagColorVar(6)).toBe('var(--color-tag-red)')
    expect(tagColorVar(7)).toBe('var(--color-tag-orange)')
  })

  it('returns undefined for the colourless index and out-of-range values', () => {
    expect(tagColorVar(0)).toBeUndefined()
    expect(tagColorVar(8)).toBeUndefined()
  })
})

describe('isColoredTag / coloredTagCount', () => {
  it('classifies colored vs colourless', () => {
    expect(isColoredTag(tag('x', 0))).toBe(false)
    expect(isColoredTag(tag('x', 6))).toBe(true)
  })

  it('counts only colored tags', () => {
    expect(coloredTagCount([tag('a', 0), tag('b', 6), tag('c', 2)])).toBe(2)
    expect(coloredTagCount(undefined)).toBe(0)
  })
})

describe('tagClusterWidthPx', () => {
  it('is zero with no colored tags', () => {
    expect(tagClusterWidthPx(0)).toBe(0)
  })

  it('grows by the overlap offset per dot up to the cap', () => {
    expect(tagClusterWidthPx(1)).toBe(TAG_CLUSTER_GAP + TAG_DOT_SIZE)
    expect(tagClusterWidthPx(2)).toBe(TAG_CLUSTER_GAP + TAG_DOT_SIZE + TAG_DOT_OVERLAP_OFFSET)
    expect(tagClusterWidthPx(3)).toBe(TAG_CLUSTER_GAP + TAG_DOT_SIZE + 2 * TAG_DOT_OVERLAP_OFFSET)
  })

  it('adds the chip width once the count overflows (4+), capped at 3 slots', () => {
    const threeDots = TAG_CLUSTER_GAP + TAG_DOT_SIZE + 2 * TAG_DOT_OVERLAP_OFFSET
    expect(tagClusterWidthPx(4)).toBe(threeDots + TAG_CHIP_EXTRA)
    // The width plateaus past the cap regardless of how many tags there are.
    expect(tagClusterWidthPx(42)).toBe(tagClusterWidthPx(4))
  })
})
