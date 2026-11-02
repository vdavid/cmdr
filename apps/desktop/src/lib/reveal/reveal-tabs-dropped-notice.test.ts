import { describe, it, expect, vi, beforeEach } from 'vitest'

const mocks = vi.hoisted(() => ({ addToast: vi.fn(), tString: vi.fn() }))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast: mocks.addToast }))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: mocks.tString }))
vi.mock('#lib/tauri-commands/index.js', () => ({ onRevealTabsDropped: vi.fn() }))
vi.mock('#lib/file-explorer/tabs/tab-state-manager.svelte.js', () => ({ MAX_TABS_PER_PANE: 10 }))

import { REVEAL_TABS_DROPPED_TOAST_ID, showRevealTabsDroppedNotice } from './reveal-tabs-dropped-notice'

describe('showRevealTabsDroppedNotice', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.tString.mockReturnValue('worded')
  })

  it('names how many opened, how many did not, and the limit, then leaves on its own', () => {
    showRevealTabsDroppedNotice({ opened: 9, dropped: 3 })
    expect(mocks.tString).toHaveBeenCalledWith('main.revealTabsDropped', { opened: 9, dropped: 3, limit: 10 })
    expect(mocks.addToast).toHaveBeenCalledWith('worded', {
      level: 'warn',
      dismissal: 'transient',
      id: REVEAL_TABS_DROPPED_TOAST_ID,
    })
  })
})
