import { describe, it, expect } from 'vitest'
import { deriveFolderIndexState, type FolderIndexInputs } from './folder-index-state'

function inputs(overrides: Partial<FolderIndexInputs> = {}): FolderIndexInputs {
  return {
    enabled: true,
    scope: 'chosen',
    chosenFolders: [],
    excludedFolders: [],
    folderPath: '/Users/dave/Photos',
    enriching: false,
    ...overrides,
  }
}

describe('deriveFolderIndexState', () => {
  it('says nothing at all while image search is off', () => {
    expect(deriveFolderIndexState(inputs({ enabled: false }))).toBe('off')
    // Even for a folder that would otherwise be covered.
    expect(deriveFolderIndexState(inputs({ enabled: false, chosenFolders: ['/Users/dave/Photos'] }))).toBe('off')
  })

  it('reports an unchosen folder as not indexed in the narrow scope', () => {
    expect(deriveFolderIndexState(inputs())).toBe('notIndexed')
  })

  it('reports a chosen folder, and a folder under a chosen one, as indexed', () => {
    expect(deriveFolderIndexState(inputs({ chosenFolders: ['/Users/dave/Photos'] }))).toBe('indexed')
    expect(
      deriveFolderIndexState(inputs({ folderPath: '/Users/dave/Photos/2026', chosenFolders: ['/Users/dave/Photos'] })),
    ).toBe('indexed')
    // A name-prefix sibling is NOT under it.
    expect(
      deriveFolderIndexState(inputs({ folderPath: '/Users/dave/Photos2', chosenFolders: ['/Users/dave/Photos'] })),
    ).toBe('notIndexed')
  })

  it('lets the exclusion veto beat a chosen folder', () => {
    expect(
      deriveFolderIndexState(
        inputs({ chosenFolders: ['/Users/dave/Photos'], excludedFolders: ['/Users/dave/Photos'] }),
      ),
    ).toBe('excluded')
    // An excluded ancestor vetoes a chosen child too.
    expect(
      deriveFolderIndexState(
        inputs({
          folderPath: '/Users/dave/Photos/IDs',
          chosenFolders: ['/Users/dave/Photos/IDs'],
          excludedFolders: ['/Users/dave/Photos'],
        }),
      ),
    ).toBe('excluded')
  })

  it('shows a running pass only for a folder it can prove is covered', () => {
    expect(deriveFolderIndexState(inputs({ chosenFolders: ['/Users/dave/Photos'], enriching: true }))).toBe('indexing')
    // Not covered: the drive is busy, but not with this folder.
    expect(deriveFolderIndexState(inputs({ enriching: true }))).toBe('notIndexed')
    // Excluded: a running pass never touches it.
    expect(deriveFolderIndexState(inputs({ excludedFolders: ['/Users/dave/Photos'], enriching: true }))).toBe(
      'excluded',
    )
  })

  it('stays honest in the automatic scope, where coverage is unknown', () => {
    // No explicit choice: importance decides, and the frontend cannot know the score
    // without a new per-folder query. Never claim indexed, never claim not indexed.
    expect(deriveFolderIndexState(inputs({ scope: 'importance' }))).toBe('automatic')
    expect(deriveFolderIndexState(inputs({ scope: 'importance', enriching: true }))).toBe('automatic')
    // An explicit choice is still knowable in the automatic scope.
    expect(deriveFolderIndexState(inputs({ scope: 'importance', chosenFolders: ['/Users/dave/Photos'] }))).toBe(
      'indexed',
    )
    expect(deriveFolderIndexState(inputs({ scope: 'importance', excludedFolders: ['/Users/dave/Photos'] }))).toBe(
      'excluded',
    )
  })

  it('ignores trailing slashes on either side', () => {
    expect(
      deriveFolderIndexState(inputs({ folderPath: '/Users/dave/Photos/', chosenFolders: ['/Users/dave/Photos'] })),
    ).toBe('indexed')
    expect(deriveFolderIndexState(inputs({ chosenFolders: ['/Users/dave/Photos/'] }))).toBe('indexed')
  })

  it('reports nothing for a path it cannot match', () => {
    // An empty path (a pane that hasn't landed yet) must not read as "not indexed".
    expect(deriveFolderIndexState(inputs({ folderPath: '' }))).toBe('off')
  })
})
