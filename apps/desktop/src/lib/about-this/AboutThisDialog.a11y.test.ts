/**
 * Tier 3 a11y tests for `AboutThisDialog.svelte` over the gallery's fixture cards: a full card, one still filling
 * (pending rows), one with every Notable reason, a remote card with its volume note, and an unavailable one.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount, type Component } from 'svelte'
import AboutThisDialog from './AboutThisDialog.svelte'
import AboutRow from './AboutRow.svelte'
import AboutAbsence from './AboutAbsence.svelte'
import AboutPath from './AboutPath.svelte'
import AboutHeadline from './AboutHeadline.svelte'
import AboutRemoval from './AboutRemoval.svelte'
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
  mounted = mount(AboutThisDialog, { target, props: { card, onClose: () => {}, onGoTo: () => {} } })
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
    const live = target.querySelectorAll('[aria-live]')
    expect(live).toHaveLength(1)
    expect(live[0].textContent).toContain('Document')
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

  it('the removal list with its More button', async () => {
    const removal = aboutThisFixtures['more-consequences']?.derived?.removal ?? null
    await expectNoA11yViolations(mountInto(AboutRemoval, { removal, settled: true }))
  })
})
