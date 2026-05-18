/**
 * Keyboard-navigation index resolver for the swatch grid in
 * `SettingColorSwatchPicker.svelte`.
 *
 * Returns the next focus index given a key press, or `null` when the key
 * doesn't move focus. Extracted into a pure module so the grid-traversal
 * arithmetic can be unit-tested without driving DOM events.
 */
export function nextSwatchIndex(key: string, currentIndex: number, total: number, colsPerRow: number): number | null {
  if (total === 0) return null
  if (key === 'ArrowRight') return Math.min(currentIndex + 1, total - 1)
  if (key === 'ArrowLeft') return Math.max(currentIndex - 1, 0)
  if (key === 'ArrowDown') return Math.min(currentIndex + colsPerRow, total - 1)
  if (key === 'ArrowUp') return Math.max(currentIndex - colsPerRow, 0)
  if (key === 'Home') return 0
  if (key === 'End') return total - 1
  return null
}
