/** Tests for the demo video's footage manifest (`marketing-video.ts`). */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { beatEntry, buildVideoManifest } from './marketing-video.ts'

const markers = {
  slug: 'trust',
  firstFrameEpochMs: 10_000,
  startEpochMs: 11_500,
  endEpochMs: 27_250,
  posterEpochMs: 15_000,
  notes: [
    { epochMs: 12_300, label: 'F5' },
    { epochMs: 14_000, label: 'Pause' },
  ],
}

describe('beatEntry', () => {
  it('turns wall-clock markers into seconds from the master start', () => {
    expect(beatEntry(markers, { duration: 17.4, width: 2560, height: 1600 }, 'trust-dark.mov')).toEqual({
      id: 'trust',
      file: 'trust-dark.mov',
      duration: 17.4,
      width: 2560,
      height: 1600,
      usable: { start: 1.5, end: 17.25 },
      keyFrame: 5,
      notes: [
        { t: 2.3, label: 'F5' },
        { t: 4, label: 'Pause' },
      ],
    })
  })

  it('carries a remark only when there is one', () => {
    expect(beatEntry(markers, { duration: 1, width: 2, height: 2 }, 'x.mov', 'no AI provider').remark).toBe(
      'no AI provider',
    )
  })
})

describe('buildVideoManifest', () => {
  let dir = ''

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'cmdr-video-manifest-'))
  })

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  it('lists fresh beats in order and reports the missing and the stale', () => {
    for (const beat of ['speed', 'trust', 'hero']) {
      writeFileSync(join(dir, `${beat}-dark.mov`), 'mov')
      writeFileSync(join(dir, `${beat}-dark.markers.json`), JSON.stringify({ ...markers, slug: beat }))
    }
    // `hero` is from an earlier run.
    utimesSync(join(dir, 'hero-dark.markers.json'), new Date(1_000), new Date(1_000))

    const { manifest, missing } = buildVideoManifest({
      mastersDir: dir,
      beats: ['speed', 'trust', 'rename', 'hero'],
      notBefore: 1_000_000,
      remarks: {},
      probe: () => ({ duration: 18, width: 2560, height: 1600 }),
    })

    expect(manifest.beats.map((b) => b.id)).toEqual(['speed', 'trust'])
    expect(missing).toEqual(['rename', 'hero'])
  })
})
