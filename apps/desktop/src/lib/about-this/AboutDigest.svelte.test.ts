/**
 * What's inside and an app's Library data on the card, over the gallery's fixture cards: what each says, and that a
 * row naming a place takes the pane there.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'
import AboutThisDialog from './AboutThisDialog.svelte'
import type { AboutGoTarget } from './about-card'
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

async function render(id: string, onGoTo: (target: AboutGoTarget) => void = () => {}): Promise<HTMLElement> {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, { target, props: { card, onClose: () => {}, onGoTo, onAct: () => {} } })
  flushSync()
  await tick()
  return target
}

function section(target: HTMLElement, name: string): HTMLElement {
  const found = target.querySelector<HTMLElement>(`[data-section="${name}"]`)
  if (!found) throw new Error(`no ${name} section`)
  return found
}

function text(element: Element): string {
  return element.textContent.replace(/\s+/g, ' ').trim()
}

describe("What's inside", () => {
  it('leads a folder’s card and names its biggest children and largest files', async () => {
    const target = await render('folder')
    expect(target.querySelector('[data-section]')?.getAttribute('data-section')).toBe('contents')
    const contents = text(section(target, 'contents'))
    expect(contents).toContain('14 items: 7 folders, 4 documents, 2 code files, and 1 video')
    expect(contents).toContain('photos')
    expect(contents).toContain('team-offsite.mp4')
    expect(contents).toContain('in photos/2024')
    expect(contents).toContain('At least 41% of it hasn’t changed in a year')
    // A partly walked child's size says so.
    expect(contents).toContain('at least')
  })

  it('takes the pane to a child it names, as a folder or as a file in its folder', async () => {
    const onGoTo = vi.fn()
    const target = await render('folder', onGoTo)
    const buttons = [...section(target, 'contents').querySelectorAll<HTMLButtonElement>('button')]
    buttons.find((button) => text(button).startsWith('photos'))?.click()
    buttons.find((button) => text(button).startsWith('launch-video.mov'))?.click()
    expect(onGoTo.mock.calls).toEqual([
      [{ path: '/Users/test_user_1/projects/website/photos', isFolder: true }],
      [{ path: '/Users/test_user_1/projects/website/launch-video.mov', isFolder: false }],
    ])
  })

  it('says what node_modules is', async () => {
    const target = await render('node-modules')
    expect(text(section(target, 'contents'))).toContain('Installed packages a package manager put here')
    expect(text(target)).toContain('Not ranked')
  })

  it('says an empty folder is empty, in one line and nothing more', async () => {
    const contents = section(await render('empty-folder'), 'contents')
    expect(text(contents)).toBe('What’s inside Contents Empty')
    expect(contents.querySelector('button')).toBeNull()
  })

  // Its two searches scan the whole arena, so on a big drive it lands after the first frame: the section holds the
  // digest's whole height until the facts settle, and lets it go then, with every other reservation.
  it('holds its whole height while the card fills, and only then', async () => {
    const folder = aboutThisFixtures.folder
    if (!folder) throw new Error('no folder fixture')
    const filling = { ...folder, settled: false, facts: { ...folder.facts, folderDigest: undefined } }
    let mountedCard = mount(AboutThisDialog, {
      target: document.body.appendChild(document.createElement('div')),
      props: { card: filling, onClose: () => {}, onGoTo: () => {}, onAct: () => {} },
    })
    flushSync()
    await tick()
    const reserved = document.querySelector<HTMLElement>('[data-section="contents"] dl')?.style.minHeight
    expect(reserved).toContain('19')
    void unmount(mountedCard)
    document.body.innerHTML = ''
    const landed = { ...folder, settled: false }
    mountedCard = mount(AboutThisDialog, {
      target: document.body.appendChild(document.createElement('div')),
      props: { card: landed, onClose: () => {}, onGoTo: () => {}, onAct: () => {} },
    })
    flushSync()
    await tick()
    expect(document.querySelector<HTMLElement>('[data-section="contents"] dl')?.style.minHeight).toBe(reserved)
    void unmount(mountedCard)
    document.body.innerHTML = ''
    const settled = section(await render('folder'), 'contents').querySelector<HTMLElement>('dl')
    expect(settled?.style.minHeight).toBe('')
  })

  it('says a folder the index hasn’t listed is not indexed, never empty', async () => {
    const contents = text(section(await render('not-indexed-folder'), 'contents'))
    expect(contents).not.toContain('Empty')
    expect(contents).toMatch(/index/i)
  })
})

describe('an app’s data in the Library', () => {
  it('lists its folders with their place, the total as a lower bound, and the places not looked in', async () => {
    const target = await render('app')
    const context = text(section(target, 'context'))
    expect(context).toContain('At least 2.47 GB in 3 folders')
    expect(context).toContain('Containers')
    expect(context).toContain('hasn’t listed Group Containers and Logs yet')
    expect(text(target)).toContain('Its data in your Library stays: at least 2.47 GB in 3 folders.')
  })

  it('takes the pane to one of its folders', async () => {
    const onGoTo = vi.fn()
    const target = await render('app', onGoTo)
    section(target, 'context').querySelector<HTMLButtonElement>('button')?.click()
    expect(onGoTo).toHaveBeenCalledWith({
      path: '/Users/test_user_1/Library/Containers/example.tessera.mac',
      isFolder: true,
    })
  })
})
