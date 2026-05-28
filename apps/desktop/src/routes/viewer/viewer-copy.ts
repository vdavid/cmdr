/**
 * Three-band copy policy for the viewer's selection-to-clipboard flow.
 *
 * The user need is mundane: most copies are tiny and should land silently. A copy of
 * 10 MiB is unusual enough to deserve a confirm; over 100 MiB risks freezing the
 * downstream app's paste handler, so we refuse and offer a save-as alternative.
 *
 * Thresholds are fixed binary bytes, independent of the user's display setting; the
 * sizes shown in dialogs/toasts go through `formatBytes()`, which honours
 * `appearance.fileSizeFormat`.
 */

/** 10 MiB. At this size, paste in most apps is still smooth, but we ask the user. */
export const COPY_CONFIRM_BYTES = 10 * 1024 * 1024

/** 100 MiB. Above this we refuse the direct copy and steer to save-as. */
export const COPY_REFUSE_BYTES = 100 * 1024 * 1024

export type CopyAction = 'silent' | 'confirm' | 'refuse'

/**
 * Picks the right copy band for `bytes`. Boundary semantics:
 *
 * - `< 10 MiB` → `silent`
 * - `[10 MiB, 100 MiB)` → `confirm`
 * - `>= 100 MiB` → `refuse`
 *
 * Negative inputs (impossible but defended) are treated as `silent`.
 */
export function selectCopyAction(bytes: number): CopyAction {
  if (bytes >= COPY_REFUSE_BYTES) return 'refuse'
  if (bytes >= COPY_CONFIRM_BYTES) return 'confirm'
  return 'silent'
}
