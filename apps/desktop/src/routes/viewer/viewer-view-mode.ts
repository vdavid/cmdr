export type ViewerDisplayMode = 'text' | 'binary' | 'hex' | 'media'

/** Lister-compatible mode keys. A modified digit belongs to the focused control or OS. */
export function modeForKey(event: KeyboardEvent, mediaAvailable: boolean): ViewerDisplayMode | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || event.isComposing) return null
  switch (event.key) {
    case '0':
      return mediaAvailable ? 'media' : null
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
