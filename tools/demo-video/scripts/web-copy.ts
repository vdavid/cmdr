// Makes a web-friendly copy of a rendered master next to it: two-pass H.264 at 1.3 Mbps (about 16 MB for 90 s, small
// UI text stays crisp), AAC 128 kbps, faststart. A fixed CRF lands near 28 MB on this grainy, hazy screen content.
// Usage: node scripts/web-copy.ts --video=anywhere   (reads out/cmdr-<id>-90s.mp4, writes out/cmdr-<id>-90s-web.mp4)
import { execFileSync } from 'node:child_process'
import { rmSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { videoFromArgs } from '../src/videos/index.ts'

const OUT = join(import.meta.dirname, '../out')
const id = videoFromArgs(process.argv).id
const master = join(OUT, `cmdr-${id}-90s.mp4`)
const web = join(OUT, `cmdr-${id}-90s-web.mp4`)
const log = join(OUT, `x264-${id}`)
const video = ['-c:v', 'libx264', '-preset', 'slow', '-b:v', '1300k', '-passlogfile', log]
execFileSync('ffmpeg', ['-v', 'error', '-y', '-i', master, ...video, '-pass', '1', '-an', '-f', 'mp4', '/dev/null'])
execFileSync('ffmpeg', [
  '-v',
  'error',
  '-y',
  '-i',
  master,
  ...video,
  '-pass',
  '2',
  '-pix_fmt',
  'yuv420p',
  '-movflags',
  '+faststart',
  '-c:a',
  'aac',
  '-b:a',
  '128k',
  web,
])
for (const suffix of ['-0.log', '-0.log.mbtree']) rmSync(log + suffix, { force: true })
console.log(`${web} (${(statSync(web).size / 1e6).toFixed(1)} MB)`)
