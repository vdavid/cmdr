import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { SpaceInfo } from '$lib/ipc/bindings'

type FormatSize = (bytes: number) => string

/** The bounded half of `SpaceInfo`: the only shape a percentage means anything on. */
export type BoundedSpaceInfo = Extract<SpaceInfo, { kind: 'bounded' }>

/**
 * How full a volume is. The band carries no copy at all, which is the point:
 * `severity` is what the tooltip branches on, so there's nothing here a
 * user-facing string could be matched against. The bar itself needs only the
 * colour; the two low-space bands additionally earn a sentence in the tooltip.
 */
export interface DiskUsageLevel {
  severity: 'ok' | 'warning' | 'critical'
  cssVar: string
}

/** Returns the usage band for the bar color and the tooltip's warning sentence. */
export function getDiskUsageLevel(usedPercent: number): DiskUsageLevel {
  if (usedPercent >= 95) return { severity: 'critical', cssVar: '--color-disk-danger' }
  if (usedPercent >= 80) return { severity: 'warning', cssVar: '--color-disk-warning' }
  return { severity: 'ok', cssVar: '--color-disk-ok' }
}

/**
 * Returns used percentage (0–100), clamped.
 *
 * ❗ Takes the BOUNDED shape only, and that's the guardrail: storage with no
 * ceiling has no denominator, so a percentage of it would be invented. Callers
 * that hold a plain `SpaceInfo` go through `getUsageBar`, which answers `null`
 * for the unbounded case instead of a number.
 */
export function getUsedPercent(space: BoundedSpaceInfo): number {
  if (space.totalBytes <= 0) return 0
  const used = space.totalBytes - space.availableBytes
  return Math.max(0, Math.min(100, Math.round((used / space.totalBytes) * 100)))
}

/**
 * What to draw the usage bar with, or `null` when there is no bar to draw.
 *
 * `null` means the volume has no ceiling: ❌ no fill at an invented width and
 * ❌ no severity band, because you can't run out of storage that has no limit.
 * Every surface that renders the bar goes through here, so none of them can
 * grow its own idea of what an unbounded volume looks like.
 */
export function getUsageBar(space: SpaceInfo): (DiskUsageLevel & { usedPercent: number }) | null {
  if (space.kind !== 'bounded') return null
  const usedPercent = getUsedPercent(space)
  return { usedPercent, ...getDiskUsageLevel(usedPercent) }
}

/** Free percentage (0–100), clamped, as the catalog's preformatted string param. */
function freePercentText(space: BoundedSpaceInfo): string {
  const freePercent = Math.max(0, Math.min(100, Math.round((space.availableBytes / space.totalBytes) * 100)))
  return formatInteger(freePercent)
}

/**
 * Formats the status bar text: "420 GB of 1 TB free (42%)", or "64 MB used"
 * where there is no ceiling and so no free figure to state.
 */
export function formatDiskSpaceStatus(space: SpaceInfo, formatSize: FormatSize): string {
  if (space.kind !== 'bounded') {
    return tString('fileExplorer.diskSpace.used', { usedText: formatSize(space.usedBytes) })
  }
  return tString('fileExplorer.diskSpace.free', {
    freeText: formatSize(space.availableBytes),
    totalText: formatSize(space.totalBytes),
    percentText: freePercentText(space),
  })
}

/**
 * Formats the short volume selector text: "420 GB free of 1 TB", or "64 MB used"
 * where there is no ceiling. The unbounded line is already short, so it's the
 * same sentence the status bar shows.
 */
export function formatDiskSpaceShort(space: SpaceInfo, formatSize: FormatSize): string {
  if (space.kind !== 'bounded') {
    return tString('fileExplorer.diskSpace.used', { usedText: formatSize(space.usedBytes) })
  }
  return tString('fileExplorer.diskSpace.freeShort', {
    freeText: formatSize(space.availableBytes),
    totalText: formatSize(space.totalBytes),
  })
}

/**
 * The sentences that belong after the figures, joined, or `''` when there are
 * none. Shared by the usage bar's tooltip and the status-bar text's, so the two
 * can't disagree about when a volume earns a warning.
 *
 * ❗ On storage with no ceiling the low-space notes CANNOT fire: there's no
 * percentage to band, and telling someone an unlimited account is 95% full is a
 * lie. It gets a note saying why there's no bar instead.
 *
 * `mtpHint` carries the phone-storage explanation (resolved from the message
 * catalog by the caller) for MTP volumes, where the browsable folders add up to
 * less than the used space because apps and system data aren't reachable over
 * USB.
 */
export function formatSpaceNotes(space: SpaceInfo, mtpHint?: string): string {
  const bar = getUsageBar(space)
  const notes: string[] = []
  if (bar === null) notes.push(tString('fileExplorer.diskSpace.noLimitNote'))
  else if (bar.severity === 'critical') notes.push(tString('fileExplorer.diskSpace.lowNote'))
  else if (bar.severity === 'warning') notes.push(tString('fileExplorer.diskSpace.somewhatLowNote'))
  if (mtpHint) notes.push(mtpHint)
  return notes.join(' ')
}

/**
 * Formats the usage bar tooltip: the figures, then whatever
 * [`formatSpaceNotes`] has to add.
 *
 * The figures and the notes after them are separate sentences, so the catalog
 * owns how they're joined: a language that ends a sentence with something other
 * than `. ` gets to say so.
 */
export function formatBarTooltip(space: SpaceInfo, formatSize: FormatSize, mtpHint?: string): string {
  const notes = formatSpaceNotes(space, mtpHint)
  return tString('fileExplorer.diskSpace.barTooltip', {
    hasNotes: notes ? 'yes' : 'no',
    sizes: formatDiskSpaceStatus(space, formatSize),
    notes,
  })
}
