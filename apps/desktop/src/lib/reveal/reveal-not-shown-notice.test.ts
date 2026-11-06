import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({ addToast: vi.fn(), tString: vi.fn() }))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: mocks.addToast }))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: mocks.tString }))
vi.mock('#lib/tauri-commands/index.js', () => ({ onRevealNotShown: vi.fn() }))

import { REVEAL_NOT_SHOWN_TOAST_ID, showRevealNotShownNotice } from './reveal-not-shown-notice'

describe('showRevealNotShownNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.tString.mockReturnValue('worded')
  })

  it('says the file is missing from the folder it opened, then leaves on its own', () => {
    showRevealNotShownNotice({ name: 'a.zip', folder: 'Downloads', reason: 'notInFolder' })
    expect(mocks.tString).toHaveBeenCalledWith('main.revealNotShown.notInFolder', {
      folder: 'Downloads',
      name: 'a.zip',
    })
    expect(mocks.addToast).toHaveBeenCalledWith('worded', {
      level: 'warn',
      dismissal: 'transient',
      id: REVEAL_NOT_SHOWN_TOAST_ID,
    })
  })

  it('names the folder it couldn’t open and the file it was for', () => {
    showRevealNotShownNotice({ name: 'a.zip', folder: 'Downloads', reason: 'folderNotOpened' })
    expect(mocks.tString).toHaveBeenCalledWith('main.revealNotShown.folderNotOpened', {
      folder: 'Downloads',
      name: 'a.zip',
    })
  })

  it('names a revealed folder once when the folder itself didn’t open', () => {
    showRevealNotShownNotice({ name: 'Photos', folder: 'Photos', reason: 'folderNotOpened' })
    expect(mocks.tString).toHaveBeenCalledWith('main.revealNotShown.folderItselfNotOpened', { name: 'Photos' })
  })

  it('blames a drive that isn’t answering', () => {
    showRevealNotShownNotice({ name: 'a.txt', folder: 'nas', reason: 'notAnswering' })
    expect(mocks.tString).toHaveBeenCalledWith('main.revealNotShown.notAnswering', { name: 'a.txt' })
  })
})
