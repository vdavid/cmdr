// Copies the repo's brand assets and the chosen music track into public/, where Remotion serves them from.
// Usage: node scripts/prepare-assets.ts
import { copyFileSync, existsSync, mkdirSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { MUSIC } from '../src/config.ts'

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

const track = join(ROOT, 'assets/music', MUSIC.source)
if (existsSync(track)) copy(track, join(PUB, MUSIC.file))
else console.warn(`No music yet: download it to assets/music/${MUSIC.source} (see CREDITS.md), then rerun.`)
