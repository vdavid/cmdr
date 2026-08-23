/**
 * The block that opens a thread the agent started for itself.
 *
 * The load-bearing property is that every WORD comes from the catalog and every NUMBER from
 * the wire: the backend hands over counts and paths precisely so ten locales can each say it
 * their own way, and a rendered backend sentence appearing here would be untranslated copy
 * frozen in `main.db`. The rest is the disclosure: collapsed by default, and honest about the
 * folders it did not have room to name.
 */

import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import AskCmdrWakeDigest from './AskCmdrWakeDigest.svelte'
import type { WakeDigestFolderView, WakeDigestRollupView } from '$lib/tauri-commands'

function folder(overrides: Partial<WakeDigestFolderView> = {}): WakeDigestFolderView {
  return { folder: '/Users/dana/Downloads', created: 0, modified: 0, removed: 0, renamed: 0, ...overrides }
}

function render(folders: WakeDigestFolderView[], rollups: WakeDigestRollupView[] = []): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrWakeDigest, { target, props: { folders, rollups } })
  return target
}

function toggleOf(target: HTMLElement): HTMLButtonElement {
  const toggle = target.querySelector<HTMLButtonElement>('.digest-toggle')
  if (toggle === null) throw new Error('expected a .digest-toggle button')
  return toggle
}

describe('AskCmdrWakeDigest', () => {
  it('starts collapsed, so a thread opens on what the agent SAID', async () => {
    const target = render([folder({ created: 4 })])
    await tick()
    expect(toggleOf(target).getAttribute('aria-expanded')).toBe('false')
    expect(target.querySelector('.detail')).toBeNull()
  })

  it('counts every folder the digest covered, the rolled-up ones included', async () => {
    const target = render(
      [folder({ created: 1 }), folder({ folder: '/Users/dana/Desktop', modified: 2 })],
      [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }],
    )
    await tick()
    // Two named plus seven summarized: a summary that counted only the named ones would
    // disagree with what expanding reveals.
    expect(toggleOf(target).textContent).toContain('9')
  })

  it('names only the kinds that happened, in one comma-separated tally', async () => {
    const target = render([folder({ created: 3, renamed: 1 })])
    await tick()
    toggleOf(target).click()
    await tick()
    const counts = target.querySelector('.counts')?.textContent ?? ''
    expect(counts).toContain('3 new items')
    expect(counts).toContain('1 renamed item')
    expect(counts).not.toContain('changed')
    expect(counts).not.toContain('removed')
  })

  it('says something rather than nothing when every count is zero', async () => {
    const target = render([folder()])
    await tick()
    toggleOf(target).click()
    await tick()
    expect(target.querySelector('.counts')?.textContent).toBe('Nothing changed')
  })

  it('admits how many folders it is not showing', async () => {
    const target = render([folder({ created: 1 })], [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }])
    await tick()
    toggleOf(target).click()
    await tick()
    const rollup = target.querySelector('.rollup')?.textContent ?? ''
    expect(rollup).toContain('7')
    expect(rollup).toContain('/Users/dana/Projects')
    expect(rollup).toContain('40')
  })

  /** A folder name is attacker-controlled: the rail renders it as escaped plain text, and
   *  this is the one block whose whole content comes from disk. */
  it('renders a folder name as text, never as markup', async () => {
    const target = render([folder({ folder: '/Users/dana/<img src=x onerror=alert(1)>', created: 1 })])
    await tick()
    toggleOf(target).click()
    await tick()
    expect(target.querySelector('img')).toBeNull()
    expect(target.querySelector('.path')?.textContent).toBe('/Users/dana/<img src=x onerror=alert(1)>')
  })
})
