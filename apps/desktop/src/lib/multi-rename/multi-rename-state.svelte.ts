/**
 * The Multi-Rename sheet's state: the spec the fields edit, the live preview, the
 * window of rows the table draws, the presets, and Start. One instance per open
 * sheet, over one backend session.
 *
 * The files and their names live in the backend session (`src-tauri/src/multi_rename/session.rs`):
 * a preview answers its id, the counts, and the first rows, and the table pages in
 * the rows it scrolls to (`show`). Start sends the session and preview ids, never names.
 *
 * The preview reruns 120 ms after the last edit, and a generation counter drops
 * an answer a newer edit overtook, so fast typing never shows an older preview.
 */

import {
  applyMultiRename,
  deleteMultiRenamePreset,
  getMultiRenamePresets,
  getMultiRenamePreviewRows,
  previewMultiRename,
  saveMultiRenamePreset,
  type MultiRenameError,
  type MultiRenamePreset,
  type MultiRenameSpec,
  type MultiRenameStarted,
  type PreviewCounts,
  type PreviewRow,
} from '$lib/tauri-commands'
import { SvelteMap } from 'svelte/reactivity'
import { DEFAULT_SPEC } from './spec'

export const PREVIEW_DELAY_MS = 120

/** The most rows one page request asks for (the backend's `MAX_PAGE`). */
const PAGE_LIMIT = 1000

/** Rows held past this many are dropped when they're far from the window. */
const HELD_ROWS = 3000

const NO_COUNTS: PreviewCounts = { ready: 0, unchanged: 0, problems: 0 }

/** Rows `start..end` of the preview, end exclusive. */
export interface RowWindow {
  start: number
  end: number
}

export interface MultiRenameState {
  readonly spec: MultiRenameSpec
  readonly counts: PreviewCounts
  /** How many rows the preview has: the table's height. */
  readonly total: number
  /** Why the preview has no answer: a bad mask or regex, a gone listing, a timeout. */
  readonly error: MultiRenameError | null
  /** Why the last Start didn't run. Cleared by the next edit or preview; doesn't block a retry. */
  readonly applyError: MultiRenameError | null
  /** An edit the preview hasn't caught up with yet: Start waits, so it never runs a spec nobody saw. */
  readonly pending: boolean
  readonly presets: MultiRenamePreset[]
  readonly applying: boolean
  /** Row `index` of the preview, or `undefined` while its page is on its way. */
  rowAt: (index: number) => PreviewRow | undefined
  /** The table shows rows `start..end`: fetch the ones missing, drop the far ones. */
  show: (window: RowWindow) => void
  update: (patch: Partial<MultiRenameSpec>) => void
  /** Replaces the whole spec (loading a preset). */
  load: (spec: MultiRenameSpec) => void
  loadPresets: () => Promise<void>
  savePreset: (name: string) => Promise<void>
  deletePreset: (id: string) => Promise<void>
  /** Starts the rename. Resolves with the operation, or `null` when it didn't start (`applyError` says why). */
  apply: () => Promise<MultiRenameStarted | null>
  dispose: () => void
}

export function createMultiRenameState(sessionId: string): MultiRenameState {
  let spec = $state<MultiRenameSpec>({ ...DEFAULT_SPEC })
  let previewId = $state<number | null>(null)
  let counts = $state.raw<PreviewCounts>(NO_COUNTS)
  // Replaced with each preview; pages land in it as they arrive.
  let rows = $state.raw(new SvelteMap<number, PreviewRow>())
  let error = $state<MultiRenameError | null>(null)
  let presets = $state.raw<MultiRenamePreset[]>([])
  let applying = $state(false)
  let applyError = $state<MultiRenameError | null>(null)
  let waiting = $state(0)
  let generation = 0
  let timer: ReturnType<typeof setTimeout> | null = null
  let shown: RowWindow = { start: 0, end: 0 }
  let fetching: string | null = null

  const totalOf = (c: PreviewCounts): number => c.ready + c.unchanged + c.problems

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
      rows = new SvelteMap(answer.value.rows.map((row) => [row.row, row]))
      error = null
      show(shown)
    } else {
      // A spec problem keeps the last good preview on screen under the error.
      error = answer.error
      if (answer.error.type !== 'spec') {
        previewId = null
        counts = NO_COUNTS
        rows = new SvelteMap()
      }
    }
  }

  function show(window: RowWindow): void {
    shown = window
    const { start, end } = window
    const id = previewId
    if (id === null) return
    const last = Math.min(end, totalOf(counts))
    let from = start
    while (from < last && rows.has(from)) from++
    if (from >= last) return
    let to = last
    while (to > from && rows.has(to - 1)) to--
    const limit = Math.min(to - from, PAGE_LIMIT)
    const key = `${String(id)}:${String(from)}:${String(limit)}`
    if (fetching === key) return
    fetching = key
    void getMultiRenamePreviewRows(sessionId, id, from, limit).then((answer) => {
      if (fetching === key) fetching = null
      // A newer preview replaced this one: its own rows are on their way.
      if (!answer.ok || previewId !== id) return
      for (const row of answer.value) rows.set(row.row, row)
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

  // The first preview: the default spec shows every name as it is.
  void refresh()

  return {
    get spec() {
      return spec
    },
    get counts() {
      return counts
    },
    get total() {
      return totalOf(counts)
    },
    get error() {
      return error
    },
    get presets() {
      return presets
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
    update(patch) {
      spec = { ...spec, ...patch }
      schedule()
    },
    load(next) {
      spec = { ...next }
      schedule()
    },
    async loadPresets() {
      presets = await getMultiRenamePresets()
    },
    async savePreset(name) {
      const trimmed = name.trim()
      if (trimmed === '') return
      const existing = presets.find((p) => p.name.trim().toLowerCase() === trimmed.toLowerCase())
      await saveMultiRenamePreset({
        id: existing?.id ?? crypto.randomUUID(),
        name: trimmed,
        spec: $state.snapshot(spec),
      })
      presets = await getMultiRenamePresets()
    },
    async deletePreset(id) {
      await deleteMultiRenamePreset(id)
      presets = await getMultiRenamePresets()
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
