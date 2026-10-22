/**
 * What a `bind:this` on a `ToggleGroup` can call. In a `.ts` file, since the typed lint can't
 * resolve a type exported from a `.svelte` module script into a call.
 */
export interface ToggleGroupAPI {
  /** Focuses the active cell (or the first enabled one); `false` when none took focus. */
  focus: () => boolean
}
