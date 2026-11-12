/**
 * The palette and the shortcuts editor search what each OS names a command, and the old words
 * stay findable as keywords: Windows names the home command "Go to user folder" (its own "Home"
 * is File Explorer's landing page), so "home" keeps finding it there; "recycle bin" finds the
 * trash commands everywhere. Vocabulary: `docs/style-guide.md` § "Platform vocabulary".
 */
import { describe, expect, it, onTestFinished } from 'vitest'
import { searchAllCommands, searchCommands } from './fuzzy-search'
import { commands } from './command-registry'
import { withPlatform } from '#lib/platform/test-platform.js'

const PLATFORMS = ['macos', 'windows', 'linux'] as const

const nameOf = (id: string): string | undefined => commands.find((c) => c.id === id)?.name
const paletteIds = (query: string): string[] => searchCommands(query).map((m) => m.command.id)
const editorIds = (query: string): string[] => searchAllCommands(query).map((m) => m.command.id)

describe('commands named in each OS’s words', () => {
  it.each([
    ['macos', 'Go to home folder', 'Go to trash', 'Undo move to trash', 'Quit Cmdr'],
    ['windows', 'Go to user folder', 'Go to Recycle Bin', 'Undo delete', 'Exit Cmdr'],
    ['linux', 'Go to home folder', 'Go to trash', 'Undo move to trash', 'Quit Cmdr'],
  ] as const)('on %s', (platform, home, goToTrash, undoTrash, quit) => {
    onTestFinished(withPlatform(platform))
    expect(nameOf('nav.goHome')).toBe(home)
    expect(nameOf('file.goToTrash')).toBe(goToTrash)
    expect(nameOf('edit.undoTrash')).toBe(undoTrash)
    expect(nameOf('app.quit')).toBe(quit)
  })
})

describe('searching for a command by its old or other OS’s words', () => {
  it('finds the user folder command by “home” on Windows, in the palette and the shortcuts editor', () => {
    onTestFinished(withPlatform('windows'))
    const palette = searchCommands('home').find((m) => m.command.id === 'nav.goHome')
    expect(palette?.command.displayName).toBe('Go to user folder')
    expect(editorIds('home')).toContain('nav.goHome')
    expect(paletteIds('user folder')).toContain('nav.goHome')
    expect(editorIds('user folder')).toContain('nav.goHome')
  })

  it.each(PLATFORMS)('finds “Go to” the trash by “recycle bin” on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(paletteIds('recycle bin')).toContain('file.goToTrash')
    expect(editorIds('recycle bin')).toContain('file.goToTrash')
    expect(editorIds('recycle bin')).toContain('edit.undoTrash')
  })

  it.each(['macos', 'linux'] as const)('finds the home command by “home folder” on %s', (platform) => {
    onTestFinished(withPlatform(platform))
    expect(paletteIds('home folder')).toContain('nav.goHome')
  })
})
