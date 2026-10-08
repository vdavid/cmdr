/**
 * Three-band copy policy for the viewer's selection-to-clipboard flow.
 *
 * The user need is mundane: most copies are tiny and should land silently. A copy of
 * 10 MB is unusual enough to deserve a confirm; over 100 MB risks freezing the
 * downstream app's paste handler, so we refuse and offer a save-as alternative.
 *
 * Thresholds are fixed DECIMAL bytes, because `viewer.copyDialog.refuseBody` names
 * "100 MB" in its copy and the symbol has to match the base. The sizes shown in
 * dialogs/toasts go through `formatByteSize()` (`$lib/units`), which honours
 * `appearance.fileSizeFormat`.
 */

/** 10 MB. At this size, paste in most apps is still smooth, but we ask the user. */
export const COPY_CONFIRM_BYTES = 10_000_000

/** 100 MB, the limit `viewer.copyDialog.refuseBody` names. Above this we refuse the direct copy and steer to save-as. */
export const COPY_REFUSE_BYTES = 100_000_000

export type CopyAction = 'silent' | 'confirm' | 'refuse'

/**
 * Picks the right copy band for `bytes`. Boundary semantics:
 *
 * - `< 10 MB` → `silent`
 * - `[10 MB, 100 MB)` → `confirm`
 * - `>= 100 MB` → `refuse`
 *
 * Negative inputs (impossible but defended) are treated as `silent`.
 */
export function selectCopyAction(bytes: number): CopyAction {
  if (bytes >= COPY_REFUSE_BYTES) return 'refuse'
  if (bytes >= COPY_CONFIRM_BYTES) return 'confirm'
  return 'silent'
}
