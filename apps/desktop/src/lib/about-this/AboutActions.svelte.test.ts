/**
 * The card's footer actions, over the gallery's fixture cards: what shows, what waits behind "More", that nothing in
 * the footer takes focus on open, and that a press hands its action to the opener.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import type { AboutAction } from '#lib/ipc/bindings.js'
import AboutThisDialog from './AboutThisDialog.svelte'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

let mounted: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

async function render(id: string, onAct: (action: AboutAction) => void = () => {}): Promise<HTMLElement> {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, { target, props: { card, onClose: () => {}, onGoTo: () => {}, onAct } })
  flushSync()
  await tick()
  return target
}

function footerActions(target: HTMLElement): string[] {
  return [...target.querySelectorAll('[data-about-actions] [data-action]')].map(
    (element) => element.getAttribute('data-action') ?? '',
  )
}

describe('the footer actions', () => {
  it('a downloaded disk image shows open, reveal, and its page, with trash behind More', async () => {
    const target = await render('downloaded')
    expect(footerActions(target)).toEqual(['openWithDefaultApp', 'showInFileManager', 'openDownloadPage', 'more'])
    const page = target.querySelector('[data-action="openDownloadPage"]')
    // The host is on the button before the press.
    expect(page?.textContent).toContain('tessera.example')
  })

  it('a plain document shows its three, trash last', async () => {
    const target = await render('plain-document')
    expect(footerActions(target)).toEqual(['openWithDefaultApp', 'showInFileManager', 'moveToTrash'])
  })

  it('never puts a removal first, and a card that can only delete hides it behind More', async () => {
    for (const id of Object.keys(aboutThisFixtures)) {
      const target = await render(id)
      const actions = footerActions(target)
      expect(['moveToTrash', 'delete']).not.toContain(actions[0])
      if (mounted) void unmount(mounted)
      mounted = null
      document.body.innerHTML = ''
    }
    const remote = await render('remote')
    expect(footerActions(remote)).toEqual(['more'])
  })

  it('takes no focus on open: the body has it, never an action', async () => {
    const target = await render('downloaded')
    const focused = document.activeElement
    expect(focused?.closest('[data-about-actions]')).toBeNull()
    expect(target.querySelector('.card')).toBe(focused)
  })

  it('hands a press to the opener with the action Rust derived', async () => {
    const onAct = vi.fn()
    const target = await render('downloaded', onAct)
    target.querySelector<HTMLButtonElement>('[data-action="openDownloadPage"] button')?.click()
    expect(onAct).toHaveBeenCalledWith({
      type: 'openDownloadPage',
      host: 'tessera.example',
      url: 'https://tessera.example/download',
    })
  })
})
