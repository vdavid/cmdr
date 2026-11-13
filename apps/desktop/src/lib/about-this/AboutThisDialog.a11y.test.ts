/**
 * Tier 3 a11y tests for `AboutThisDialog.svelte` over the gallery's fixture cards: a full card, one still filling
 * (pending rows), one with every Notable reason, a remote card with its volume note, a share that stopped answering,
 * an archive's entry, a snapshot from git's history, an unavailable one, and the AI take's states. Then each part on its own.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount, type Component } from 'svelte'
import AboutThisDialog from './AboutThisDialog.svelte'
import AboutRow from './AboutRow.svelte'
import AboutAbsence from './AboutAbsence.svelte'
import AboutPath from './AboutPath.svelte'
import AboutHeadline from './AboutHeadline.svelte'
import AboutRemoval from './AboutRemoval.svelte'
import AboutDigest from './AboutDigest.svelte'
import AboutAppData from './AboutAppData.svelte'
import AboutPlaceList from './AboutPlaceList.svelte'
import AboutActions from './AboutActions.svelte'
import AboutVerdict from './AboutVerdict.svelte'
import { aboutThisFixtures } from '#lib/dialog-gallery/fixtures/about-this.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

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

async function render(id: string): Promise<HTMLElement> {
  const card = aboutThisFixtures[id]
  if (!card) throw new Error(`no fixture ${id}`)
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutThisDialog, {
    target,
    props: { card, onClose: () => {}, onGoTo: () => {}, onAct: () => {}, onStopVerdict: () => {} },
  })
  flushSync()
  await tick()
  return target
}

describe('AboutThisDialog a11y', () => {
  it.each([
    'downloaded',
    'still-filling',
    'notable-a',
    'remote',
    'unavailable',
    'app',
    'folder',
    'node-modules',
    'empty-folder',
    'not-indexed-folder',
    'stopped-answering',
    'archive-entry',
    'verdict-waiting',
    'verdict-streaming',
    'verdict-done',
    'verdict-stopped',
    'verdict-no-consent',
    'verdict-managed',
    'verdict-ai-off',
    'git-snapshot',
  ])('has no violations: %s', async (id) => {
    await expectNoA11yViolations(await render(id))
  })

  it('lists each section’s rows as a description list, marks pending rows busy, and focuses the body', async () => {
    const target = await render('still-filling')
    expect(target.querySelectorAll('dl').length).toBeGreaterThan(0)
    expect(target.querySelector('[aria-busy="true"]')).not.toBeNull()
    expect(document.activeElement?.classList.contains('card')).toBe(true)
  })

  it('announces the headline once, when the facts settle, and never per fact', async () => {
    const target = await render('plain-document')
    // Two quiet regions: the headline's, and the AI take's (empty: this card has none). Nothing else is live.
    const live = [...target.querySelectorAll('[aria-live]')].map((region) => region.textContent.trim())
    expect(live).toHaveLength(2)
    expect(live[0]).toContain('Document')
    expect(live[1]).toBe('')
  })
})

describe('the card’s parts on their own', () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- one helper mounts parts whose props differ; each call passes that part's real props
  function mountInto(component: Component<any>, props: Record<string, unknown>, tag = 'div'): HTMLElement {
    if (mounted) void unmount(mounted)
    const target = document.createElement(tag)
    document.body.appendChild(target)
    mounted = mount(component, { target, props })
    flushSync()
    return target
  }

  it('a row, inside its description list', async () => {
    const download = aboutThisFixtures.downloaded?.facts.download ?? null
    const props = { row: { kind: 'download' as const, fact: download }, target: 'file' as const, settled: true }
    await expectNoA11yViolations(mountInto(AboutRow, props, 'dl'))
  })

  it('an absence, Routine and Notable', async () => {
    await expectNoA11yViolations(
      mountInto(AboutAbsence, { kind: 'download', unknown: { why: 'noRecord', prominence: 'routine' } }),
    )
    await expectNoA11yViolations(
      mountInto(AboutAbsence, { kind: 'download', unknown: { why: 'timedOut', prominence: 'notable' } }),
    )
  })

  it('a path with its copy button', async () => {
    await expectNoA11yViolations(mountInto(AboutPath, { path: '/Users/test_user_1/a.txt', copyLabel: 'Copy the path' }))
  })

  it('the headline, pending and known', async () => {
    await expectNoA11yViolations(mountInto(AboutHeadline, { headline: null, settled: false }))
    const headline = aboutThisFixtures.downloaded?.derived?.headline ?? null
    await expectNoA11yViolations(mountInto(AboutHeadline, { headline, settled: true }))
  })

  it('What’s inside, an app’s Library data, and a list of places', async () => {
    const digest = aboutThisFixtures.folder?.facts.folderDigest
    if (digest?.kind !== 'folderDigest' || digest.fact.state !== 'known')
      throw new Error('the folder fixture has a digest')
    await expectNoA11yViolations(
      mountInto(
        AboutDigest,
        { digest: digest.fact.value, folderPath: '/Users/test_user_1/website', onGoTo: () => {} },
        'dl',
      ),
    )
    const data = aboutThisFixtures.app?.facts.appData
    if (data?.kind !== 'appData' || data.fact.state !== 'known') throw new Error('the app fixture has its data')
    await expectNoA11yViolations(mountInto(AboutAppData, { data: data.fact.value, onGoTo: () => {} }))
    const items = [
      {
        name: 'media',
        path: '/a/media',
        isFolder: true,
        size: { state: 'unknown', value: { why: 'notIndexed', prominence: 'routine' } },
        detail: null,
      },
    ]
    await expectNoA11yViolations(mountInto(AboutPlaceList, { items, onGoTo: () => {} }))
  })

  it('the footer actions, shown ones and the More button', async () => {
    const actions = aboutThisFixtures.downloaded?.derived?.actions ?? null
    expect(actions?.more.length).toBeGreaterThan(0)
    await expectNoA11yViolations(mountInto(AboutActions, { actions, onAct: () => {} }))
  })

  it('the removal list with its More button', async () => {
    const removal = aboutThisFixtures['more-consequences']?.derived?.removal ?? null
    await expectNoA11yViolations(mountInto(AboutRemoval, { removal, settled: true }))
  })

  it('the AI take, writing, stopped, and refused for consent', async () => {
    for (const id of ['verdict-streaming', 'verdict-stopped', 'verdict-no-consent']) {
      const verdict = aboutThisFixtures[id]?.verdict
      if (!verdict) throw new Error(`no verdict on ${id}`)
      await expectNoA11yViolations(mountInto(AboutVerdict, { verdict, onStop: () => {} }))
    }
  })
})

describe('what a card reached through its volume says', () => {
  it('a share that stopped answering says so once, at the top, and each row quietly', async () => {
    const target = await render('stopped-answering')
    expect(target.textContent).toContain('Naspolya stopped answering')
    expect(target.textContent.split('stopped answering').length).toBe(2)
    expect(target.textContent).toContain('No answer from the drive')
  })

  it('a file inside a zip names the archive, then the archive’s record', async () => {
    const target = await render('archive-entry')
    expect(target.textContent).toContain('Inside the archive “Holiday photos.zip”.')
    expect(target.textContent).toContain('recorded about the archive')
    expect(target.textContent).toContain('photos.example')
  })

  it('a file in git’s history names the commit, short, and says there’s nothing to remove', async () => {
    const target = await render('git-snapshot')
    expect(target.textContent).toContain('As commit 1a2b3c4 has it in')
    expect(target.textContent).not.toContain('1a2b3c4d5e6f')
    expect(target.textContent).toContain('git’s record of it')
  })
})
