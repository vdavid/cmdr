/**
 * What the card's footer actions do once the card closed: each goes through the pane's own path for it, about the
 * card's item alone.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AboutAction } from '#lib/ipc/bindings.js'
import type { AboutHooks } from '#lib/about-this/about-this-state.svelte.js'
import type { AboutSubject } from '#lib/about-this/about-card.js'
import type { ExplorerAPI } from '../explorer-api'
import { openAboutThisFor } from './about-this-opener'

const { opened, tauri } = vi.hoisted(() => ({
  opened: [] as { subject: AboutSubject; hooks: AboutHooks | null }[],
  tauri: {
    showInFinder: vi.fn(() => Promise.resolve()),
    openExternalUrl: vi.fn(() => Promise.resolve()),
  },
}))

vi.mock('#lib/about-this/about-this-state.svelte.js', () => ({
  openAboutThis: (subject: AboutSubject, hooks: AboutHooks | null) => {
    opened.push({ subject, hooks })
  },
}))
vi.mock('#lib/tauri-commands/index.js', () => tauri)
vi.mock('#lib/file-explorer/pane/focused-pane-reads.js', () => ({ getFocusedPaneVolumeId: () => 'root' }))
vi.mock('#lib/file-explorer/navigation/navigate-and-select.js', () => ({
  resolveLocationOrToast: (path: string) => Promise.resolve({ volumeId: 'root', path }),
}))

const PATH = '/Users/test_user_1/Downloads/Tool.dmg'

function explorer(cursorPath: string = PATH) {
  return {
    getFileAndPathUnderCursor: vi.fn(() =>
      Promise.resolve({ path: cursorPath, filename: 'Tool.dmg', iconId: 'ext:dmg', isDirectory: false }),
    ),
    openItemUnderCursor: vi.fn(() => Promise.resolve()),
    openDeleteDialog: vi.fn(() => Promise.resolve()),
    navigate: vi.fn(),
    getFocusedPane: () => 'left' as const,
  }
}

async function press(action: AboutAction, pane: ReturnType<typeof explorer>): Promise<void> {
  openAboutThisFor(pane as unknown as ExplorerAPI, {
    path: PATH,
    filename: 'Tool.dmg',
    iconId: 'ext:dmg',
    isDirectory: false,
  })
  const last = opened.at(-1)
  if (!last?.hooks) throw new Error('no card opened')
  last.hooks.act(action, last.subject)
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('the card’s actions', () => {
  beforeEach(() => {
    opened.length = 0
    vi.clearAllMocks()
  })

  it('Move to trash opens the pane’s delete dialog for this item alone, asking first', async () => {
    const pane = explorer()
    await press({ type: 'moveToTrash' }, pane)
    expect(pane.openDeleteDialog).toHaveBeenCalledWith({ permanent: false, initiator: 'user', cursorPath: PATH })
  })

  it('Delete on a volume with no trash goes through the same dialog, permanent', async () => {
    const pane = explorer()
    await press({ type: 'delete' }, pane)
    expect(pane.openDeleteDialog).toHaveBeenCalledWith({ permanent: true, initiator: 'user', cursorPath: PATH })
  })

  it('Open is Enter’s own chain, and only while the cursor is still on the item', async () => {
    const pane = explorer()
    await press({ type: 'openWithDefaultApp', app: 'DiskImageMounter' }, pane)
    expect(pane.openItemUnderCursor).toHaveBeenCalledOnce()

    const moved = explorer('/Users/test_user_1/Downloads/other.txt')
    await press({ type: 'openWithDefaultApp', app: 'DiskImageMounter' }, moved)
    expect(moved.openItemUnderCursor).not.toHaveBeenCalled()
  })

  it('opens the download page by the address Rust wrote back out, and shows the item in Finder by its path', async () => {
    const pane = explorer()
    await press({ type: 'openDownloadPage', host: 'tessera.example', url: 'https://tessera.example/download' }, pane)
    expect(tauri.openExternalUrl).toHaveBeenCalledWith('https://tessera.example/download')
    await press({ type: 'showInFileManager' }, pane)
    expect(tauri.showInFinder).toHaveBeenCalledWith(PATH)
  })

  it('reveals the owning app with the cursor on it, and goes into an app’s data folder', async () => {
    const pane = explorer()
    await press({ type: 'revealOwner', app: 'Slack', path: '/Applications/Slack.app' }, pane)
    await vi.waitFor(() => {
      expect(pane.navigate).toHaveBeenCalledWith({
        pane: 'left',
        to: { goTo: { volumeId: 'root', path: '/Applications' } },
        source: 'user',
        selectName: 'Slack.app',
      })
    })
    const folder = '/Users/test_user_1/Library/Application Support/Slack'
    await press({ type: 'goToAppFolder', app: 'Slack', path: folder }, pane)
    await vi.waitFor(() => {
      expect(pane.navigate).toHaveBeenLastCalledWith({
        pane: 'left',
        to: { goTo: { volumeId: 'root', path: folder } },
        source: 'user',
      })
    })
  })
})
