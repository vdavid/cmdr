/**
 * Tests for the take controller in `marketing-clips-take.ts`, against a stand-in
 * recorder that speaks the real one's stdout protocol. The real recorder needs Screen
 * Recording and a window; the protocol handling, the temp-name rename, and the markers
 * file don't, and a mistake in them would cost a whole overnight run.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startTake } from './marketing-clips-take.js'

/** A recorder that records nothing: `recording` at once, the `--out` file and `done` on `stop`. */
const FAKE_RECORDER = `#!/usr/bin/env node
const args = process.argv.slice(2)
const out = args[args.indexOf('--out') + 1]
if (process.env.FAKE_RECORDER_FAIL) {
  console.log(JSON.stringify({ event: 'error', message: 'window 7 is not shareable' }))
  process.exit(1)
}
console.log(JSON.stringify({ event: 'recording', firstFrameEpochMs: Date.now() - 5, width: 2000, height: 1280 }))
process.stdin.setEncoding('utf8')
process.stdin.on('data', (chunk) => {
  if (!chunk.includes('stop')) return
  require('node:fs').writeFileSync(out, 'mov')
  console.log(JSON.stringify({ event: 'done', frames: 3, durationS: 0.1 }))
  process.exit(0)
})
`

describe('startTake', () => {
  let dir = ''
  const saved = { ...process.env }

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'cmdr-clips-take-'))
    const recorder = join(dir, 'fake-recorder.cjs')
    writeFileSync(recorder, FAKE_RECORDER)
    chmodSync(recorder, 0o755)
    process.env.CMDR_CLIPS_RECORDER = recorder
    process.env.CMDR_CLIPS_MASTERS_DIR = dir
  })

  afterEach(() => {
    // In place, ❌ never `process.env = { ...saved }`: a spawned child inherits the real
    // environment, not a replacement object, so later tests' keys never reached the fake
    // recorder (Node 25, 2026-10-26).
    for (const key of Object.keys(process.env)) {
      if (!(key in saved)) Reflect.deleteProperty(process.env, key)
    }
    Object.assign(process.env, saved)
    rmSync(dir, { recursive: true, force: true })
  })

  it('writes the master under its final name, with markers on the recorder clock', async () => {
    const take = await startTake({ slug: 'search', theme: 'light', windowId: 42 })
    take.markStart()
    take.markPoster()
    await take.finish()

    expect(existsSync(join(dir, 'search-light.mov'))).toBe(true)
    expect(existsSync(join(dir, 'search-light.recording.mov'))).toBe(false)
    const markers = JSON.parse(readFileSync(join(dir, 'search-light.markers.json'), 'utf8')) as Record<string, number>
    expect(markers.firstFrameEpochMs).toBeLessThanOrEqual(markers.startEpochMs)
    expect(markers.startEpochMs).toBeLessThanOrEqual(markers.posterEpochMs)
    expect(markers.posterEpochMs).toBeLessThanOrEqual(markers.endEpochMs)
  })

  it('refuses to finish a take that never marked its start, keeping the old master', async () => {
    writeFileSync(join(dir, 'search-dark.mov'), 'the last good master')
    const take = await startTake({ slug: 'search', theme: 'dark', windowId: 42 })
    await expect(take.finish()).rejects.toThrow(/never marked its start/)
    expect(readFileSync(join(dir, 'search-dark.mov'), 'utf8')).toBe('the last good master')
  })

  it("surfaces the recorder's own error message", async () => {
    process.env.FAKE_RECORDER_FAIL = '1'
    await expect(startTake({ slug: 'search', theme: 'dark', windowId: 7 })).rejects.toThrow(/not shareable/)
  })
})
