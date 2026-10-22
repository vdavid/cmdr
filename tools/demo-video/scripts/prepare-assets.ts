// Copies the repo's brand assets and each video's music track into public/, where Remotion serves them from, and
// maps each track's strong drum hits (for the pulse effects). Usage: node scripts/prepare-assets.ts
import { copyFileSync, existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { paths } from '../src/config.ts'
import { VIDEOS } from '../src/videos/index.ts'
import { strongOnsets } from './lib/audio.ts'

const ROOT = join(import.meta.dirname, '..')
const REPO = join(ROOT, '../..')
const PUB = join(ROOT, 'public')

function copy(from: string, to: string): void {
  if (!existsSync(from)) throw new Error(`Missing ${from}`)
  mkdirSync(join(to, '..'), { recursive: true })
  copyFileSync(from, to)
  console.log(`${from.replace(REPO + '/', '')} → public/${to.replace(PUB + '/', '')}`)
}

copy(join(REPO, 'brand/logos/cmdr.svg'), join(PUB, 'brand/cmdr.svg'))
const shots = join(REPO, 'brand/screenshots')
for (const name of readdirSync(shots).filter((n) => n.endsWith('-dark.webp')))
  copy(join(shots, name), join(PUB, 'brand/screenshots', name))
copy(join(REPO, 'apps/website/public/fonts/inter-latin-variable.woff2'), join(PUB, 'brand/fonts/inter.woff2'))

for (const video of VIDEOS) {
  const track = join(ROOT, 'assets/music', video.music.source)
  if (!existsSync(track)) {
    console.warn(`${video.id}: no music yet. Download it to assets/music/${video.music.source} (see CREDITS.md).`)
    continue
  }
  const p = paths(video.id)
  copy(track, join(PUB, p.music))
  const onsets = strongOnsets(track)
  writeFileSync(join(PUB, p.onsets), JSON.stringify(onsets))
  console.log(`${video.id}: ${onsets.length} drum hits → public/${p.onsets}`)
}
