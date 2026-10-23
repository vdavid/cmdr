/**
 * The marketing clips by slug, the one list both the spec (`marketing-clips.spec.ts`) and
 * the orchestrator (`scripts/marketing-clips.ts`, for `--only` and the encode) read.
 * Import-free on purpose: the orchestrator runs under bare Node and must not pull in
 * Playwright to learn a list of names.
 */

/**
 * In recording order. ❗ `language-switch` goes LAST: after its takes the app's saved
 * language drifted back to the last previewed one (Vietnamese) despite English being
 * picked again, so any window opened later in the same run came up in Vietnamese (seen
 * on 2026-10-22, reported as a possible app bug). The next launch re-pins English.
 */
export const CLIP_SLUGS = [
  'copy-rollback',
  'ops-queue',
  'multi-rename',
  'search',
  'viewer',
  'settings',
  'language-switch',
] as const

export type ClipSlug = (typeof CLIP_SLUGS)[number]

/**
 * The demo video's beats (`marketing-video-scenes.ts`), in recording order, dark only.
 * `care` goes last for the same reason `language-switch` does.
 */
export const VIDEO_BEATS = ['speed', 'trust', 'rename', 'reach', 'ai', 'hero', 'care'] as const

export type VideoBeat = (typeof VIDEO_BEATS)[number]

/** A video beat's test title prefix, kept apart from the clip slugs for `--grep`. */
export const VIDEO_TITLE_PREFIX = 'video-'

/**
 * The second demo video's beats ("safe file operations, to and from anywhere"),
 * `marketing-video2-scenes.ts`, dark only. `hero2` and `smb` reach the NAS, `phone` the
 * virtual MTP device the run launches with.
 */
export const VIDEO2_BEATS = ['smb', 's3', 'phone', 'archives', 'transfers', 'undo', 'hero2'] as const

export type Video2Beat = (typeof VIDEO2_BEATS)[number]

/** Every footage set `pnpm marketing:clips --video` / `--video2` records: its beats, title prefix, and folder. */
export const VIDEO_SETS = {
  video: { beats: VIDEO_BEATS as readonly string[], prefix: VIDEO_TITLE_PREFIX, folder: 'video' },
  video2: { beats: VIDEO2_BEATS as readonly string[], prefix: 'video2-', folder: 'video2' },
} as const

export type VideoSetId = keyof typeof VIDEO_SETS
