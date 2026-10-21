/**
 * Where the range editor gets its examples: the sheet's state, handed down through Svelte context
 * so `MaskInput` and its token editors stay free of the session. Without one (a field outside the
 * sheet) the editor has no files to show.
 */
import { getContext, setContext } from 'svelte'
import type { RangeSample } from '#lib/tauri-commands/index.js'
import type { SampleRows } from './multi-rename-state.svelte'

export interface RangeSampleSource {
  /** The session rows to show (the list cursor's and up to four around it), or `null` with none yet. */
  readonly sampleRows: SampleRows | null
  /** What range `token` (`N2-5`) takes from the files at `rows`, by the engine, or `null` when it can't say. */
  rangeSamples: (token: string, rows: number[]) => Promise<RangeSample[] | null>
}

const KEY = Symbol('multi-rename-range-sample-source')

/** Call during the sheet's init: every range editor under it asks `source`. */
export function provideRangeSampleSource(source: RangeSampleSource): void {
  setContext(KEY, source)
}

export function rangeSampleSource(): RangeSampleSource | undefined {
  return getContext<RangeSampleSource | undefined>(KEY)
}
