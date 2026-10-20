/**
 * The `tags-skipped` warning: a color-tag toggle (keyboard command or context-menu
 * circle) left some files alone because Cmdr couldn't read the tags they already
 * carry. Writing them anyway would have replaced those tags with the one color.
 */
import { addToast } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

/** Dedup id: a second skip replaces the first toast rather than stacking. */
export const TAGS_SKIPPED_TOAST_ID = 'tags-skipped'

/** Handles the `tags-skipped` event from Rust (`file_system::tags::report_skips`). */
export function handleTagsSkipped(count: number): void {
  addToast(tString('fileExplorer.tags.skipped', { count, countText: formatInteger(count) }), {
    id: TAGS_SKIPPED_TOAST_ID,
    level: 'warn',
  })
}
