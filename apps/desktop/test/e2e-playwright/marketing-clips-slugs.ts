/**
 * The marketing clips by slug, the one list both the spec (`marketing-clips.spec.ts`) and
 * the orchestrator (`scripts/marketing-clips.ts`, for `--only` and the encode) read.
 * Import-free on purpose: the orchestrator runs under bare Node and must not pull in
 * Playwright to learn a list of names.
 */

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
