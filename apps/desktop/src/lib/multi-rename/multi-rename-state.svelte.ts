/**
 * The Multi-Rename sheet's state: the spec the fields edit, the live preview, the
 * window of rows the list draws, the presets (and which one the fields came from),
 * and Start. One instance per open sheet, over one backend session. It opens on the
 * settings (and the preset) the last sheet closed with (`persist`), as TC does; an
 * edit made before they arrive wins.
 *
 * The files and their names live in the backend session (`src-tauri/src/multi_rename/session.rs`):
 * a preview answers its id, the counts, and the first rows, and the list pages in
 * the rows it scrolls to (`show`, which `source` hands `ColumnList`), from every row
 * or from the problem rows alone. Start sends the session and preview ids, never names.
 *
 * The preview reruns 120 ms after the last edit, and a generation counter drops
 * an answer a newer edit overtook, so fast typing never shows an older preview.
 */

import {
  applyMultiRename,
  deleteMultiRenamePreset,
  getMultiRenameLastSettings,
  getMultiRenamePresets,
  getMultiRenamePreviewRows,
  previewMultiRename,
  renameMultiRenamePreset,
  saveMultiRenameLastSettings,
  saveMultiRenamePreset,
  updateMultiRenamePreset,
  type LoadedPreset,
  type MultiRenameError,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewCounts,
  type PreviewFilter,
  type PreviewRow,
} from '$lib/tauri-commands'
import type { ColumnListWindowedSource } from '$lib/ui/column-list-types'
import { SvelteMap } from 'svelte/reactivity'
import { BUILT_IN_PRESETS, DEFAULT_SPEC, specsEqual } from './spec'

export const PREVIEW_DELAY_MS = 120

/** The most rows one page request asks for (the backend's `MAX_PAGE`). */
const PAGE_LIMIT = 1000

/** Rows held past this many are dropped when they're far from the window. */
const HELD_ROWS = 3000

const NO_COUNTS: PreviewCounts = { ready: 0, unchanged: 0, problems: 0 }

/** The preset the fields were last loaded from (or saved as): a saved one, or one that ships with Cmdr. */
export type { LoadedPreset }

/** How the backend tells two preset names apart (`presets.rs`'s `dedupe_key`). */
const nameKey = (name: string): string => name.trim().toLowerCase()

/** Rows `start..end` of the preview, end exclusive. */
export interface RowWindow {
  start: number
  end: number
}

export interface MultiRenameState {
  readonly spec: MultiRenameSpec
  readonly counts: PreviewCounts
  /** How many rows the list shows: the preview's, or its problem rows' with `problemsOnly`. */
  readonly total: number
  /** The list shows the problem rows alone. */
  readonly problemsOnly: boolean
  /** The rows as `ColumnList` reads them: `total`, `rowAt`, and `show`. */
  readonly source: ColumnListWindowedSource<PreviewRow>
  /** Why the preview has no answer: a bad mask or regex, a gone listing, a timeout. */
  readonly error: MultiRenameError | null
  /** Why the last Start didn't run. Cleared by the next edit or preview; doesn't block a retry. */
  readonly applyError: MultiRenameError | null
  /** An edit the preview hasn't caught up with yet: Start waits, so it never runs a spec nobody saw. */
  readonly pending: boolean
  readonly presets: MultiRenamePreset[]
  /** The preset the fields came from, or `null` (nothing loaded, Reset all fields, or it was deleted). */
  readonly loaded: LoadedPreset | null
  /** A field differs from the loaded preset. Always `false` with nothing loaded. */
  readonly edited: boolean
  readonly applying: boolean
  /** Row `index` of the list (of the problem rows, with `problemsOnly`), or `undefined` while its page is on its way. */
  rowAt: (index: number) => PreviewRow | undefined
  /** The list shows rows `start..end`: fetch the ones missing, drop the far ones. */
  show: (window: RowWindow) => void
  /** Lists the problem rows alone, or every row again. Pages them in from the same preview. */
  setProblemsOnly: (on: boolean) => void
  update: (patch: Partial<MultiRenameSpec>) => void
  /** Fills every field from a preset (doesn't start anything). An unknown one is ignored. */
  loadPreset: (preset: LoadedPreset) => void
  /** Total Commander's `<Default>`: every field back to "no change", nothing loaded. */
  resetFields: () => void
  loadPresets: () => Promise<void>
  /** The saved preset the backend would treat as `name` (trimmed, any case), if any. */
  presetNamed: (name: string) => MultiRenamePreset | undefined
  /** Saves the fields under `name`, replacing a preset with that name; it becomes the loaded one. */
  savePreset: (name: string) => Promise<void>
  /** Renames a preset in place; a preset with that name is replaced. */
  renamePreset: (args: { id: string; name: string }) => Promise<void>
  /** Gives a preset the current fields; it becomes the loaded one. */
  updatePreset: (id: string) => Promise<void>
  deletePreset: (id: string) => Promise<void>
  /** Remembers the fields and the loaded preset for the next sheet. */
  persist: () => Promise<void>
  /** Starts the rename. Resolves with the operation, or `null` when it didn't start (`applyError` says why). */
  apply: () => Promise<MultiRenameStarted | null>
  dispose: () => void
}

export function createMultiRenameState(sessionId: string): MultiRenameState {
  let spec = $state<MultiRenameSpec>({ ...DEFAULT_SPEC })
  let previewId = $state<number | null>(null)
  let counts = $state.raw<PreviewCounts>(NO_COUNTS)
  let problemsOnly = $state(false)
  // Keyed by place in the list (every row, or the problem rows alone). Replaced with
  // each preview and each switch of the list; pages land in it as they arrive.
  let rows = $state.raw(new SvelteMap<number, PreviewRow>())
  let error = $state<MultiRenameError | null>(null)
  let presets = $state.raw<MultiRenamePreset[]>([])
  let loaded = $state.raw<LoadedPreset | null>(null)
  let applying = $state(false)
  let applyError = $state<MultiRenameError | null>(null)
  let waiting = $state(0)
  let generation = 0
  let timer: ReturnType<typeof setTimeout> | null = null
  let shown: RowWindow = { start: 0, end: 0 }
  let fetching: string | null = null
  // An edit made before the last settings arrive wins over them.
  let touched = false

  const totalOf = (c: PreviewCounts): number => c.ready + c.unchanged + c.problems
  const filterOf = (): PreviewFilter => (problemsOnly ? 'problems' : 'all')
  const listed = (): number => (problemsOnly ? counts.problems : totalOf(counts))

  function specOf(preset: LoadedPreset): MultiRenameSpec | undefined {
    const from = preset.kind === 'saved' ? presets : BUILT_IN_PRESETS
    return from.find((p) => p.id === preset.id)?.spec
  }

  function presetNamed(name: string): MultiRenamePreset | undefined {
    return presets.find((p) => nameKey(p.name) === nameKey(name))
  }

  // Read from the list, so a renamed, updated, or deleted preset is followed with no syncing.
  const loadedSpec = $derived(loaded ? specOf(loaded) : undefined)

  async function refresh(): Promise<void> {
    const asked = ++generation
    waiting++
    let answer
    try {
      answer = await previewMultiRename(sessionId, $state.snapshot(spec))
    } finally {
      waiting--
    }
    if (asked !== generation) return
    if (answer.ok) {
      previewId = answer.value.previewId
      counts = answer.value.counts
      // With no problems left there's nothing to list alone: every row again.
      if (counts.problems === 0) problemsOnly = false
      // The first page is every row's; a list of problems pages its own in.
      rows = problemsOnly ? new SvelteMap() : new SvelteMap(answer.value.rows.map((row) => [row.row, row]))
      error = null
      show(shown)
    } else {
      // A spec problem keeps the last good preview on screen under the error.
      error = answer.error
      if (answer.error.type !== 'spec') {
        previewId = null
        counts = NO_COUNTS
        problemsOnly = false
        rows = new SvelteMap()
      }
    }
  }

  function show(window: RowWindow): void {
    shown = window
    const { start, end } = window
    const id = previewId
    if (id === null) return
    const filter = filterOf()
    const last = Math.min(end, listed())
    let from = start
    while (from < last && rows.has(from)) from++
    if (from >= last) return
    let to = last
    while (to > from && rows.has(to - 1)) to--
    const limit = Math.min(to - from, PAGE_LIMIT)
    const key = `${String(id)}:${filter}:${String(from)}:${String(limit)}`
    if (fetching === key) return
    fetching = key
    const into = rows
    void getMultiRenamePreviewRows(sessionId, id, from, limit, filter).then((answer) => {
      if (fetching === key) fetching = null
      // A newer preview or the other list replaced this one: its own rows are on their way.
      if (!answer.ok || previewId !== id || rows !== into) return
      answer.value.forEach((row, i) => rows.set(from + i, row))
      if (rows.size > HELD_ROWS) {
        const keepFrom = shown.start - PAGE_LIMIT
        const keepTo = shown.end + PAGE_LIMIT
        const far = [...rows.keys()].filter((index) => index < keepFrom || index >= keepTo)
        for (const index of far) rows.delete(index)
      }
      show(shown)
    })
  }

  function schedule(): void {
    applyError = null
    if (timer === null) waiting++
    else clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      waiting--
      void refresh()
    }, PREVIEW_DELAY_MS)
  }

  // The first preview, on the settings the last sheet closed with (or the defaults).
  async function start(): Promise<void> {
    let last = null
    try {
      last = await getMultiRenameLastSettings()
    } catch {
      // Nothing remembered: the defaults.
    }
    if (last && !touched) {
      spec = { ...DEFAULT_SPEC, ...last.spec }
      loaded = last.preset ?? null
    }
    await refresh()
  }
  void start()

  return {
    get spec() {
      return spec
    },
    get counts() {
      return counts
    },
    get total() {
      return listed()
    },
    get problemsOnly() {
      return problemsOnly
    },
    source: {
      get count() {
        return listed()
      },
      getRow: (index) => rows.get(index),
      onRangeChange: (range) => {
        show(range)
      },
    },
    get error() {
      return error
    },
    get presets() {
      return presets
    },
    get loaded() {
      return loadedSpec ? loaded : null
    },
    get edited() {
      return loadedSpec !== undefined && !specsEqual(spec, loadedSpec)
    },
    get applying() {
      return applying
    },
    get applyError() {
      return applyError
    },
    get pending() {
      return waiting > 0
    },
    rowAt(index) {
      return rows.get(index)
    },
    show,
    setProblemsOnly(on) {
      if (on === problemsOnly) return
      problemsOnly = on
      rows = new SvelteMap()
      fetching = null
      show(shown)
    },
    update(patch) {
      touched = true
      spec = { ...spec, ...patch }
      schedule()
    },
    loadPreset(preset) {
      const next = specOf(preset)
      if (!next) return
      touched = true
      loaded = preset
      spec = { ...next }
      schedule()
    },
    resetFields() {
      touched = true
      loaded = null
      spec = { ...DEFAULT_SPEC }
      schedule()
    },
    async loadPresets() {
      presets = await getMultiRenamePresets()
    },
    presetNamed,
    async savePreset(name) {
      const trimmed = name.trim()
      if (trimmed === '') return
      const id = presetNamed(trimmed)?.id ?? crypto.randomUUID()
      await saveMultiRenamePreset({ id, name: trimmed, spec: $state.snapshot(spec) })
      presets = await getMultiRenamePresets()
      loaded = { kind: 'saved', id }
    },
    async renamePreset({ id, name }) {
      const trimmed = name.trim()
      if (trimmed === '') return
      await renameMultiRenamePreset(id, trimmed)
      presets = await getMultiRenamePresets()
    },
    async updatePreset(id) {
      await updateMultiRenamePreset(id, $state.snapshot(spec))
      presets = await getMultiRenamePresets()
      loaded = { kind: 'saved', id }
    },
    async deletePreset(id) {
      await deleteMultiRenamePreset(id)
      presets = await getMultiRenamePresets()
      if (loaded?.kind === 'saved' && loaded.id === id) loaded = null
    },
    async persist() {
      // A preset deleted meanwhile isn't remembered: `loaded` reads it from the list.
      await saveMultiRenameLastSettings($state.snapshot(spec), loadedSpec ? loaded : null)
    },
    async apply() {
      const id = previewId
      if (applying || waiting > 0 || id === null) return null
      applying = true
      try {
        // The preview the user saw: the backend renames only if it still computes exactly its ready rows.
        const answer = await applyMultiRename(sessionId, id)
        if (answer.ok) return answer.value
        applyError = answer.error
        // The folder moved under the preview: show it as it is now.
        if (answer.error.type === 'previewOutOfDate') void refresh()
        return null
      } finally {
        applying = false
      }
    },
    dispose() {
      if (timer !== null) clearTimeout(timer)
      generation++
    },
  }
}
