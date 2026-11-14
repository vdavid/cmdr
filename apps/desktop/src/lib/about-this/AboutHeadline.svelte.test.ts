/**
 * The headline, worded from the typed parts Rust derives: one case per template and per `what`, the size's three
 * shapes, and the placeholder while nothing has landed.
 */

import { afterEach, describe, expect, it } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import type { Headline } from '#lib/ipc/bindings.js'
import AboutHeadline from './AboutHeadline.svelte'

let mounted: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

function words(headline: Headline | null, settled = true): string {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(AboutHeadline, { target, props: { headline, settled } })
  flushSync()
  return target.textContent.replace(/\s+/g, ' ').trim()
}

function reset(): void {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
}

const DAY = Math.floor(Date.parse('2026-03-02T12:00:00Z') / 1000)
const file: Headline['what'] = { type: 'file', kind: 'diskImage', extension: 'dmg' }
const plain = (what: Headline['what'], size: Headline['size'] = null): Headline => ({
  what,
  size,
  lead: { template: 'plain' },
})

describe('the headline', () => {
  it('waits with a placeholder until there is something to say, and reserves its second line meanwhile', () => {
    expect(words(null, false)).toBe('Looking this up')
    expect(document.querySelector('[data-headline]')?.classList.contains('reserving')).toBe(true)
  })

  it('names what the item is for every target', () => {
    const cases: [Headline['what'], string][] = [
      [file, 'Disk image'],
      [{ type: 'file', kind: null, extension: 'xyz' }, 'xyz'],
      [{ type: 'file', kind: null, extension: null }, 'File'],
      [{ type: 'folder' }, 'Folder'],
      [{ type: 'app', version: '2.4.1' }, '2.4.1'],
      [{ type: 'app', version: null }, 'App'],
      [{ type: 'package' }, 'Package'],
      [{ type: 'symlink' }, 'link'],
      [{ type: 'archiveEntry' }, 'archive'],
    ]
    for (const [what, expected] of cases) {
      expect(words(plain(what)).toLowerCase()).toContain(expected.toLowerCase())
      reset()
    }
  })

  it('says a lower-bound size as "at least", an exact one plainly, and no size as nothing', () => {
    expect(words(plain({ type: 'folder' }, { bytes: 2_100_000, atLeast: true }))).toContain('at least')
    reset()
    const exact = words(plain(file, { bytes: 2_100_000, atLeast: false }))
    expect(exact).toContain('MB')
    expect(exact).not.toContain('at least')
    reset()
    expect(words(plain(file))).not.toContain('MB')
  })

  it('words each lead from its template', () => {
    const leads: [Headline['lead'], string[]][] = [
      [{ template: 'insideArchive', archive: 'Photos.zip' }, ['Photos.zip']],
      [{ template: 'fromGitHistory', kind: 'branch', name: 'feature/tiles' }, ['feature/tiles']],
      [{ template: 'fromGitHistory', kind: 'commit', name: '1a2b3c4d5e6f7a8b9c0d' }, ['1a2b3c4']],
      [{ template: 'downloadedFrom', host: 'tessera.example', by: 'Chrome', at: DAY }, ['tessera.example', 'Chrome']],
      [{ template: 'downloadedFrom', host: 'tessera.example', by: null, at: null }, ['tessera.example']],
      [{ template: 'putHereByCmdr', kind: 'copy', at: DAY, sourceVolume: 'Naspolya' }, ['Naspolya']],
      [{ template: 'putHereByCmdr', kind: 'move', at: DAY, sourceVolume: null }, ['Cmdr']],
      [{ template: 'receivedByMail', at: DAY }, ['Mail']],
      [{ template: 'receivedByAirDrop', at: null }, ['AirDrop']],
      [{ template: 'dataOfApp', app: 'Slack', certainty: 'probable' }, ['probably', 'Slack']],
      [{ template: 'dataOfApp', app: 'TextEdit', certainty: 'certain' }, ['TextEdit’s data']],
      [{ template: 'cacheOfApp', app: 'Safari', certainty: 'certain' }, ['Safari’s cache']],
      [{ template: 'dataOfMissingApp', bundleId: 'com.tessera.desktop' }, ['com.tessera.desktop', 'isn’t installed']],
      [{ template: 'downloadedBy', app: 'Safari', at: DAY }, ['Safari']],
      [{ template: 'lastOpened', at: DAY }, ['opened']],
    ]
    for (const [lead, expected] of leads) {
      const text = words({ what: file, size: { bytes: 1_000, atLeast: false }, lead })
      for (const part of expected) expect(text, `${lead.template}: ${text}`).toContain(part)
      reset()
    }
  })

  it('drops the reserved second line once the facts settle', () => {
    words(plain(file), true)
    expect(document.querySelector('[data-headline]')?.classList.contains('reserving')).toBe(false)
  })
})
