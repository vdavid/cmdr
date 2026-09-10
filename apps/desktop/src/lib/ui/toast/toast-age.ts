/**
 * The "2m ago" label a toast wears once it has been up for a minute, and when that label next
 * changes.
 *
 * Nothing under a minute, on purpose: a seconds count would tick on a surface the eye keeps
 * catching, and most toasts are gone within four seconds anyway. From a minute up the label
 * counts whole minutes, then whole hours, always rounded down, so it never claims a toast is
 * older than it is.
 */

import { tString } from '$lib/intl/messages.svelte'
import { formatDuration, seconds } from '$lib/units'

const MINUTE_MS = 60_000
const HOUR_MS = 60 * MINUTE_MS

/** The unit the label counts in at this age: minutes under an hour, hours from then on. */
function unitMsAt(ageMs: number): number {
  return ageMs < HOUR_MS ? MINUTE_MS : HOUR_MS
}

/** "2m ago" / "1h ago" for a toast `ageMs` old, or `null` while it's under a minute old. */
export function formatToastAge(ageMs: number): string | null {
  if (ageMs < MINUTE_MS) return null
  const unitMs = unitMsAt(ageMs)
  const flooredMs = Math.floor(ageMs / unitMs) * unitMs
  return tString('ui.toast.age', { durationText: formatDuration(seconds(flooredMs / 1000)) })
}

/**
 * How long until {@link formatToastAge} returns something different for a toast that's `ageMs`
 * old now, so the toast sleeps exactly that long instead of polling.
 */
export function msUntilToastAgeChanges(ageMs: number): number {
  if (ageMs < MINUTE_MS) return MINUTE_MS - ageMs
  const unitMs = unitMsAt(ageMs)
  return (Math.floor(ageMs / unitMs) + 1) * unitMs - ageMs
}
