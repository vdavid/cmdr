/**
 * Index↔value mapping for a slider whose `constraints.stopsAreDiscrete` is set: the track runs
 * over the stops' INDICES, while the store keeps the stop's own value.
 *
 * Why index space at all: a range whose ends are orders of magnitude apart (the Ask Cmdr wake
 * cadence runs 5 seconds to 2 hours) puts its first stops inside a single pixel on a linear
 * track. Over indices every stop gets equal travel, and `ui/Slider`'s `positionOf`, ticks, and
 * snap targets are all linear over min/max, so they're already correct there.
 *
 * ❌ **The INDEX is never stored.** Reordering or inserting a stop would then silently change
 * what every user chose. The pair below is the seam that keeps the store in value space.
 */

/**
 * The index of the stop nearest `value`, ties going to the shorter one.
 *
 * ❌ Never `stops.indexOf(value)`: a value that isn't in the table (a hand-edited settings
 * file, or a stop a later build retired) yields `-1`, which reads as the first stop while the
 * store still holds the old number — the control and the setting then disagree silently.
 *
 * @param stops the stop table, in track order
 * @param value the stored value to place
 * @returns an index into `stops`, or `0` for an empty table
 */
export function nearestStopIndex(stops: readonly number[], value: number): number {
  let nearest = 0
  for (let index = 1; index < stops.length; index++) {
    if (Math.abs(stops[index] - value) < Math.abs(stops[nearest] - value)) nearest = index
  }
  return nearest
}

/**
 * The stop a track position lands on, rounded and clamped into the table.
 *
 * @param stops the stop table, in track order
 * @param track the slider's current position, in index space
 * @returns the stop's value, ready to store
 */
export function stopAt(stops: readonly number[], track: number): number {
  const index = Math.min(Math.max(Math.round(track), 0), stops.length - 1)
  return stops[index]
}
