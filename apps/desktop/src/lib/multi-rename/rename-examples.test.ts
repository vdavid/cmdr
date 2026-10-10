import { describe, it, expect, vi } from 'vitest'

const { renderMultiRenameExamples } = vi.hoisted(() => ({ renderMultiRenameExamples: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ renderMultiRenameExamples }))

import { DEFAULT_SPEC } from './spec'
import { MARK_END, MARK_START, example, examplePieces, marked, renderExamples } from './rename-examples'

describe('rename examples', () => {
  it('splits a rendered example into its marked parts and what’s around them', () => {
    expect(examplePieces(`2026${marked('-')}07${marked('-')}14.jpg`)).toEqual([
      { text: '2026', marked: false },
      { text: '-', marked: true },
      { text: '07', marked: false },
      { text: '-', marked: true },
      { text: '14.jpg', marked: false },
    ])
  })

  it('drops empty pieces, so an example with nothing marked is one quiet piece', () => {
    expect(examplePieces('IMG_0042.jpg')).toEqual([{ text: 'IMG_0042.jpg', marked: false }])
    expect(examplePieces(marked('Lisbon') + '.jpg')).toEqual([
      { text: 'Lisbon', marked: true },
      { text: '.jpg', marked: false },
    ])
  })

  it('reads an unclosed mark as marking the rest', () => {
    expect(examplePieces(`a${MARK_START}b`)).toEqual([
      { text: 'a', marked: false },
      { text: 'b', marked: true },
    ])
    expect(MARK_END).not.toBe(MARK_START)
  })

  it('builds an example on the default spec, changing only what it names', () => {
    expect(example('a.txt', { search: 'a' })).toEqual({ fileName: 'a.txt', spec: { ...DEFAULT_SPEC, search: 'a' } })
  })

  it('asks the engine for every example in one call and keys the answers, leaving out what it couldn’t render', async () => {
    renderMultiRenameExamples.mockResolvedValueOnce(['b.txt', null])
    const asked = new Map([
      ['good', example('a.txt', { search: 'a', replace: 'b' })],
      ['bad', example('a.txt', { nameMask: '[Q]' })],
    ])
    expect([...(await renderExamples(asked))]).toEqual([['good', 'b.txt']])
    expect(renderMultiRenameExamples).toHaveBeenCalledTimes(1)
    expect(renderMultiRenameExamples).toHaveBeenCalledWith([...asked.values()])
  })
})
