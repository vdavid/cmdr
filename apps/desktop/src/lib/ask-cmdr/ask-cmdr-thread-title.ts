/**
 * The title a thread row shows. Usually the stored title, but a wake thread the user hasn't
 * renamed is stored under its busiest folder's bare name (`veszelovszki`, `tmp`), which says
 * nothing about the thread. The backend hands over that folder as `wakePlace` exactly while the
 * stored title is still the wake's own, and the row says "Activity in ~/tmp" instead, in the
 * user's language (`agent/store/wake_place.rs`).
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { WakePlace } from '#lib/tauri-commands/index.js'

export function threadDisplayTitle(thread: { title: string; wakePlace: WakePlace | null }): string {
  const place = thread.wakePlace
  if (!place) return thread.title
  return tString('askCmdr.sessions.wakeTitle', {
    folder: place.folder,
    otherCount: place.otherFolders,
    otherCountText: formatInteger(place.otherFolders),
  })
}
