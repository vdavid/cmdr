import { describe, it, expect } from 'vitest'
import { getInitialFileName } from './new-file-operations'

/** A pane whose checked cursor read (`refreshCursorEntry`) answers `entry`. */
function makeMockPaneRef(entry: { name: string; isDirectory: boolean } | null) {
  return { refreshCursorEntry: () => Promise.resolve(entry) } as never
}

describe('getInitialFileName', () => {
  it('returns full filename with extension for files', async () => {
    const result = await getInitialFileName(makeMockPaneRef({ name: 'notes.txt', isDirectory: false }))
    expect(result).toBe('notes.txt')
  })

  it('returns empty string for directories', async () => {
    const result = await getInitialFileName(makeMockPaneRef({ name: 'my-folder', isDirectory: true }))
    expect(result).toBe('')
  })

  it('returns empty string for the ".." entry', async () => {
    const result = await getInitialFileName(makeMockPaneRef({ name: '..', isDirectory: true }))
    expect(result).toBe('')
  })

  it('returns empty string when the rows never settled', async () => {
    expect(await getInitialFileName(makeMockPaneRef(null))).toBe('')
  })

  it('returns empty string when paneRef is undefined', async () => {
    expect(await getInitialFileName(undefined)).toBe('')
  })

  it('returns full name for dotfiles', async () => {
    const result = await getInitialFileName(makeMockPaneRef({ name: '.gitignore', isDirectory: false }))
    expect(result).toBe('.gitignore')
  })

  it('handles files without extension', async () => {
    const result = await getInitialFileName(makeMockPaneRef({ name: 'Makefile', isDirectory: false }))
    expect(result).toBe('Makefile')
  })
})
