export type ViewerDisplayMode = 'text' | 'binary' | 'hex' | 'media'

/** Lister-compatible mode keys. A modified digit belongs to the focused control or OS. */
export function modeForKey(event: KeyboardEvent): ViewerDisplayMode | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || event.isComposing) return null
  switch (event.key) {
    case '1':
      return 'text'
    case '2':
      return 'binary'
    case '3':
      return 'hex'
    default:
      return null
  }
}
