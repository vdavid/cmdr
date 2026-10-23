// Which product clips exist in `public/clips/`, read once at build time. `pnpm marketing:clips`
// (apps/desktop) writes them; this module only reads. Import it from component frontmatter only,
// never from a client `<script>`.
//
// File contract per clip, `theme` being `light` or `dark`:
// - `/clips/{slug}-{theme}-av1.mp4`: AV1, preferred where the browser can decode it
// - `/clips/{slug}-{theme}-h264.mp4`: H.264, the fallback every browser plays
// - `/clips/{slug}-{theme}.webp`: the poster frame
// - `/clips/clips.json`: `{ clips: [{ slug, width, height, duration, themes }] }`, as
//   `apps/desktop/scripts/marketing-clips-encode.ts` writes it
//
// A clip counts as available when its clips.json entry, both H.264 files, and both posters exist. The
// AV1 files are optional per theme: without them the player offers H.264 only.

import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

export const CLIP_SLUGS = [
  'copy-rollback',
  'ops-queue',
  'multi-rename',
  'search',
  'language-switch',
  'viewer',
  'settings',
] as const

export type ClipSlug = (typeof CLIP_SLUGS)[number]
export type ClipTheme = 'light' | 'dark'

export interface ClipThemeFiles {
  av1: string | null
  h264: string
  poster: string
}

export interface ProductClip {
  slug: ClipSlug
  width: number
  height: number
  /** Seconds. */
  duration: number
  files: Record<ClipTheme, ClipThemeFiles>
}

interface ClipMeta {
  width: number
  height: number
  duration: number
}

// Astro runs builds, checks, and the dev server with `apps/website` as the working directory.
const CLIPS_DIR = join(process.cwd(), 'public', 'clips')

function isClipMeta(value: unknown): value is ClipMeta {
  if (typeof value !== 'object' || value === null) return false
  const meta = value as Record<string, unknown>
  return (
    typeof meta.width === 'number' &&
    typeof meta.height === 'number' &&
    typeof meta.duration === 'number' &&
    meta.width > 0 &&
    meta.height > 0
  )
}

/** The manifest's entries, keyed by slug. */
function readManifest(): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(readFileSync(join(CLIPS_DIR, 'clips.json'), 'utf8'))
    if (typeof parsed !== 'object' || parsed === null) return {}
    const clips = (parsed as { clips?: unknown }).clips
    if (!Array.isArray(clips)) return {}
    const bySlug: Record<string, unknown> = {}
    for (const entry of clips as unknown[]) {
      if (typeof entry === 'object' && entry !== null && typeof (entry as { slug?: unknown }).slug === 'string') {
        bySlug[(entry as { slug: string }).slug] = entry
      }
    }
    return bySlug
  } catch {
    return {}
  }
}

function themeFiles(slug: ClipSlug, theme: ClipTheme): ClipThemeFiles | null {
  const h264 = `${slug}-${theme}-h264.mp4`
  const poster = `${slug}-${theme}.webp`
  const av1 = `${slug}-${theme}-av1.mp4`
  if (!existsSync(join(CLIPS_DIR, h264)) || !existsSync(join(CLIPS_DIR, poster))) return null
  return {
    av1: existsSync(join(CLIPS_DIR, av1)) ? `/clips/${av1}` : null,
    h264: `/clips/${h264}`,
    poster: `/clips/${poster}`,
  }
}

/** The clip's files and size, or `null` when it hasn't been recorded yet (callers show a still). */
export function getProductClip(slug: ClipSlug): ProductClip | null {
  const meta = readManifest()[slug]
  if (!isClipMeta(meta)) return null
  const light = themeFiles(slug, 'light')
  const dark = themeFiles(slug, 'dark')
  if (!light || !dark) return null
  return { slug, width: meta.width, height: meta.height, duration: meta.duration, files: { light, dark } }
}
